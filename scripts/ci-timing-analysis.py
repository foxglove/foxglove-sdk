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
import re
import statistics
import subprocess
import sys
from datetime import datetime


# Workflows and the job names to look for in each.
WORKFLOWS = {
    "ci.yml": ["rust", "rust-compat", "rust-lint", "rust-test"],
    "remote-access-tests.yml": ["test"],
}

# Canonical step names, matched by prefix against the run command.
STEP_KEYS = [
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
    ("docker_up",      "docker compose up"),
    ("ra_test_lk",     "cargo test -p remote_access_tests -- --ignored livekit_"),
    ("ra_test_auth",   "cargo test -p remote_access_tests -- --ignored auth_"),
    ("docker_down",    "docker compose down"),
]

# Each layout maps job names to the steps that run in that job.
# Every job implicitly includes one "setup" overhead.
LAYOUTS = {
    "Monolithic (1 job)": {
        "rust": [
            "fmt", "proto_gen", "clippy", "build", "build_examples",
            "build_no_def", "msrv", "nightly_doc", "test_all", "test_no_def",
        ],
    },
    "rust + rust-compat (current, 2 jobs)": {
        "rust": [
            "fmt", "proto_gen", "clippy", "build", "build_examples",
            "build_no_def", "test_all", "test_no_def",
        ],
        "rust-compat": ["msrv", "nightly_doc"],
    },
    "rust-lint + rust-test + rust-compat (3 jobs)": {
        "rust-lint": ["fmt", "proto_gen", "clippy"],
        "rust-test": [
            "build", "build_examples", "build_no_def",
            "test_all", "test_no_def",
        ],
        "rust-compat": ["msrv", "nightly_doc"],
    },
}

