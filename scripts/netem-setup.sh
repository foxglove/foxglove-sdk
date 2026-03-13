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

# Validate that a netem args string contains only safe characters. Netem args
# are passed unquoted to tc (intentional word-splitting), so shell
# metacharacters (;, &, |, $, `, etc.) would be interpreted by the shell.
validate_netem_args() {
    case "$2" in
        *[';|&$`()\{\}\"'\''!'\\*?]* | *'>'* | *'<'* | *'['*)
            echo "ERROR: $1 contains shell metacharacters: $2"
            echo "Only netem parameters are allowed (e.g. 'delay 200ms 50ms loss 5%')."
            exit 1
            ;;
    esac
}

validate_netem_args "NETEM_ARGS" "$NETEM_ARGS"

# Track whether any leaf tc command (class, qdisc, filter) fails on an
# interface that accepted the HTB root. We continue past failures to apply
# rules on other interfaces, but exit non-zero at the end so the container
# healthcheck can detect partial setup. HTB root failures and flat-mode
# failures are logged as warnings but not tracked here, since some interfaces
# (e.g. lo) legitimately don't support these qdiscs.
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
        # Flat mode: single root netem qdisc. Apply to all interfaces to cover
        # both Docker (eth0) and Podman rootless/pasta (which may use different
        # interface names). Some interfaces (e.g. lo) may not support netem;
        # failures are logged but not fatal since at least one must succeed.
        # shellcheck disable=SC2086
        tc qdisc replace dev "$iface" root netem $NETEM_ARGS 2>/dev/null \
            && echo "netem (flat) applied to $iface: $NETEM_ARGS" \
            || echo "  WARNING: failed to apply netem to $iface (may be expected for lo)"
    else
        # Per-link mode: HTB root with netem leaf classes. Applied to all
        # interfaces (same Docker/Podman rationale as flat mode).
        echo "configuring per-link netem on $iface..."

        # HTB root qdisc. Unclassified traffic goes to default class 1:ff00.
        # Use a high class ID for the default to leave room for link classes.
        # Failure here (e.g. on lo) means we can't add child classes, so skip
        # this interface. This is not counted as a setup error — same as flat
        # mode, some interfaces legitimately don't support HTB.
        tc qdisc replace dev "$iface" root handle 1: htb default ff00 2>/dev/null \
            || { echo "  WARNING: failed to add HTB root qdisc on $iface (skipping)"; continue; }

        # Default class (unclassified traffic). Only log success if both
        # commands succeed; failures are already logged by the || clauses.
        default_ok=true
        tc class add dev "$iface" parent 1: classid 1:ff00 htb rate 10gbit 2>/dev/null \
            || { echo "  ERROR: failed to add default class on $iface"; SETUP_ERRORS=$((SETUP_ERRORS + 1)); default_ok=false; }
        # shellcheck disable=SC2086
        tc qdisc add dev "$iface" parent 1:ff00 handle ff00: netem $NETEM_ARGS 2>/dev/null \
            || { echo "  ERROR: failed to add netem qdisc on default class ($iface)"; SETUP_ERRORS=$((SETUP_ERRORS + 1)); default_ok=false; }
        if [ "$default_ok" = true ]; then
            echo "  default class 1:ff00 -> netem $NETEM_ARGS"
        fi

        # Per-link classes. Assign class IDs starting at 1:10, incrementing by 10.
        class_minor=10
        for name in $LINK_NAMES; do
            dst_var="NETEM_LINK_${name}_DST"
            args_var="NETEM_LINK_${name}_ARGS"

            # Use printenv for variable indirection — avoids eval and any
            # risk of shell metacharacter interpretation in the values.
            dst=$(printenv "$dst_var" 2>/dev/null || true)
            link_args=$(printenv "$args_var" 2>/dev/null || true)
            link_args="${link_args:-$NETEM_ARGS}"
            validate_netem_args "$args_var" "$link_args"

            if [ -z "$dst" ]; then
                echo "  WARNING: $dst_var is empty, skipping link $name"
                continue
            fi

            class_id="1:$(printf '%x' $class_minor)"
            handle="$(printf '%x' $class_minor):"

            # Only log success if all three commands succeed; failures are
            # already logged by the || clauses.
            link_ok=true
            tc class add dev "$iface" parent 1: classid "$class_id" htb rate 10gbit 2>/dev/null \
                || { echo "  ERROR: failed to add class $class_id on $iface"; SETUP_ERRORS=$((SETUP_ERRORS + 1)); link_ok=false; }
            # shellcheck disable=SC2086
            tc qdisc add dev "$iface" parent "$class_id" handle "$handle" netem $link_args 2>/dev/null \
                || { echo "  ERROR: failed to add netem qdisc on class $class_id ($iface)"; SETUP_ERRORS=$((SETUP_ERRORS + 1)); link_ok=false; }
            tc filter add dev "$iface" parent 1: protocol ip u32 \
                match ip dst "$dst/32" flowid "$class_id" 2>/dev/null \
                || { echo "  ERROR: failed to add u32 filter for $dst on $iface"; SETUP_ERRORS=$((SETUP_ERRORS + 1)); link_ok=false; }
            if [ "$link_ok" = true ]; then
                echo "  link $name: class $class_id -> dst $dst -> netem $link_args"
            fi

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
