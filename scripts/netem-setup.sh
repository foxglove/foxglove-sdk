#!/bin/sh
# Set up tc/netem rules on all network interfaces inside the netem sidecar
# container. Supports two modes:
#
#   1. Flat mode (default): applies a single netem qdisc to all interfaces.
#      Controlled by the NETEM_ARGS env var.
#
#   2. Per-link mode: uses an HTB root qdisc with separate netem leaf classes
#      for different destination IPs. Enabled when any NETEM_LINK_<name>_DST
#      env var is set. Each link gets its own impairment profile; unclassified
#      traffic falls into a default class using NETEM_ARGS.
#
# Per-link env vars follow the pattern:
#   NETEM_LINK_<NAME>_DST   — destination IP to classify (required per link)
#   NETEM_LINK_<NAME>_ARGS  — netem arguments for this link (defaults to NETEM_ARGS)
#
# Example:
#   NETEM_ARGS="delay 80ms 20ms loss 2%"
#   NETEM_LINK_SDK_DST="172.18.0.3"
#   NETEM_LINK_SDK_ARGS="delay 300ms 100ms loss 10%"
#   NETEM_LINK_APP_DST="172.18.0.1"
#   NETEM_LINK_APP_ARGS="delay 20ms 5ms"

set -eu

NETEM_ARGS="${NETEM_ARGS:-delay 80ms 20ms loss 2%}"

# Track whether any per-link tc command fails on an interface that accepted the
# HTB root. We continue past failures to apply rules on other interfaces, but
# exit non-zero at the end so the container healthcheck can detect partial setup.
# Flat-mode failures are logged but not tracked here since some interfaces
# (e.g. lo) legitimately don't support netem.
SETUP_ERRORS=0

# ---------------------------------------------------------------------------
# Discover per-link definitions from NETEM_LINK_*_DST env vars.
# ---------------------------------------------------------------------------

# Collect unique link names by scanning env vars for the NETEM_LINK_*_DST pattern.
LINK_NAMES=""
for var in $(env | grep '^NETEM_LINK_.*_DST=' | sed 's/=.*//' | sort); do
    # Extract the link name: NETEM_LINK_<NAME>_DST -> <NAME>
    name=$(echo "$var" | sed 's/^NETEM_LINK_//;s/_DST$//')
    LINK_NAMES="$LINK_NAMES $name"
done

# Trim leading space.
LINK_NAMES=$(echo "$LINK_NAMES" | sed 's/^ //')

# ---------------------------------------------------------------------------
# Apply rules to each interface.
# ---------------------------------------------------------------------------

for iface in $(ls /sys/class/net/); do
    if [ -z "$LINK_NAMES" ]; then
        # Flat mode: single root netem qdisc. Some interfaces (e.g. lo) may
        # not support netem; failures are logged but not fatal since at least
        # one interface (eth0) must succeed for the tests to work.
        # shellcheck disable=SC2086
        tc qdisc replace dev "$iface" root netem $NETEM_ARGS 2>/dev/null \
            && echo "netem (flat) applied to $iface: $NETEM_ARGS" \
            || echo "  WARNING: failed to apply netem to $iface (may be expected for lo)"
    else
        # Per-link mode: HTB root with netem leaf classes.
        echo "configuring per-link netem on $iface..."

        # HTB root qdisc. Unclassified traffic goes to default class 1:ff00.
        # Use a high class ID for the default to leave room for link classes.
        # Failure here (e.g. on lo) means we can't add child classes, so skip
        # this interface but still track the error.
        tc qdisc replace dev "$iface" root handle 1: htb default ff00 2>/dev/null \
            || { echo "  ERROR: failed to add HTB root qdisc on $iface"; SETUP_ERRORS=$((SETUP_ERRORS + 1)); continue; }

        # Default class (unclassified traffic).
        tc class add dev "$iface" parent 1: classid 1:ff00 htb rate 10gbit 2>/dev/null \
            || { echo "  ERROR: failed to add default class on $iface"; SETUP_ERRORS=$((SETUP_ERRORS + 1)); }
        # shellcheck disable=SC2086
        tc qdisc add dev "$iface" parent 1:ff00 handle ff00: netem $NETEM_ARGS 2>/dev/null \
            || { echo "  ERROR: failed to add netem qdisc on default class ($iface)"; SETUP_ERRORS=$((SETUP_ERRORS + 1)); }
        echo "  default class 1:ff00 -> netem $NETEM_ARGS"

        # Per-link classes. Assign class IDs starting at 1:10, incrementing by 10.
        class_minor=10
        for name in $LINK_NAMES; do
            dst_var="NETEM_LINK_${name}_DST"
            args_var="NETEM_LINK_${name}_ARGS"

            # eval is used for variable indirection — the variable names are
            # derived from env var keys we control (filtered by the grep
            # pattern above). The resulting values ($link_args) are passed
            # unquoted to tc (intentional word-splitting); callers must not
            # include shell metacharacters in NETEM_LINK_*_ARGS values.
            eval "dst=\${$dst_var:-}"
            eval "link_args=\${$args_var:-}"
            link_args="${link_args:-$NETEM_ARGS}"

            if [ -z "$dst" ]; then
                echo "  WARNING: $dst_var is empty, skipping link $name"
                continue
            fi

            class_id="1:$(printf '%x' $class_minor)"
            handle="$(printf '%x' $class_minor):"

            tc class add dev "$iface" parent 1: classid "$class_id" htb rate 10gbit 2>/dev/null \
                || { echo "  ERROR: failed to add class $class_id on $iface"; SETUP_ERRORS=$((SETUP_ERRORS + 1)); }
            # shellcheck disable=SC2086
            tc qdisc add dev "$iface" parent "$class_id" handle "$handle" netem $link_args 2>/dev/null \
                || { echo "  ERROR: failed to add netem qdisc on class $class_id ($iface)"; SETUP_ERRORS=$((SETUP_ERRORS + 1)); }
            tc filter add dev "$iface" parent 1: protocol ip u32 \
                match ip dst "$dst/32" flowid "$class_id" 2>/dev/null \
                || { echo "  ERROR: failed to add u32 filter for $dst on $iface"; SETUP_ERRORS=$((SETUP_ERRORS + 1)); }
            echo "  link $name: class $class_id -> dst $dst -> netem $link_args"

            class_minor=$((class_minor + 10))
        done
    fi
done

# Print final state for debugging. Iterate over all interfaces since per-link
# rules may be applied to a non-default interface (e.g. eth1 for the perlink
# network).
for iface in $(ls /sys/class/net/); do
    echo ""
    echo "=== $iface: tc qdisc ==="
    tc -s qdisc show dev "$iface" 2>/dev/null || true
    echo "=== $iface: tc class ==="
    tc -s class show dev "$iface" 2>/dev/null || true
    echo "=== $iface: tc filter ==="
    tc -s filter show dev "$iface" 2>/dev/null || true
done

# Print error summary last (after debug dump) so it's visible at the end of
# the log. Exit non-zero so the container healthcheck can detect partial setup.
if [ "$SETUP_ERRORS" -gt 0 ]; then
    echo ""
    echo "ERROR: $SETUP_ERRORS tc command(s) failed during per-link setup."
    exit 1
fi
