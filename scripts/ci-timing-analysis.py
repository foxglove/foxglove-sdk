#!/usr/bin/env python3
"""
Analyze CI build step timings and model the wall-clock impact of different
job layouts.

Usage:
    python3 scripts/ci-timing-analysis.py [--run-id-hit ID] [--run-id-miss ID]

If run IDs are not provided, the script finds recent runs on main automatically.
Requires `gh` CLI to be authenticated.
"""
import argparse
import json
import re
import subprocess
import sys
from collections import defaultdict
from dataclasses import dataclass
from datetime import datetime


REPO = "foxglove/foxglove-sdk"
WORKFLOW = "ci.yml"
# Before the split, the job was called "rust". After, "rust-lint" / "rust-test".
# Try both.
JOB_NAMES = ["rust", "rust-lint", "rust-test"]


@dataclass
class Step:
    name: str
    duration: float  # seconds

    @property
    def duration_str(self):
        if self.duration >= 60:
            return f"{int(self.duration // 60)}m {self.duration % 60:.0f}s"
        return f"{self.duration:.1f}s"


def gh(*args, **kwargs):
    r = subprocess.run(
        ["gh"] + list(args),
        capture_output=True, text=True, cwd="/workspace", **kwargs,
    )
    if r.returncode != 0:
        print(f"gh command failed: {r.stderr}", file=sys.stderr)
        return None
    return r.stdout.strip()


def find_run_ids():
    """Find a cache-hit and cache-miss run on main."""
    raw = gh(
        "run", "list", "--workflow", WORKFLOW, "--branch", "main",
        "--limit", "10", "--json", "databaseId,conclusion",
    )
    if not raw:
        return None, None
    runs = json.loads(raw)
    successful = [r["databaseId"] for r in runs if r["conclusion"] == "success"]
    if len(successful) < 2:
        return successful[0] if successful else None, None
    return successful[0], successful[1]


def get_job_id(run_id, job_name):
    raw = gh(
        "run", "view", str(run_id), "--json", "jobs", "--jq",
        f'.jobs[] | select(.name=="{job_name}") | .databaseId',
    )
    return raw if raw else None


def parse_job_log(run_id, job_name):
    """Parse step timings from a job's log output."""
    job_id = get_job_id(run_id, job_name)
    if not job_id:
        return None, False

    raw = gh("run", "view", str(run_id), "--log", "--job", job_id)
    if not raw:
        return None, False

    ts_re = re.compile(r"(\d{4}-\d{2}-\d{2}T[\d:.]+)Z")

    # Detect cache hit/miss
    cache_hit = None
    if "No cache found" in raw:
        cache_hit = False
    elif "Cache restored successfully" in raw and "rust-cache" in raw.lower():
        cache_hit = True

    # Extract step boundaries from ##[group]Run ... markers
    entries = []  # (timestamp, step_name)
    for line in raw.split("\n"):
        m = ts_re.search(line)
        if not m:
            continue
        ts = datetime.fromisoformat(m.group(1))

        if "##[group]Run " in line:
            step_name = line.split("##[group]Run ", 1)[-1].strip()[:100]
            entries.append((ts, step_name))

    # Convert to step durations
    steps = []
    for i in range(len(entries)):
        name = entries[i][1]
        start = entries[i][0]
        end = entries[i + 1][0] if i + 1 < len(entries) else start
        duration = (end - start).total_seconds()
        steps.append(Step(name=name, duration=duration))

    return steps, cache_hit


def classify_step(step):
    """Classify a step as setup, stable build, msrv build, nightly build, or other."""
    n = step.name.lower()
    if any(kw in n for kw in ["checkout", "setup-", "common-deps", "protoc",
                               "rust-cache", "rust-toolchain", "rustup",
                               "corepack", "actions/"]):
        return "setup"
    if "+1.83.0" in step.name:
        return "1.83.0"
    if "+nightly" in step.name:
        return "nightly"
    if any(kw in step.name for kw in ["cargo ", "make "]):
        return "stable"
    if "apt-get" in n or "sudo " in n:
        return "setup"
    return "other"


