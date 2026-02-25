#!/usr/bin/env python3
"""
Analyze end-to-end CI timing across all workflows triggered by a push.

Fetches job-level start/end times from every workflow, computes per-workflow
and overall wall-clock and runner-minutes, and models the impact of ci.yml
layout changes on the full CI flow.

Usage:
    python3 scripts/ci-timing-analysis.py [--limit N]

Requires `gh` CLI to be authenticated.
"""
import argparse
import json
import re
import statistics
import subprocess
import sys
from collections import defaultdict
from datetime import datetime, timezone


# All workflows that trigger on push-to-main / PR.
ALL_WORKFLOWS = [
    "ci.yml",
    "python.yml",
    "c_cpp.yml",
    "cpp_data_loader.yml",
    "remote-access-tests.yml",
    "docs.yml",
    "ros.yml",
]

# ci.yml step prefixes for detailed modeling of layout options.
CI_STEP_KEYS = [
    ("fmt",            "cargo fmt"),
    ("proto_gen",      "cargo run --bin foxglove_proto_gen"),
    ("clippy",         "cargo clippy"),
    ("build",          "cargo build --"),
    ("build_examples", "set -euo"),
    ("build_no_def",   "cargo build -p foxglove"),
    ("msrv",           "cargo +1.83.0"),
    ("nightly_doc",    "cargo +nightly"),
    ("test_all",       "cargo test --all"),
    ("test_no_def",    "cargo test -p foxglove"),
]

# ci.yml layout options. Each maps job names to step keys.
# Every job implicitly pays one "setup" overhead.
CI_LAYOUTS = {
    "monolithic (1 job)": {
        "rust": [
            "fmt", "proto_gen", "clippy", "build", "build_examples",
            "build_no_def", "msrv", "nightly_doc", "test_all", "test_no_def",
        ],
    },
    "rust + rust-compat (current)": {
        "rust": [
            "fmt", "proto_gen", "clippy", "build", "build_examples",
            "build_no_def", "test_all", "test_no_def",
        ],
        "rust-compat": ["msrv", "nightly_doc"],
    },
    "rust-lint + rust-test + rust-compat": {
        "rust-lint": ["fmt", "proto_gen", "clippy"],
        "rust-test": [
            "build", "build_examples", "build_no_def",
            "test_all", "test_no_def",
        ],
        "rust-compat": ["msrv", "nightly_doc"],
    },
}


REPO_ROOT = subprocess.run(
    ["git", "rev-parse", "--show-toplevel"],
    capture_output=True, text=True,
).stdout.strip() or "."


def gh(*args):
    r = subprocess.run(
        ["gh"] + list(args),
        capture_output=True, text=True, cwd=REPO_ROOT,
    )
    if r.returncode != 0:
        return None
    return r.stdout.strip()


def find_push_events(limit):
    """Find recent pushes to main by looking at ci.yml runs (always triggers)."""
    raw = gh(
        "run", "list", "--workflow", "ci.yml", "--branch", "main",
        "--limit", str(limit * 2),
        "--json", "databaseId,conclusion,headSha,startedAt",
    )
    if not raw:
        return []
    runs = json.loads(raw)
    seen = set()
    pushes = []
    for r in runs:
        if r["conclusion"] == "success" and r["headSha"] not in seen:
            seen.add(r["headSha"])
            pushes.append({
                "sha": r["headSha"],
                "started": r["startedAt"],
            })
    return pushes[:limit]


def get_workflow_jobs(sha, workflow):
    """Get all jobs for a workflow triggered by a specific commit."""
    raw = gh(
        "run", "list", "--workflow", workflow, "--commit", sha,
        "--json", "databaseId,conclusion",
    )
    if not raw:
        return []
    runs = json.loads(raw)
    if not runs:
        return []

    run_id = runs[0]["databaseId"]
    raw = gh(
        "run", "view", str(run_id), "--json", "jobs",
        "--jq", ".jobs[]",
    )
    if not raw:
        return []

    jobs = []
    for line in raw.split("\n"):
        if not line.strip():
            continue
        try:
            job = json.loads(line)
            jobs.append(job)
        except json.JSONDecodeError:
            pass

    if not jobs:
        raw2 = gh("run", "view", str(run_id), "--json", "jobs")
        if raw2:
            data = json.loads(raw2)
            jobs = data.get("jobs", [])

    return jobs


def parse_ts(ts_str):
    if not ts_str:
        return None
    return datetime.fromisoformat(ts_str.replace("Z", "+00:00"))


