#!/usr/bin/env python3
"""
Analyze CI build step timings and model the wall-clock impact of different
job layouts.

Usage:
    python3 scripts/ci-timing-analysis.py [--run-ids ID1 ID2 ...] [--limit N]

If run IDs are not provided, the script finds recent successful runs on main
automatically. Requires `gh` CLI to be authenticated.
"""
import argparse
import json
import math
import re
import subprocess
import sys
from collections import defaultdict
from dataclasses import dataclass, field
from datetime import datetime


WORKFLOW = "ci.yml"
JOB_NAMES = ["rust", "rust-lint", "rust-test"]

# Canonical step names, matched by prefix against the cargo command
STEP_KEYS = [
    ("fmt",            "cargo fmt"),
    ("proto_gen",      "cargo run --bin foxglove_proto_gen"),
    ("clippy",         "cargo clippy"),
    ("build",          "cargo build --"),  # matches "cargo build --verbose" but not "cargo build -p"
    ("build_examples", "set -euo"),
    ("build_no_def",   "cargo build -p foxglove"),
    ("msrv",           "cargo +1.83.0"),
    ("nightly_doc",    "cargo +nightly"),
    ("test_all",       "cargo test --all"),
    ("test_no_def",    "cargo test -p foxglove"),
]


def gh(*args):
    r = subprocess.run(
        ["gh"] + list(args),
        capture_output=True, text=True, cwd="/workspace",
    )
    if r.returncode != 0:
        return None
    return r.stdout.strip()


def find_run_ids(limit):
    raw = gh(
        "run", "list", "--workflow", WORKFLOW, "--branch", "main",
        "--limit", str(limit * 3),
        "--json", "databaseId,conclusion",
    )
    if not raw:
        return []
    runs = json.loads(raw)
    return [r["databaseId"] for r in runs if r["conclusion"] == "success"][:limit]


def get_job_id(run_id, job_name):
    raw = gh(
        "run", "view", str(run_id), "--json", "jobs", "--jq",
        f'.jobs[] | select(.name=="{job_name}") | .databaseId',
    )
    return raw if raw else None


def parse_job_log(run_id, job_name):
    job_id = get_job_id(run_id, job_name)
    if not job_id:
        return None, None

    raw = gh("run", "view", str(run_id), "--log", "--job", job_id)
    if not raw:
        return None, None

    ts_re = re.compile(r"(\d{4}-\d{2}-\d{2}T[\d:.]+)Z")

    cache_hit = None
    if "No cache found" in raw:
        cache_hit = False
    elif "Cache restored successfully" in raw and "rust-cache" in raw.lower():
        cache_hit = True

    entries = []
    for line in raw.split("\n"):
        m = ts_re.search(line)
        if not m:
            continue
        ts = datetime.fromisoformat(m.group(1))
        if "##[group]Run " in line:
            step_name = line.split("##[group]Run ", 1)[-1].strip()[:120]
            entries.append((ts, step_name))

    steps = {}
    for i in range(len(entries)):
        name = entries[i][1]
        start = entries[i][0]
        end = entries[i + 1][0] if i + 1 < len(entries) else start
        duration = (end - start).total_seconds()
        steps[name] = duration

    return steps, cache_hit


def match_step(raw_name, key_prefix):
    return raw_name.strip().startswith(key_prefix)


def extract_timings(run_id):
    """Extract canonical step timings and setup overhead from a run."""
    all_raw_steps = {}
    cache_hit = None

    for job_name in JOB_NAMES:
        raw, hit = parse_job_log(run_id, job_name)
        if raw:
            all_raw_steps.update(raw)
            if hit is not None:
                cache_hit = hit

    if not all_raw_steps:
        return None

    # Match raw step names to canonical keys
    matched = {}
    setup_total = 0.0

    for raw_name, duration in all_raw_steps.items():
        found = False
        for key, prefix in STEP_KEYS:
            if match_step(raw_name, prefix):
                matched[key] = duration
                found = True
                break

        if not found:
            rn = raw_name.lower()
            is_setup = any(kw in rn for kw in [
                "checkout", "setup-", "common-deps", "protoc", "rust-cache",
                "rust-toolchain", "rustup", "corepack", "actions/",
                "apt-get", "sudo ", "swatinem", "not all versions",
                "construct rustup", "add-matcher", "command -v rustup",
                "grep -r", "! grep",
            ])
            if is_setup:
                setup_total += duration

    matched["setup"] = setup_total
    matched["cache_hit"] = cache_hit
    matched["run_id"] = run_id
    return matched


def fmt_dur(seconds):
    if seconds >= 60:
        return f"{int(seconds // 60)}m {seconds % 60:.0f}s"
    return f"{seconds:.1f}s"


def mean(values):
    return sum(values) / len(values) if values else 0


def stdev(values):
    if len(values) < 2:
        return 0
    m = mean(values)
    return math.sqrt(sum((x - m) ** 2 for x in values) / (len(values) - 1))


def fmt_stat(values):
    m = mean(values)
    s = stdev(values)
    return f"{m:.1f}s ± {s:.1f}s"