def cargo_step_id(step):
    """Extract a canonical cargo command for matching across layouts."""
    n = step.name
    # Normalize: strip --verbose, collapse whitespace
    n = n.replace(" --verbose", "").strip()
    # Take first line
    lines = [l.strip() for l in n.split("\n")
             if l.strip() and not l.strip().startswith("#")
             and not l.strip().startswith("set ")]
    return lines[0] if lines else n


def print_step_table(steps, cache_hit):
    status = "HIT" if cache_hit else "MISS" if cache_hit is False else "unknown"
    print(f"\nCache status: **{status}**\n")
    print("| Duration | Category | Step |")
    print("|----------|----------|------|")

    totals = defaultdict(float)
    for s in steps:
        cat = classify_step(s)
        totals[cat] += s.duration
        print(f"| {s.duration_str} | {cat} | `{s.name[:80]}` |")

    print(f"\n| Category | Total |")
    print(f"|----------|-------|")
    for cat in ["setup", "stable", "1.83.0", "nightly", "other"]:
        if totals[cat] > 0:
            t = totals[cat]
            print(f"| {cat} | {t:.0f}s ({t/60:.1f}m) |")
    total = sum(totals.values())
    print(f"| **total** | **{total:.0f}s ({total/60:.1f}m)** |")


def model_layouts(steps):
    """Given the measured step durations, model different job layouts."""

    # Identify each step by its cargo command
    step_map = {}
    setup_total = 0
    for s in steps:
        cat = classify_step(s)
        if cat == "setup":
            setup_total += s.duration
        else:
            cid = cargo_step_id(s)
            step_map[cid] = (s.duration, cat)

    # Define the steps in each layout option.
    # Within a job, steps run sequentially and share a target/ directory.
    # Across jobs, each job starts from cache (or empty) and compiles independently.
    #
    # Key: when clippy and build are in SEPARATE jobs on a cold run,
    # clippy must do a full compile (~same as cargo build).
    # When in the SAME job, clippy reuses target/ from cargo build.

    # Find specific step durations
    def dur(prefix):
        for cid, (d, _) in step_map.items():
            if cid.startswith(prefix):
                return d
        return 0

    d_fmt = dur("cargo fmt")
    d_proto = dur("cargo run --bin foxglove_proto_gen")
    d_clippy = dur("cargo clippy")
    d_build = dur("cargo build")
    d_examples = max(dur("set -euo"), dur("cargo metadata"), 0)  # build examples script
    if d_examples == 0:
        # Try alternate names
        for cid, (d, _) in step_map.items():
            if "example" in cid.lower() or "set -euo" in cid:
                d_examples = d
                break
    d_no_default = dur("cargo build -p foxglove")
    d_msrv = dur("cargo +1.83.0")
    d_nightly = dur("cargo +nightly")
    d_test_all = dur("cargo test --all")
    d_test_no_default = dur("cargo test -p foxglove")

    # On a cold run with clippy in a separate job, it compiles everything from
    # scratch (similar time to cargo build). Estimate this as build time.
    d_clippy_standalone_cold = d_build  # approximate

    # Other checks that appear in the log
    d_doc_checks = 0
    for cid, (d, cat) in step_map.items():
        if "grep" in cid or "[ -d" in cid:
            d_doc_checks += d

    print("\n## Measured step durations\n")
    print("| Step | Duration | Category |")
    print("|------|----------|----------|")
    for label, d, cat in [
        ("cargo fmt", d_fmt, "stable"),
        ("proto_gen", d_proto, "stable"),
        ("cargo clippy", d_clippy, "stable"),
        ("cargo build", d_build, "stable"),
        ("build examples", d_examples, "stable"),
        ("build no-default", d_no_default, "stable"),
        ("1.83.0 build foxglove", d_msrv, "1.83.0"),
        ("nightly rustdoc", d_nightly, "nightly"),
        ("test --all-features", d_test_all, "stable"),
        ("test no-default", d_test_no_default, "stable"),
    ]:
        print(f"| {label} | {d:.1f}s | {cat} |")

    print(f"\n**Setup overhead per job: {setup_total:.0f}s**\n")

    # --- Model each layout ---

    # For each layout, compute per-job time = setup + sum(steps in job).
    # Wall clock = max across parallel jobs.
    # Note: "clippy standalone" means clippy in its own job on a cold run
    # must compile from scratch; use d_clippy_standalone_cold.

    layouts = []

    # Layout 1: Monolithic
    mono = setup_total + d_fmt + d_proto + d_build + d_examples + d_no_default + d_msrv + d_clippy + d_nightly + d_doc_checks + d_test_all + d_test_no_default
    layouts.append(("Monolithic (1 job)", [("rust", mono)]))

    # Layout 2: rust-lint + rust-test (current)
    lint_2 = setup_total + d_fmt + d_proto + d_clippy
    test_2 = setup_total + d_build + d_examples + d_no_default + d_msrv + d_nightly + d_doc_checks + d_test_all + d_test_no_default
    layouts.append(("rust-lint + rust-test (current, 2 jobs)", [("rust-lint", lint_2), ("rust-test", test_2)]))

    # Layout 3: rust-stable + rust-compat (2 jobs, all stable merged)
    stable_3 = setup_total + d_fmt + d_proto + d_build + d_examples + d_no_default + d_clippy + d_test_all + d_test_no_default
    compat_3 = setup_total + d_msrv + d_nightly + d_doc_checks
    layouts.append(("rust-stable + rust-compat (2 jobs)", [("rust-stable", stable_3), ("rust-compat", compat_3)]))

    # Layout 4: rust-lint + rust-test + rust-compat (3 jobs)
    lint_4 = setup_total + d_fmt + d_proto + d_clippy
    test_4 = setup_total + d_build + d_examples + d_no_default + d_test_all + d_test_no_default
    compat_4 = setup_total + d_msrv + d_nightly + d_doc_checks
    layouts.append(("rust-lint + rust-test + rust-compat (3 jobs)", [("rust-lint", lint_4), ("rust-test", test_4), ("rust-compat", compat_4)]))

    print("## Layout comparison\n")
    print("| Layout | Per-job times | Wall clock | Runners |")
    print("|--------|---------------|------------|---------|")
    for name, jobs in layouts:
        wall = max(t for _, t in jobs)
        job_strs = ", ".join(f"{jn}={t/60:.1f}m" for jn, t in jobs)
        print(f"| {name} | {job_strs} | **{wall/60:.1f}m** | {len(jobs)} |")

    return setup_total


