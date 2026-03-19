#!/usr/bin/env python3
"""Set up tc/netem rules on all network interfaces inside the netem sidecar.

Supports two modes:

  1. Flat mode (default): applies a single netem qdisc to all interfaces.
     Controlled by the NETEM_ARGS env var.

  2. Per-link mode: uses an HTB root qdisc with separate netem leaf classes
     for different destination IPs. Enabled when any NETEM_LINK_<name>_DST
     env var is set. Each link gets its own impairment profile; unclassified
     traffic falls into a default class using NETEM_ARGS.

Per-link env vars follow the pattern:
  NETEM_LINK_<NAME>_DST   — destination IP to classify (required per link)
  NETEM_LINK_<NAME>_ARGS  — netem arguments for this link (defaults to NETEM_ARGS)
"""

import os
import re
import shlex
import subprocess
import sys
from pathlib import Path


def tc(*args: str) -> bool:
    """Run a tc command. Returns True on success, False on failure."""
    result = subprocess.run(
        ["tc", *args], capture_output=True, text=True  # noqa: S603, S607
    )
    return result.returncode == 0


def tc_show(*args: str) -> str:
    """Run a tc command and return stdout."""
    result = subprocess.run(
        ["tc", *args], capture_output=True, text=True  # noqa: S603, S607
    )
    return result.stdout


def interfaces() -> list[str]:
    """List network interface names to apply netem rules to.

    By default includes all interfaces (lo, eth0, tunnel interfaces, etc.) so
    netem rules cover both Docker (eth0) and Podman rootless/pasta (lo)
    networking paths.

    Set NETEM_SKIP_LOOPBACK=1 to exclude the loopback interface. This is needed
    for per-container sidecars where the WebRTC stack uses loopback internally
    for ICE candidate gathering.
    """
    skip_lo = os.environ.get("NETEM_SKIP_LOOPBACK", "") == "1"
    return sorted(
        p.name for p in Path("/sys/class/net").iterdir()
        if not (skip_lo and p.name == "lo")
    )


def discover_links() -> dict[str, str]:
    """Discover per-link definitions from NETEM_LINK_*_DST env vars.

    Returns a dict of link name -> destination IP, skipping empty values.
    """
    links: dict[str, str] = {}
    for key, value in sorted(os.environ.items()):
        m = re.match(r"^NETEM_LINK_(.+)_DST$", key)
        if m and value:
            links[m.group(1)] = value
    return links


def apply_flat(netem_args: list[str]) -> None:
    """Flat mode: apply a single root netem qdisc to all interfaces."""
    for iface in interfaces():
        if tc("qdisc", "replace", "dev", iface, "root", "netem", *netem_args):
            print(f"netem (flat) applied to {iface}: {' '.join(netem_args)}")
        else:
            print(f"  WARNING: failed to apply netem to {iface} (may be expected for lo)")


def apply_perlink(netem_args: list[str], links: dict[str, str]) -> int:
    """Per-link mode: HTB root with netem leaf classes per destination IP.

    Returns the number of errors encountered.
    """
    errors = 0

    for iface in interfaces():
        print(f"configuring per-link netem on {iface}...")

        # HTB root qdisc. Unclassified traffic goes to default class 1:ff00.
        if not tc("qdisc", "replace", "dev", iface, "root", "handle", "1:", "htb", "default", "ff00"):
            print(f"  WARNING: failed to add HTB root qdisc on {iface} (skipping)")
            continue

        # Default class (unclassified traffic).
        ok = True
        if not tc("class", "add", "dev", iface, "parent", "1:", "classid", "1:ff00", "htb", "rate", "10gbit"):
            print(f"  ERROR: failed to add default class on {iface}")
            errors += 1
            ok = False
        if not tc("qdisc", "add", "dev", iface, "parent", "1:ff00", "handle", "ff00:", "netem", *netem_args):
            print(f"  ERROR: failed to add netem qdisc on default class ({iface})")
            errors += 1
            ok = False
        if ok:
            print(f"  default class 1:ff00 -> netem {' '.join(netem_args)}")

        # Per-link classes. Assign class IDs starting at 1:10, incrementing by 0x10.
        class_minor = 0x10
        for name, dst in links.items():
            link_args_str = os.environ.get(f"NETEM_LINK_{name}_ARGS", "")
            link_args = shlex.split(link_args_str) if link_args_str else netem_args

            class_id = f"1:{class_minor:x}"
            handle = f"{class_minor:x}:"

            link_ok = True
            if not tc("class", "add", "dev", iface, "parent", "1:", "classid", class_id, "htb", "rate", "10gbit"):
                print(f"  ERROR: failed to add class {class_id} on {iface}")
                errors += 1
                link_ok = False
            if not tc("qdisc", "add", "dev", iface, "parent", class_id, "handle", handle, "netem", *link_args):
                print(f"  ERROR: failed to add netem qdisc on class {class_id} ({iface})")
                errors += 1
                link_ok = False
            if not tc("filter", "add", "dev", iface, "parent", "1:", "protocol", "ip", "u32",
                       "match", "ip", "dst", f"{dst}/32", "flowid", class_id):
                print(f"  ERROR: failed to add u32 filter for {dst} on {iface}")
                errors += 1
                link_ok = False
            if link_ok:
                print(f"  link {name}: class {class_id} -> dst {dst} -> netem {' '.join(link_args)}")

            class_minor += 0x10

    return errors


def dump_state() -> None:
    """Print final tc state for debugging."""
    for iface in interfaces():
        for what in ("qdisc", "class", "filter"):
            print(f"\n=== {iface}: tc {what} ===")
            print(tc_show("-s", what, "show", "dev", iface), end="")


def main() -> None:
    netem_args = shlex.split(os.environ.get("NETEM_ARGS", "delay 80ms 20ms loss 2%"))
    links = discover_links()

    if links:
        errors = apply_perlink(netem_args, links)
    else:
        apply_flat(netem_args)
        errors = 0

    dump_state()

    if errors > 0:
        print(f"\nERROR: {errors} tc command(s) failed during per-link setup.")
        sys.exit(1)


if __name__ == "__main__":
    main()