def main():
    parser = argparse.ArgumentParser(description="Analyze CI timing across multiple runs")
    parser.add_argument("--run-ids", type=int, nargs="+", help="Specific run IDs to analyze")
    parser.add_argument("--limit", type=int, default=8, help="Number of recent runs to fetch (default: 8)")
    args = parser.parse_args()

    if args.run_ids:
        run_ids = args.run_ids
    else:
        print(f"Finding up to {args.limit} recent successful CI runs on main...\n")
        run_ids = find_run_ids(args.limit)

    if not run_ids:
        print("No runs found.", file=sys.stderr)
        sys.exit(1)

    print(f"Analyzing {len(run_ids)} runs: {run_ids}\n")

    # Collect timings from all runs
    all_timings = []
    for rid in run_ids:
        t = extract_timings(rid)
        if t:
            all_timings.append(t)
            status = "HIT" if t["cache_hit"] else "MISS" if t["cache_hit"] is False else "?"
            print(f"  run {rid}: cache {status}")
        else:
            print(f"  run {rid}: failed to parse")

    if not all_timings:
        print("No timing data collected.", file=sys.stderr)
        sys.exit(1)

    # Split by cache hit/miss
    hits = [t for t in all_timings if t.get("cache_hit") is True]
    misses = [t for t in all_timings if t.get("cache_hit") is False]

    step_keys = [k for k, _ in STEP_KEYS]

    for group_label, group in [("Cache HIT", hits), ("Cache MISS", misses)]:
        if not group:
            continue

        print(f"\n{'=' * 70}")
        print(f"# {group_label} ({len(group)} runs)")
        print(f"{'=' * 70}")

        # Per-step statistics
        print(f"\n## Step durations (mean ± stddev, n={len(group)})\n")
        print("| Step | Mean ± StdDev | Min | Max | Values |")
        print("|------|---------------|-----|-----|--------|")

        step_means = {}
        for key in ["setup"] + step_keys:
            values = [t.get(key, 0) for t in group]
            m = mean(values)
            s = stdev(values)
            step_means[key] = m
            mn = min(values)
            mx = max(values)
            vals_str = ", ".join(f"{v:.0f}" for v in values)
            print(f"| {key} | {fmt_stat(values)} | {fmt_dur(mn)} | {fmt_dur(mx)} | [{vals_str}] |")

        # Each layout is a list of (job_name, [step_keys_in_job]).
        # Every job implicitly includes "setup" overhead.
        LAYOUTS = {
            "Monolithic (1 job)": {
                "rust": ["fmt", "proto_gen", "clippy", "build", "build_examples",
                         "build_no_def", "msrv", "nightly_doc", "test_all", "test_no_def"],
            },
            "rust-lint + rust-test (current, 2 jobs)": {
                "rust-lint": ["fmt", "proto_gen", "clippy"],
                "rust-test": ["build", "build_examples", "build_no_def", "msrv",
                              "nightly_doc", "test_all", "test_no_def"],
            },
            "rust-stable + rust-compat (2 jobs)": {
                "rust-stable": ["fmt", "proto_gen", "clippy", "build", "build_examples",
                                "build_no_def", "test_all", "test_no_def"],
                "rust-compat": ["msrv", "nightly_doc"],
            },
            "rust-lint + rust-test + rust-compat (3 jobs)": {
                "rust-lint": ["fmt", "proto_gen", "clippy"],
                "rust-test": ["build", "build_examples", "build_no_def",
                              "test_all", "test_no_def"],
                "rust-compat": ["msrv", "nightly_doc"],
            },
        }

        def job_time(timings, steps_in_job):
            return timings.get("setup", 0) + sum(timings.get(s, 0) for s in steps_in_job)

        # Layout comparison using mean values
        print(f"\n## Layout comparison (using mean step durations)\n")
        print("| Layout | Per-job times | Wall clock | Runners |")
        print("|--------|---------------|------------|---------|")
        for layout_name, jobs in LAYOUTS.items():
            job_times = {jn: job_time(step_means, steps) for jn, steps in jobs.items()}
            wall = max(job_times.values())
            job_strs = ", ".join(f"{jn}={t/60:.1f}m" for jn, t in job_times.items())
            print(f"| {layout_name} | {job_strs} | **{wall/60:.1f}m** | {len(jobs)} |")

        # Per-run wall-clock distribution
        print(f"\n## Layout wall-clock distribution (per-run)\n")
        print("| Layout | Mean ± StdDev | Values |")
        print("|--------|---------------|--------|")

        for layout_name, jobs in LAYOUTS.items():
            per_run_walls = []
            for t in group:
                wall = max(job_time(t, steps) for steps in jobs.values())
                per_run_walls.append(wall)
            vals_str = ", ".join(f"{v/60:.1f}" for v in per_run_walls)
            print(f"| {layout_name} | {mean(per_run_walls)/60:.1f}m ± {stdev(per_run_walls)/60:.1f}m | [{vals_str}] min |")


if __name__ == "__main__":
    main()