def main():
    parser = argparse.ArgumentParser(description="Analyze CI timing")
    parser.add_argument("--run-id-hit", type=int, help="Run ID with cache hit")
    parser.add_argument("--run-id-miss", type=int, help="Run ID with cache miss")
    args = parser.parse_args()

    run_hit = args.run_id_hit
    run_miss = args.run_id_miss

    if not run_hit and not run_miss:
        print("Finding recent CI runs on main...\n")
        r1, r2 = find_run_ids()
        if r1:
            run_hit = r1
        if r2:
            run_miss = r2

    for run_id, label in [(run_hit, "Run A"), (run_miss, "Run B")]:
        if not run_id:
            continue

        print(f"\n{'=' * 70}")
        print(f"# {label}: run {run_id}")
        print(f"{'=' * 70}")

        # Try the old monolithic "rust" job name first, then the split names
        all_steps = []
        cache_hit = None

        for job_name in JOB_NAMES:
            steps, hit = parse_job_log(run_id, job_name)
            if steps:
                print(f"\n## Job: {job_name}")
                print_step_table(steps, hit)
                all_steps.extend(steps)
                if hit is not None:
                    cache_hit = hit

        if all_steps:
            print(f"\n## Layout modeling (using timings from run {run_id})")
            model_layouts(all_steps)


if __name__ == "__main__":
    main()