def parse_ci_step_log(run_id, job_name):
    """Parse step-level timings from ci.yml job logs for layout modeling."""
    job_id_raw = gh(
        "run", "view", str(run_id), "--json", "jobs", "--jq",
        f'.jobs[] | select(.name=="{job_name}") | .databaseId',
    )
    if not job_id_raw:
        return None

    raw = gh("run", "view", str(run_id), "--log", "--job", job_id_raw)
    if not raw:
        return None

    ts_re = re.compile(r"(\d{4}-\d{2}-\d{2}T[\d:.]+)Z")

    cache_hit = None
    if "No cache found" in raw:
        cache_hit = False
    elif "Cache restored successfully" in raw and "rust-cache" in raw.lower():
        cache_hit = True

    entries = []
    last_ts = None
    for line in raw.split("\n"):
        m = ts_re.search(line)
        if not m:
            continue
        ts = datetime.fromisoformat(m.group(1))
        last_ts = ts
        if "##[group]Run " in line:
            step_name = line.split("##[group]Run ", 1)[-1].strip()[:120]
            entries.append((ts, step_name))

    steps = {}
    setup_total = 0.0
    for i in range(len(entries)):
        name = entries[i][1]
        start = entries[i][0]
        end = entries[i + 1][0] if i + 1 < len(entries) else last_ts
        duration = (end - start).total_seconds()

        matched = False
        for key, prefix in CI_STEP_KEYS:
            if name.strip().startswith(prefix):
                steps[key] = duration
                matched = True
                break

        if not matched:
            rn = name.lower()
            if any(kw in rn for kw in [
                "checkout", "setup-", "common-deps", "protoc", "rust-cache",
                "rust-toolchain", "rustup", "corepack", "actions/",
                "apt-get", "sudo ", "swatinem", "not all versions",
                "construct rustup", "add-matcher", "command -v rustup",
                "grep -r", "! grep",
            ]):
                setup_total += duration

    steps["setup"] = setup_total
    steps["cache_hit"] = cache_hit
    return steps


def fmt_stat(values):
    m = statistics.mean(values)
    s = statistics.stdev(values) if len(values) >= 2 else 0
    return f"{m/60:.1f}m ± {s/60:.1f}m"


def ci_job_time(timings, steps_in_job):
    # Each job pays one setup overhead (approximated as the average observed).
    return timings.get("setup", 0) + sum(timings.get(s, 0) for s in steps_in_job)