# Same layouts but with remote-access-tests as a separate parallel job.
LAYOUTS_WITH_RA = {
    f"{name} + remote-access": {
        **jobs,
        "remote-access": ["docker_up", "ra_test_lk", "ra_test_auth", "docker_down"],
    }
    for name, jobs in LAYOUTS.items()
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


def find_run_ids(workflow, limit):
    raw = gh(
        "run", "list", "--workflow", workflow, "--branch", "main",
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
    for i in range(len(entries)):
        name = entries[i][1]
        start = entries[i][0]
        end = entries[i + 1][0] if i + 1 < len(entries) else last_ts
        duration = (end - start).total_seconds()
        steps[name] = duration

    return steps, cache_hit


def extract_timings(run_ids_by_workflow):
    """Extract canonical step timings from paired workflow runs."""
    all_raw_steps = {}
    cache_hit = None

    for workflow, run_id in run_ids_by_workflow.items():
        job_names = WORKFLOWS[workflow]
        for job_name in job_names:
            raw, hit = parse_job_log(run_id, job_name)
            if raw:
                all_raw_steps.update(raw)
                if hit is not None:
                    cache_hit = hit

    if not all_raw_steps:
        return None

    matched = {}
    setup_total = 0.0

    for raw_name, duration in all_raw_steps.items():
        found = False
        for key, prefix in STEP_KEYS:
            if raw_name.strip().startswith(prefix):
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
    return matched


def fmt_dur(seconds):
    if seconds >= 60:
        return f"{int(seconds // 60)}m {seconds % 60:.0f}s"
    return f"{seconds:.1f}s"


def fmt_stat(values):
    m = statistics.mean(values)
    s = statistics.stdev(values) if len(values) >= 2 else 0
    return f"{m:.1f} ± {s:.1f}"


def job_time(timings, steps_in_job):
    # Each job pays one setup overhead. This uses the average setup time observed
    # across all jobs in the run as an approximation.
    return timings.get("setup", 0) + sum(timings.get(s, 0) for s in steps_in_job)


def analyze_layouts(group, all_layouts, step_keys):
    """Compute wall-clock and total runner-minutes for each layout across runs."""
    results = {}
    for layout_name, jobs in all_layouts.items():
        per_run_walls = []
        per_run_total_runner = []
        for t in group:
            job_times = {jn: job_time(t, steps) for jn, steps in jobs.items()}
            per_run_walls.append(max(job_times.values()))
            per_run_total_runner.append(sum(job_times.values()))
        results[layout_name] = {
            "jobs": jobs,
            "wall": per_run_walls,
            "runner": per_run_total_runner,
        }
    return results


def print_layout_table(results):
    print("| Layout | Runners | Wall clock (mean ± sd) | Runner-minutes (mean ± sd) |")
    print("|--------|---------|------------------------|---------------------------|")
    for layout_name, data in results.items():
        n_runners = len(data["jobs"])
        wall = data["wall"]
        runner = data["runner"]
        wall_m = statistics.mean(wall)
        wall_s = statistics.stdev(wall) if len(wall) >= 2 else 0
        runner_m = statistics.mean(runner)
        runner_s = statistics.stdev(runner) if len(runner) >= 2 else 0
        print(f"| {layout_name} | {n_runners} | "
              f"{wall_m/60:.1f}m ± {wall_s/60:.1f}m | "
              f"{runner_m/60:.1f}m ± {runner_s/60:.1f}m |")


def main():
    parser = argparse.ArgumentParser(description="Analyze CI timing across multiple runs")
    parser.add_argument("--run-ids", type=int, nargs="+",
                        help="Specific CI run IDs to analyze")
    parser.add_argument("--ra-run-ids", type=int, nargs="+",
                        help="Specific remote-access-tests run IDs (paired 1:1 with --run-ids)")
    parser.add_argument("--limit", type=int, default=8,
                        help="Number of recent runs to fetch (default: 8)")
    args = parser.parse_args()

    if args.run_ids:
        ci_run_ids = args.run_ids
        ra_run_ids = args.ra_run_ids or []
    else:
        print(f"Finding up to {args.limit} recent successful runs on main...\n")
        ci_run_ids = find_run_ids("ci.yml", args.limit)
        ra_run_ids = find_run_ids("remote-access-tests.yml", args.limit)

    if not ci_run_ids:
        print("No CI runs found.", file=sys.stderr)
        sys.exit(1)

    # Pair CI and remote-access runs by proximity (they trigger on the same push)
    ra_remaining = list(ra_run_ids)
    paired = []
    for ci_id in ci_run_ids:
        best_ra = None
        if ra_remaining:
            best_ra = min(ra_remaining, key=lambda ra: abs(ra - ci_id))
            if abs(best_ra - ci_id) < 100:
                ra_remaining.remove(best_ra)
            else:
                best_ra = None
        paired.append((ci_id, best_ra))

    print(f"Analyzing {len(paired)} run pairs:\n")

    all_timings = []
    step_keys = [k for k, _ in STEP_KEYS]

    for ci_id, ra_id in paired:
        workflows = {"ci.yml": ci_id}
        if ra_id:
            workflows["remote-access-tests.yml"] = ra_id

        t = extract_timings(workflows)
        if t:
            all_timings.append(t)
            status = "HIT" if t["cache_hit"] else "MISS" if t["cache_hit"] is False else "?"
            ra_str = f" + ra:{ra_id}" if ra_id else ""
            print(f"  ci:{ci_id}{ra_str}: cache {status}")
        else:
            print(f"  ci:{ci_id}: failed to parse")

    if not all_timings:
        print("No timing data collected.", file=sys.stderr)
        sys.exit(1)

    hits = [t for t in all_timings if t.get("cache_hit") is True]
    misses = [t for t in all_timings if t.get("cache_hit") is False]

    for group_label, group in [("Cache HIT", hits), ("Cache MISS", misses)]:
        if not group:
            continue

        print(f"\n{'=' * 70}")
        print(f"# {group_label} (n={len(group)})")
        print(f"{'=' * 70}")

        print(f"\n## Step durations in seconds (mean ± stddev)\n")
        print("| Step | Mean ± StdDev | Min | Max |")
        print("|------|---------------|-----|-----|")

        for key in ["setup"] + step_keys:
            values = [t.get(key, 0) for t in group]
            if max(values) == 0:
                continue
            print(f"| {key} | {fmt_stat(values)}s | {fmt_dur(min(values))} | {fmt_dur(max(values))} |")

        has_ra = any(t.get("ra_test_lk", 0) > 0 for t in group)

        print("\n## CI-only layouts\n")
        results = analyze_layouts(group, LAYOUTS, step_keys)
        print_layout_table(results)

        if has_ra:
            print("\n## CI + remote-access-tests (separate parallel job)\n")
            results_ra = analyze_layouts(group, LAYOUTS_WITH_RA, step_keys)
            print_layout_table(results_ra)


if __name__ == "__main__":
    main()
