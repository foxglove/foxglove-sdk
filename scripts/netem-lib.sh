# Shared helper functions for netem management scripts (netem-perlink.sh,
# netem-livekit.sh). Source this file after setting REPO_ROOT.
#
# Provides:
#   netem_container_id   Find the running netem sidecar container ID.
#   netem_inspect         Dump raw tc hierarchy from inside the netem container.
#   netem_digest          Print a human-readable summary of tc state and traffic.

# Find the running netem sidecar container ID. Matches containers whose name
# ends with "-netem-<N>" (the compose default naming convention).
netem_container_id() {
    id=$(docker ps -q --filter "name=-netem-[0-9]+$" | head -1)
    if [ -z "$id" ]; then
        echo "ERROR: no running netem container found — is the stack running?" >&2
        echo "  Try: $0 up" >&2
        exit 1
    fi
    echo "$id"
}

# Dump raw tc qdiscs, classes, and filters for every interface inside the netem
# container. Useful for debugging; for a summarized view use netem_digest.
netem_inspect() {
    cid=$(netem_container_id)
    echo "=== netem container: $cid ==="
    for iface in $(docker exec "$cid" ls /sys/class/net/); do
        echo ""
        echo "--- $iface ---"
        for resource in qdisc class filter; do
            output=$(docker exec "$cid" tc -s "$resource" show dev "$iface" 2>/dev/null || true)
            if [ -n "$output" ]; then
                echo "  [$resource]"
                echo "$output" | sed 's/^/    /'
            fi
        done
    done
}

# Print a human-readable summary of the netem tc hierarchy. Collects raw tc
# data from the netem container and pipes it through the netem-digest binary,
# which correlates netem qdiscs with u32 filter destinations and formats
# impairment parameters plus traffic stats.
netem_digest() {
    cid=$(netem_container_id)
    # Collect all tc data in a single docker exec to avoid per-interface
    # round trips. Each interface is delimited by "===IFACE <name>".
    docker exec "$cid" sh -c '
        for iface in $(ls /sys/class/net/); do
            echo "===IFACE $iface"
            tc -s qdisc show dev "$iface" 2>/dev/null
            echo "---FILTERS---"
            tc -s filter show dev "$iface" 2>/dev/null
        done
    ' | netem-digest
}
