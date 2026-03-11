#!/bin/sh
# Update netem qdisc parameters live without rebuilding the tc hierarchy.
# Runs inside the netem sidecar container. Existing connections are not
# disrupted — only newly enqueued packets use the updated parameters.
# Packets already queued drain with old parameters.
#
# Usage:
#   netem-impair.sh <netem-args>           Update all netem qdiscs
#   netem-impair.sh default <netem-args>   Update only the default class (ff00:)
#
# All desired parameters must be specified — omitted params revert to defaults.
# For example, "delay 200ms" removes any previously configured loss.

set -eu

TARGET="all"
if [ "${1:-}" = "default" ]; then
    TARGET="default"
    shift
fi

if [ $# -eq 0 ]; then
    echo "Usage: netem-impair.sh [default] <netem-args>"
    echo ""
    echo "Examples:"
    echo "  netem-impair.sh delay 200ms 50ms loss 5%"
    echo "  netem-impair.sh default delay 80ms 20ms loss 2%"
    echo "  netem-impair.sh delay 50ms rate 1mbit"
    exit 1
fi

ARGS="$*"

# Reject shell metacharacters — same validation as netem-setup.sh.
case "$ARGS" in
    *[';|&$`()\{\}\"'\''!'\\*?]* | *'>'* | *'<'* | *'['*)
        echo "ERROR: netem args contain shell metacharacters: $ARGS"
        echo "Only netem parameters are allowed (e.g. 'delay 200ms 50ms loss 5%')."
        exit 1
        ;;
esac

for iface in $(ls /sys/class/net/); do
    # Parse netem qdiscs from tc output. Each line looks like:
    #   qdisc netem <handle> root ...
    #   qdisc netem <handle> parent <class> ...
    tc qdisc show dev "$iface" 2>/dev/null | grep 'qdisc netem' | while IFS= read -r line; do
        handle=$(echo "$line" | awk '{print $3}')
        kind=$(echo "$line" | awk '{print $4}')

        if [ "$kind" = "root" ]; then
            parent_arg="root"
        else
            parent=$(echo "$line" | awk '{print $5}')
            parent_arg="parent $parent"
        fi

        # In per-link mode, the default class uses handle ff00: (set by
        # netem-setup.sh). Skip non-default qdiscs when targeting "default".
        if [ "$TARGET" = "default" ] && [ "$handle" != "ff00:" ]; then
            continue
        fi

        # shellcheck disable=SC2086
        if tc qdisc change dev "$iface" $parent_arg handle "$handle" netem $ARGS; then
            echo "  $iface handle $handle: netem $ARGS"
        else
            echo "  WARNING: failed to update $iface handle $handle" >&2
        fi
    done
done
