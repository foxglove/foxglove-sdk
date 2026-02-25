#!/usr/bin/env python3
"""
Analyze end-to-end CI timing across all workflows triggered by a push.

Fetches job-level start/end times from every workflow, computes per-workflow
and overall wall-clock and runner-minutes, and models the impact of job layout
changes on the full CI flow.

For layout modeling, pass --model-workflow and --model-job to specify which
job's steps to extract. Define layouts in the LAYOUTS dict at the top of the
script. Steps are matched by substring against the log step names.

Usage:
    python3 scripts/ci-timing-analysis.py [--limit N]
           [--model-workflow ci.yml --model-job rust]

Requires `gh` CLI to be authenticated.
"""
import argparse
import json
import re
import statistics
import subprocess
import sys
from collections import defaultdict
from datetime import datetime


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

# Layout definitions for modeling job splits.
#
# Each layout maps hypothetical job names to lists of step matchers.
# A step matcher is a substring matched against the step's run command.
# Every job implicitly pays one "setup" overhead (all steps not matched
# by any matcher are summed as setup).
#
# Multiple layout sets can be defined for different workflows/jobs.
# Select which to use with --model-layouts.

LAYOUT_SETS = {
    "ci": {
        "monolithic (1 job)": {
            "rust": ["*"],
        },
        "rust + rust-compat (current)": {
            "rust": [
                "cargo fmt", "cargo run --bin foxglove_proto_gen",
                "cargo clippy", "cargo build", "set -euo",
                "cargo test",
            ],
            "rust-compat": ["cargo +1.83.0", "cargo +nightly"],
        },
        "rust-lint + rust-test + rust-compat": {
            "rust-lint": [
                "cargo fmt", "cargo run --bin foxglove_proto_gen",
                "cargo clippy",
            ],
            "rust-test": ["cargo build", "set -euo", "cargo test"],
            "rust-compat": ["cargo +1.83.0", "cargo +nightly"],
        },
    },
    "c_cpp_lint": {
        "lint (current, 1 job)": {
            "lint": ["*"],
        },
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


def find_push_shas(limit):
    """Find recent push SHAs by looking at ci.yml runs (always triggers)."""
    raw = gh(
        "run", "list", "--workflow", "ci.yml", "--branch", "main",
        "--limit", str(limit * 2),
        "--json", "databaseId,conclusion,headSha",
    )
    if not raw:
        return []
    runs = json.loads(raw)
    seen = set()
    shas = []
    for r in runs:
        if r["conclusion"] == "success" and r["headSha"] not in seen:
            seen.add(r["headSha"])
            shas.append(r["headSha"])
    return shas[:limit]


def get_workflow_jobs(sha, workflow):
    """Get all jobs for a workflow triggered by a specific commit."""
    raw = gh(
        "run", "list", "--workflow", workflow, "--commit", sha,
        "--json", "databaseId",
    )
    if not raw:
        return [], None
    runs = json.loads(raw)
    if not runs:
        return [], None

    run_id = runs[0]["databaseId"]
    raw2 = gh("run", "view", str(run_id), "--json", "jobs")
    if not raw2:
        return [], run_id

    data = json.loads(raw2)
    return data.get("jobs", []), run_id


def parse_ts(ts_str):
    if not ts_str:
        return None
    return datetime.fromisoformat(ts_str.replace("Z", "+00:00"))


def parse_job_steps(run_id, job_name):
    """Extract all steps and their durations from a job's log."""
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

    steps = []
    for i in range(len(entries)):
        name = entries[i][1]
        start = entries[i][0]
        end = entries[i + 1][0] if i + 1 < len(entries) else last_ts
        duration = (end - start).total_seconds()
        steps.append({"name": name, "duration": duration})

    return {"steps": steps, "cache_hit": cache_hit}


def match_step(step_name, matchers):
    """Check if a step name matches any of the given substring matchers."""
    if "*" in matchers:
        return True
    return any(m in step_name for m in matchers)


def compute_layout(steps, layout):
    """Given a list of steps and a layout, compute per-job durations."""
    setup_total = 0
    job_build = defaultdict(float)
    matched_jobs = set()

    for step in steps:
        assigned = False
        for job_name, matchers in layout.items():
            if match_step(step["name"], matchers):
                job_build[job_name] += step["duration"]
                matched_jobs.add(job_name)
                assigned = True
                break
        if not assigned:
            setup_total += step["duration"]

    result = {}
    for job_name in layout:
        result[job_name] = setup_total + job_build[job_name]

    return result


def fmt_stat(values):
    m = statistics.mean(values)
    s = statistics.stdev(values) if len(values) >= 2 else 0
    return f"{m/60:.1f}m ± {s/60:.1f}m"


def main():
    parser = argparse.ArgumentParser(description="Analyze end-to-end CI timing")
    parser.add_argument("--limit", type=int, default=10,
                        help="Number of recent pushes to analyze (default: 10)")
    parser.add_argument("--model-workflow", type=str, default="ci.yml",
                        help="Workflow to extract step-level data from (default: ci.yml)")
    parser.add_argument("--model-job", type=str, nargs="+",
                        default=["rust", "rust-compat", "rust-lint", "rust-test"],
                        help="Job name(s) to extract steps from (tries each)")
    parser.add_argument("--model-layouts", type=str, default="ci",
                        help=f"Layout set to model (choices: {', '.join(LAYOUT_SETS.keys())})")
    args = parser.parse_args()

    print(f"Finding {args.limit} recent pushes to main...\n")
    shas = find_push_shas(args.limit)
    if not shas:
        print("No pushes found.", file=sys.stderr)
        sys.exit(1)

    all_push_data = []

    for sha in shas:
        push_data = {"sha": sha[:8], "workflows": {}}

        for wf in ALL_WORKFLOWS:
            jobs, run_id = get_workflow_jobs(sha, wf)
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
                    "run_id": run_id,
                }

        # Extract step-level data from the target workflow/job
        wf_data = push_data["workflows"].get(args.model_workflow)
        if wf_data:
            for job_name in args.model_job:
                step_data = parse_job_steps(wf_data["run_id"], job_name)
                if step_data and step_data["steps"]:
                    push_data["step_data"] = step_data
                    break

        if push_data["workflows"]:
            all_push_data.append(push_data)
            print(f"  {sha[:8]}: {len(push_data['workflows'])} workflows"
                  f"{' + steps' if 'step_data' in push_data else ''}")

    if not all_push_data:
        print("No data collected.", file=sys.stderr)
        sys.exit(1)

    # === Section 1: Per-workflow wall clock ===
    print(f"\n{'='*70}")
    print(f"# Per-workflow wall clock (n={len(all_push_data)} pushes)")
    print(f"{'='*70}\n")

    print("| Workflow | Wall clock | Runner-minutes | Bottleneck job |")
    print("|----------|------------|----------------|----------------|")

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
            continue

        top = max(bottleneck_counts, key=bottleneck_counts.get)
        print(f"| {wf} | {fmt_stat(walls)} | {fmt_stat(runners)} | {top} |")

    # === Section 2: End-to-end ===
    print(f"\n{'='*70}")
    print(f"# End-to-end CI time (n={len(all_push_data)} pushes)")
    print(f"{'='*70}\n")

    e2e_walls = []
    e2e_runners = []
    e2e_bottlenecks = defaultdict(int)

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
            e2e_walls.append((max(all_ends) - min(all_starts)).total_seconds())
            e2e_runners.append(total_runner)
            if slowest_wf:
                e2e_bottlenecks[slowest_wf] += 1

    print(f"End-to-end wall clock: **{fmt_stat(e2e_walls)}**")
    print(f"Total runner-minutes:  **{fmt_stat(e2e_runners)}**\n")
    print("Bottleneck workflow frequency:")
    for wf, count in sorted(e2e_bottlenecks.items(), key=lambda x: -x[1]):
        print(f"  {wf}: {count}/{len(all_push_data)}")

    # === Section 3: Step-level layout modeling ===
    step_runs = [pd for pd in all_push_data if "step_data" in pd]
    if not step_runs:
        print(f"\nNo step-level data for {args.model_workflow} / {args.model_job}")
        return

    layouts = LAYOUT_SETS.get(args.model_layouts, {})
    if not layouts:
        print(f"\nNo layout set '{args.model_layouts}'. Available: {', '.join(LAYOUT_SETS.keys())}")
        return

    hits = [pd for pd in step_runs if pd["step_data"].get("cache_hit") is True]
    misses = [pd for pd in step_runs if pd["step_data"].get("cache_hit") is False]
    unknown = [pd for pd in step_runs if pd["step_data"].get("cache_hit") is None]

    groups = [("Cache HIT", hits), ("Cache MISS", misses)]
    if unknown:
        if not hits and not misses:
            groups = [("All runs", unknown)]
        else:
            groups.append(("Cache unknown", unknown))

    for label, group in groups:
        if not group:
            continue

        print(f"\n{'='*70}")
        print(f"# {args.model_workflow} step modeling — {label} (n={len(group)})")
        print(f"{'='*70}\n")

        # Print step durations
        all_step_names = []
        for pd in group:
            for s in pd["step_data"]["steps"]:
                if s["name"] not in all_step_names:
                    all_step_names.append(s["name"])

        print("## Step durations\n")
        print("| # | Step | Mean ± StdDev | Min | Max |")
        print("|---|------|---------------|-----|-----|")

        for idx, name in enumerate(all_step_names):
            values = []
            for pd in group:
                dur = 0
                for s in pd["step_data"]["steps"]:
                    if s["name"] == name:
                        dur = s["duration"]
                        break
                values.append(dur)
            if max(values) < 0.1:
                continue
            m = statistics.mean(values)
            s = statistics.stdev(values) if len(values) >= 2 else 0
            mn, mx = min(values), max(values)
            fmt_d = lambda v: f"{int(v//60)}m {v%60:.0f}s" if v >= 60 else f"{v:.1f}s"
            print(f"| {idx} | `{name[:72]}` | {m:.1f} ± {s:.1f}s | {fmt_d(mn)} | {fmt_d(mx)} |")

        # Get non-modeled workflow wall clocks for E2E computation
        other_walls = []
        for pd in group:
            max_other = 0
            for wf, wf_data in pd["workflows"].items():
                if wf != args.model_workflow:
                    max_other = max(max_other, wf_data["wall"])
            other_walls.append(max_other)

        print(f"\n## Layout impact on end-to-end CI\n")
        print(f"Slowest other workflow: {fmt_stat(other_walls)}\n")
        print("| Layout | Modeled wall | E2E wall | Runners (runner-min) |")
        print("|--------|-------------|----------|----------------------|")

        for layout_name, layout_jobs in layouts.items():
            mod_walls = []
            e2e_with = []
            mod_runners = []

            for i, pd in enumerate(group):
                job_times = compute_layout(pd["step_data"]["steps"], layout_jobs)
                wall = max(job_times.values())
                runner = sum(job_times.values())
                mod_walls.append(wall)
                mod_runners.append(runner)
                if i < len(other_walls):
                    e2e_with.append(max(wall, other_walls[i]))

            n_jobs = len(layout_jobs)
            print(f"| {layout_name} | {fmt_stat(mod_walls)} | "
                  f"{fmt_stat(e2e_with) if e2e_with else 'N/A'} | "
                  f"{n_jobs} ({fmt_stat(mod_runners)}) |")


if __name__ == "__main__":
    main()
