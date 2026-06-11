#!/usr/bin/env python3
"""Update netem qdisc parameters live without rebuilding the tc hierarchy.

Runs inside a netem sidecar container.

Usage:
  netem_impair.py <netem-args>               Update all netem qdiscs
  netem_impair.py default <netem-args>       Update only the default class (ff00:)
  netem_impair.py link <NAME> <netem-args>   Update only the NETEM_LINK_<NAME>_* class
"""

import os
import re
import subprocess
import sys
from pathlib import Path


def discover_link_handles() -> dict[str, str]:
    """Map link names to their netem qdisc handles.

    Mirrors the assignment in netem_setup.py exactly: links are discovered
    from NETEM_LINK_<NAME>_DST env vars in sorted-env-key order and get
    handles 10:, 20:, ... in that order. The sidecar's env is identical to
    what netem_setup.py saw at startup, so the mapping is deterministic —
    but keep the two functions in sync.
    """
    handles: dict[str, str] = {}
    class_minor = 0x10
    for key, value in sorted(os.environ.items()):
        m = re.match(r"^NETEM_LINK_(.+)_DST$", key)
        if m and value:
            handles[m.group(1)] = f"{class_minor:x}:"
            class_minor += 0x10
    return handles


def usage_error() -> None:
    print("Usage: netem_impair.py [default | link <NAME>] <netem-args>", file=sys.stderr)
    sys.exit(1)


def main() -> None:
    args = sys.argv[1:]

    # `target` is the qdisc handle to update, or "all" for every netem qdisc.
    target = "all"
    if args and args[0] == "default":
        target = "ff00:"
        args = args[1:]
    elif args and args[0] == "link":
        if len(args) < 2:
            usage_error()
        handles = discover_link_handles()
        name = args[1]
        if name not in handles:
            known = ", ".join(handles) or "(none — this sidecar runs in flat mode)"
            print(
                f"ERROR: unknown link '{name}'. Links on this sidecar: {known}",
                file=sys.stderr,
            )
            sys.exit(1)
        target = handles[name]
        args = args[2:]

    if not args:
        usage_error()

    # Arguments are already a list from sys.argv — no shell parsing needed.
    netem_args = args

    # Normalize `rate` so every invocation fully replaces the qdisc settings.
    #
    # `tc qdisc change` overwrites delay/loss/jitter unconditionally — they live
    # in the base tc_netem_qopt struct. But `rate` rides a separate
    # TCA_NETEM_RATE attribute that the kernel only re-applies when `rate` is on
    # the command line; a bare change leaves any previous rate cap in place. That
    # silently breaks A/B comparisons: e.g. `delay 100ms rate 2mbit` followed by
    # `delay 0ms` leaves the 2mbit cap intact instead of clearing it. When the
    # caller omits `rate`, append an effectively-uncapped value so "no rate" means
    # "no rate limit". 1000gbit is deliberately far above the `rate 10gbit` HTB
    # classes in netem_setup.py: in classful (per-link) mode that HTB ceiling
    # bounds throughput regardless of this value, while in flat mode netem is the
    # root qdisc with nothing above it, so this value itself must be high enough
    # to never be the bottleneck.
    if "rate" not in netem_args:
        netem_args = [*netem_args, "rate", "1000gbit"]

    errors = 0
    updated = 0

    for iface in sorted(p.name for p in Path("/sys/class/net").iterdir()):
        result = subprocess.run(
            ["tc", "qdisc", "show", "dev", iface],  # noqa: S603, S607
            capture_output=True,
            text=True,
        )
        for line in result.stdout.splitlines():
            if "qdisc netem" not in line:
                continue

            # Parse: "qdisc netem <handle> root ..." or "qdisc netem <handle> parent <class> ..."
            parts = line.split()
            handle = parts[2]
            if parts[3] == "root":
                parent_args = ["root"]
            else:
                parent_args = ["parent", parts[4]]

            if target != "all" and handle != target:
                continue

            change_result = subprocess.run(
                [
                    "tc",
                    "qdisc",
                    "change",
                    "dev",
                    iface,
                    *parent_args,  # noqa: S603, S607
                    "handle",
                    handle,
                    "netem",
                    *netem_args,
                ],
                capture_output=True,
                text=True,
            )
            if change_result.returncode == 0:
                # `handle` already ends with ":" (e.g. "10:"), so no separator.
                print(f"  {iface} {handle} netem {' '.join(netem_args)}")
                updated += 1
            else:
                print(f"  ERROR: {iface} {handle}", file=sys.stderr)
                errors += 1

    if errors > 0:
        print(f"ERROR: {errors} update(s) failed", file=sys.stderr)
        sys.exit(1)
    if updated == 0:
        # Nothing matched: the hierarchy was never set up, or a targeted
        # handle has no qdisc (e.g. `default` on a flat-mode sidecar, whose
        # root netem has a different handle).
        print("ERROR: no matching netem qdisc found", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
