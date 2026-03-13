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

# Print a human-readable summary of the netem tc hierarchy. Correlates netem
# qdiscs with u32 filter destinations and shows impairment parameters plus
# traffic stats. Interfaces with no traffic are collapsed to one line; links
# with no traffic are omitted.
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
    ' | awk '
        # Convert an 8-digit hex string to dotted-decimal IP.
        function hex_to_ip(hex,    i, val, j, c, ip) {
            ip = ""
            for (i = 0; i < 4; i++) {
                val = 0
                for (j = 1; j <= 2; j++) {
                    c = index("0123456789abcdef", \
                              substr(hex, i * 2 + j, 1))
                    val = val * 16 + (c > 0 ? c - 1 : 0)
                }
                ip = (ip == "" ? val : ip "." val)
            }
            return ip
        }

        function fmt_bytes(n,    units, i) {
            if (n + 0 == 0) return "0 B"
            split("B,KB,MB,GB,TB", units, ",")
            i = 1
            while (n >= 1024 && i < 5) { n = n / 1024; i++ }
            return (i == 1) ? sprintf("%d %s", n, units[i]) \
                            : sprintf("%.1f %s", n, units[i])
        }

        function has_traffic(h) {
            return (sent_bytes[h]+0 > 0 || sent_pkts[h]+0 > 0 \
                    || dropped[h]+0 > 0)
        }

        function print_link(label, h) {
            printf "  %s\n", label
            printf "    impairment: %s\n", netem_params[h]
            printf "    traffic:    %s sent (%s packets), %s dropped\n", \
                fmt_bytes(sent_bytes[h]), sent_pkts[h]+0, dropped[h]+0
        }

        # Flush accumulated state for the previous interface.
        function flush_iface(    i, h, dst, any) {
            if (iface == "") return
            if (!has_netem) return

            # Check whether any qdisc on this interface has traffic.
            any = 0
            for (i = 0; i < lc; i++)
                if (has_traffic(handles[i])) { any = 1; break }
            if (!any) { printf "%s: no traffic\n", iface; return }

            if (is_flat) {
                h = handles[0]
                printf "%s:\n", iface
                printf "  impairment: %s\n", netem_params[h]
                printf "  traffic:    %s sent (%s packets), %s dropped\n", \
                    fmt_bytes(sent_bytes[h]), sent_pkts[h]+0, dropped[h]+0
                return
            }

            printf "%s: per-link\n", iface
            for (i = 0; i < lc; i++) {
                h = handles[i]
                if (h == default_class || !has_traffic(h)) continue
                dst = (filter_ip[h] != "" ? filter_ip[h] : "unknown")
                print_link("link 1:" h " -> " dst, h)
            }
            if (default_class != "" && netem_params[default_class] != "" \
                    && has_traffic(default_class))
                print_link("default (1:" default_class ")", default_class)
        }

        /^===IFACE / {
            flush_iface()
            # Reset per-interface state.
            iface = $2; section = "qdisc"; is_netem = 0; is_flat = 0
            has_netem = 0; lc = 0; default_class = ""
            delete netem_params; delete handles; delete sent_bytes
            delete sent_pkts; delete dropped; delete filter_ip
            next
        }

        /^---FILTERS---$/ { section = "filter"; next }

        section == "qdisc" && /^qdisc htb/ {
            is_netem = 0
            for (i = 1; i <= NF; i++)
                if ($i == "default") {
                    default_class = $(i+1); gsub(/^0x/, "", default_class)
                }
            next
        }

        section == "qdisc" && /^qdisc netem/ {
            current_handle = $3; gsub(/:$/, "", current_handle)
            is_netem = 1; has_netem = 1
            if ($4 == "root") is_flat = 1

            # Extract netem parameters: everything after "limit <N>".
            params = ""; past_limit = 0
            for (i = 1; i <= NF; i++) {
                if ($i == "limit" && $(i+1) ~ /^[0-9]+$/) {
                    past_limit = 1; i++; continue
                }
                if (past_limit) params = (params == "" ? $i : params " " $i)
            }
            netem_params[current_handle] = \
                (params == "" ? "(no impairment)" : params)
            handles[lc++] = current_handle
            next
        }

        section == "qdisc" && is_netem && /Sent/ {
            for (i = 1; i <= NF; i++) {
                if ($i == "Sent") sent_bytes[current_handle] = $(i+1)
                if ($i == "pkt") sent_pkts[current_handle] = $(i-1)
                if ($i == "(dropped") {
                    d = $(i+1); gsub(/,/, "", d)
                    dropped[current_handle] = d
                }
            }
            is_netem = 0; next
        }

        section == "qdisc" && /^qdisc/ { is_netem = 0 }

        section == "filter" && /flowid/ {
            for (i = 1; i <= NF; i++)
                if ($i == "flowid") {
                    current_filter_class = $(i+1)
                    sub(/^1:/, "", current_filter_class)
                }
            next
        }

        section == "filter" && /match .* at 16/ {
            split($2, parts, "/")
            filter_ip[current_filter_class] = hex_to_ip(parts[1])
            next
        }

        END { flush_iface() }
    '
}