def main():
    parser = argparse.ArgumentParser(description="Analyze end-to-end CI timing")
    parser.add_argument("--limit", type=int, default=10,
                        help="Number of recent pushes to analyze (default: 10)")
    args = parser.parse_args()

    print(f"Finding {args.limit} recent pushes to main...\n")
    pushes = find_push_events(args.limit)
    if not pushes:
        print("No pushes found.", file=sys.stderr)
        sys.exit(1)

    # For each push, gather timing from all workflows
    all_push_data = []

    for push in pushes:
        sha = push["sha"]
        push_data = {"sha": sha[:8], "workflows": {}}

        for wf in ALL_WORKFLOWS:
            jobs = get_workflow_jobs(sha, wf)
            if not jobs:
                continue

            wf_jobs = []
            for job in jobs:
                started = parse_ts(job.get("startedAt"))
                completed = parse_ts(job.get("completedAt"))
                if not started or not completed:
                    continue
                duration = (completed - started).total_seconds()
                wf_jobs.append({
                    "name": job["name"],
                    "duration": duration,
                    "conclusion": job.get("conclusion", "?"),
                    "started": started,
                    "completed": completed,
                })

            if wf_jobs:
                earliest = min(j["started"] for j in wf_jobs)
                latest = max(j["completed"] for j in wf_jobs)
                push_data["workflows"][wf] = {
                    "jobs": wf_jobs,
                    "wall": (latest - earliest).total_seconds(),
                    "runner_total": sum(j["duration"] for j in wf_jobs),
                }

        # Get ci.yml step-level detail for layout modeling
        ci_run_raw = gh(
            "run", "list", "--workflow", "ci.yml", "--commit", sha,
            "--json", "databaseId",
        )
        if ci_run_raw:
            ci_runs = json.loads(ci_run_raw)
            if ci_runs:
                ci_run_id = ci_runs[0]["databaseId"]
                for job_name in ["rust", "rust-compat", "rust-lint", "rust-test"]:
                    steps = parse_ci_step_log(ci_run_id, job_name)
                    if steps:
                        push_data["ci_steps"] = steps
                        break

        if push_data["workflows"]:
            all_push_data.append(push_data)
            print(f"  {sha[:8]}: {len(push_data['workflows'])} workflows")

    if not all_push_data:
        print("No data collected.", file=sys.stderr)
        sys.exit(1)

    # === Section 1: Per-workflow wall clock ===
    print(f"\n{'='*70}")
    print(f"# Per-workflow wall clock (n={len(all_push_data)} pushes)")
    print(f"{'='*70}\n")

    print("| Workflow | Wall clock (mean ± sd) | Runner-min (mean ± sd) | Bottleneck job |")
    print("|----------|------------------------|------------------------|----------------|")

    for wf in ALL_WORKFLOWS:
        walls = []
        runners = []
        bottleneck_counts = defaultdict(int)
        for pd in all_push_data:
            wf_data = pd["workflows"].get(wf)
            if wf_data:
                walls.append(wf_data["wall"])
                runners.append(wf_data["runner_total"])
                longest_job = max(wf_data["jobs"], key=lambda j: j["duration"])
                bottleneck_counts[longest_job["name"]] += 1

        if not walls:
            print(f"| {wf} | (no data) | | |")
            continue

        top_bottleneck = max(bottleneck_counts, key=bottleneck_counts.get) if bottleneck_counts else "?"
        print(f"| {wf} | {fmt_stat(walls)} | {fmt_stat(runners)} | {top_bottleneck} |")

    # === Section 2: End-to-end across all workflows ===
    print(f"\n{'='*70}")
    print(f"# End-to-end CI time (all workflows, n={len(all_push_data)} pushes)")
    print(f"{'='*70}\n")

    e2e_walls = []
    e2e_runners = []
    e2e_bottleneck_counts = defaultdict(int)

    for pd in all_push_data:
        all_starts = []
        all_ends = []
        total_runner = 0
        slowest_wf = None
        slowest_wf_time = 0
        for wf, wf_data in pd["workflows"].items():
            total_runner += wf_data["runner_total"]
            for j in wf_data["jobs"]:
                all_starts.append(j["started"])
                all_ends.append(j["completed"])
            if wf_data["wall"] > slowest_wf_time:
                slowest_wf_time = wf_data["wall"]
                slowest_wf = wf

        if all_starts and all_ends:
            e2e = (max(all_ends) - min(all_starts)).total_seconds()
            e2e_walls.append(e2e)
            e2e_runners.append(total_runner)
            if slowest_wf:
                e2e_bottleneck_counts[slowest_wf] += 1

    print(f"End-to-end wall clock: **{fmt_stat(e2e_walls)}**")
    print(f"Total runner-minutes:  **{fmt_stat(e2e_runners)}**\n")
    print("Bottleneck workflow frequency:")
    for wf, count in sorted(e2e_bottleneck_counts.items(), key=lambda x: -x[1]):
        print(f"  {wf}: {count}/{len(all_push_data)} pushes")

    # === Section 3: ci.yml layout modeling ===
    ci_step_data = [pd["ci_steps"] for pd in all_push_data if "ci_steps" in pd]
    if not ci_step_data:
        print("\nNo ci.yml step-level data available for layout modeling.")
        return

    hits = [s for s in ci_step_data if s.get("cache_hit") is True]
    misses = [s for s in ci_step_data if s.get("cache_hit") is False]

    for label, group in [("Cache HIT", hits), ("Cache MISS", misses)]:
        if not group:
            continue

        print(f"\n{'='*70}")
        print(f"# ci.yml layout modeling — {label} (n={len(group)})")
        print(f"{'='*70}\n")

        step_keys = [k for k, _ in CI_STEP_KEYS]

        print("## ci.yml step durations\n")
        print("| Step | Mean ± StdDev | Min | Max |")
        print("|------|---------------|-----|-----|")
        for key in ["setup"] + step_keys:
            values = [t.get(key, 0) for t in group]
            if max(values) == 0:
                continue
            m = statistics.mean(values)
            s = statistics.stdev(values) if len(values) >= 2 else 0
            mn, mx = min(values), max(values)
            mn_s = f"{int(mn//60)}m {mn%60:.0f}s" if mn >= 60 else f"{mn:.1f}s"
            mx_s = f"{int(mx//60)}m {mx%60:.0f}s" if mx >= 60 else f"{mx:.1f}s"
            print(f"| {key} | {m:.1f} ± {s:.1f}s | {mn_s} | {mx_s} |")

        # Model ci.yml layouts and compute impact on end-to-end
        # Get the non-ci.yml workflow wall clocks for these same pushes
        other_wf_walls = []
        for pd in all_push_data:
            if "ci_steps" not in pd:
                continue
            if pd["ci_steps"].get("cache_hit") != (label == "Cache HIT"):
                continue
            max_other = 0
            for wf, wf_data in pd["workflows"].items():
                if wf != "ci.yml":
                    max_other = max(max_other, wf_data["wall"])
            other_wf_walls.append(max_other)

        print(f"\n## Impact on end-to-end CI time\n")
        print(f"Slowest non-ci.yml workflow per push: {fmt_stat(other_wf_walls)}\n")
        print("| ci.yml layout | ci.yml wall | E2E wall (with other workflows) | ci.yml runners |")
        print("|---------------|-------------|--------------------------------|----------------|")

        for layout_name, jobs in CI_LAYOUTS.items():
            ci_walls = []
            e2e_with_layout = []
            ci_runners = []

            for i, t in enumerate(group):
                job_times = {jn: ci_job_time(t, steps) for jn, steps in jobs.items()}
                ci_wall = max(job_times.values())
                ci_runner = sum(job_times.values())
                ci_walls.append(ci_wall)
                ci_runners.append(ci_runner)
                if i < len(other_wf_walls):
                    e2e_with_layout.append(max(ci_wall, other_wf_walls[i]))

            print(f"| {layout_name} | {fmt_stat(ci_walls)} | "
                  f"{fmt_stat(e2e_with_layout) if e2e_with_layout else 'N/A'} | "
                  f"{len(jobs)} ({fmt_stat(ci_runners)}) |")


if __name__ == "__main__":
    main()
