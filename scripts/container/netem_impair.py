#!/usr/bin/env python3
"""Update netem qdisc parameters live without rebuilding the tc hierarchy.

Runs inside a netem sidecar container.

Usage:
  netem_impair.py <netem-args>           Update all netem qdiscs
  netem_impair.py default <netem-args>   Update only the default class (ff00:)
"""

import subprocess
import sys
from pathlib import Path


def main() -> None:
    args = sys.argv[1:]

    target = "all"
    if args and args[0] == "default":
        target = "default"
        args = args[1:]

    if not args:
        print("Usage: netem_impair.py [default] <netem-args>", file=sys.stderr)
        sys.exit(1)

    # Arguments are already a list from sys.argv — no shell parsing needed.
    netem_args = args
    errors = 0

    for iface in sorted(p.name for p in Path("/sys/class/net").iterdir()):
        result = subprocess.run(
            ["tc", "qdisc", "show", "dev", iface],  # noqa: S603, S607
            capture_output=True, text=True,
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

            if target == "default" and handle != "ff00:":
                continue

            result = subprocess.run(
                ["tc", "qdisc", "change", "dev", iface, *parent_args,  # noqa: S603, S607
                 "handle", handle, "netem", *netem_args],
                capture_output=True, text=True,
            )
            if result.returncode == 0:
                print(f"  {iface} {handle}: netem {' '.join(netem_args)}")
            else:
                print(f"  ERROR: {iface} {handle}", file=sys.stderr)
                errors += 1

    if errors > 0:
        print(f"ERROR: {errors} update(s) failed", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
