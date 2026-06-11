#!/usr/bin/env bash
# Diagnostics for the netem-perlink-test CI flake (FLE-595).
#
# In failing runs, the UDP echo measurement in
# rust/remote_access_tests/tests/netem_perlink_test.rs observes ~98-99% loss to
# both targets while TCP echoes succeed, far beyond the configured netem loss
# (5% / 0%). This script localizes where UDP datagrams die: it prints tool
# versions, tc qdisc/class/filter state with counters, NIC offload settings,
# and runs a per-packet UDP echo probe with tcpdump capturing at both ends
# (sidecar egress and target ingress).
#
# Intended to run on the CI runner after `docker compose up` of the perlink
# stack (docker-compose.netem-perlink.yml). Safe to run repeatedly; it only
# adds diagnostic packages (tcpdump, ethtool) inside the containers.
set -uo pipefail

PROBE_COUNT=${PROBE_COUNT:-30}
TARGET_A_IP=10.98.0.10
TARGET_B_IP=10.98.0.20
UDP_PORT=7001
TCP_PORT=7000

section() {
  echo
  echo "##### $* #####"
}

# Find a container ID by compose service name suffix.
find_container() {
  docker ps -q --filter "name=-$1-[0-9]*" --filter status=running | head -n 1
}

NETEM=$(find_container netem)
TARGET_A=$(find_container target-a)
TARGET_B=$(find_container target-b)
echo "netem=$NETEM target-a=$TARGET_A target-b=$TARGET_B"
if [ -z "$NETEM" ] || [ -z "$TARGET_A" ] || [ -z "$TARGET_B" ]; then
  echo "ERROR: missing containers; is the perlink stack up?"
  exit 1
fi

section "host versions"
uname -a
docker version --format 'docker client={{.Client.Version}} server={{.Server.Version}}' || true

section "container package versions"
for c in "$NETEM" "$TARGET_A"; do
  echo "--- container $c ---"
  docker exec "$c" sh -c 'apk list --installed 2>/dev/null | grep -E "socat|iproute2|busybox|musl|alpine-baselayout"' || true
  docker exec "$c" sh -c 'socat -V 2>&1 | head -n 2' || true
done
docker exec "$NETEM" tc -V || true

section "install diagnostic tools (tcpdump, ethtool) in netem + target-a"
docker exec "$NETEM" sh -c 'apk add --no-cache tcpdump ethtool >/dev/null 2>&1; which tcpdump ethtool' || true
docker exec "$TARGET_A" sh -c 'apk add --no-cache tcpdump >/dev/null 2>&1; which tcpdump' || true

section "netem namespace: interfaces, routes, neighbors"
docker exec "$NETEM" sh -c 'ip -d addr; echo; ip route; echo; ip neigh' || true

section "netem namespace: offload settings per interface"
docker exec "$NETEM" sh -c 'for i in $(ls /sys/class/net/); do echo "--- $i ---"; ethtool -k "$i" 2>/dev/null | grep -E "checksum|segmentation|scatter|gro|gso" ; done' || true

tc_snapshot() {
  docker exec "$NETEM" sh -c 'for i in $(ls /sys/class/net/); do
    echo "--- dev $i ---";
    tc -s qdisc show dev "$i" 2>/dev/null;
    tc -s class show dev "$i" 2>/dev/null;
    tc -s filter show dev "$i" parent 1: 2>/dev/null;
  done' || true
}

section "tc state BEFORE probe (with counters)"
tc_snapshot

section "UDP /proc counters BEFORE probe (netem ns and target-a)"
docker exec "$NETEM" sh -c 'grep Udp: /proc/net/snmp' || true
docker exec "$TARGET_A" sh -c 'grep Udp: /proc/net/snmp' || true

# Run tcpdump at both ends during the probe. Sidecar capture sees requests
# leaving and replies arriving; target capture sees requests arriving and
# replies leaving. Comparing the four counts localizes the drop.
section "UDP echo probe: $PROBE_COUNT packets per target, tcpdump at both ends"
docker exec "$NETEM" sh -c "nohup tcpdump -i any -n -l udp port $UDP_PORT > /tmp/cap-netem.txt 2>/dev/null & echo started" || true
docker exec "$TARGET_A" sh -c "nohup tcpdump -i any -n -l udp port $UDP_PORT > /tmp/cap-target.txt 2>/dev/null & echo started" || true
sleep 2

probe_udp() {
  local ip=$1 label=$2
  docker exec "$NETEM" sh -c '
    ok=0
    pattern=""
    for i in $(seq 1 '"$PROBE_COUNT"'); do
      resp=$(echo "probe-$i" | timeout 1 socat -T0.5 - UDP-CONNECT:'"$ip"':'"$UDP_PORT"' 2>/dev/null)
      if [ -n "$resp" ]; then ok=$((ok + 1)); pattern="${pattern}O"; else pattern="${pattern}."; fi
    done
    echo "'"$label"' received $ok/'"$PROBE_COUNT"'  pattern: $pattern"
  ' || true
}

probe_tcp() {
  local ip=$1 label=$2
  docker exec "$NETEM" sh -c '
    ok=0
    for i in $(seq 1 5); do
      resp=$(echo "tcp-$i" | timeout 6 socat -T5 - TCP:'"$ip"':'"$TCP_PORT"' 2>/dev/null)
      [ -n "$resp" ] && ok=$((ok + 1))
    done
    echo "'"$label"' TCP echoes ok: $ok/5"
  ' || true
}

probe_udp "$TARGET_A_IP" "link A ($TARGET_A_IP)"
probe_udp "$TARGET_B_IP" "link B ($TARGET_B_IP)"
probe_tcp "$TARGET_A_IP" "link A ($TARGET_A_IP)"
probe_tcp "$TARGET_B_IP" "link B ($TARGET_B_IP)"

sleep 2
docker exec "$NETEM" sh -c 'pkill tcpdump' 2>/dev/null || true
docker exec "$TARGET_A" sh -c 'pkill tcpdump' 2>/dev/null || true
sleep 1

section "capture summary"
echo "--- sidecar (netem ns): requests out / replies in ---"
docker exec "$NETEM" sh -c "
  echo \"requests seen: \$(grep -c \"> $TARGET_A_IP.$UDP_PORT\" /tmp/cap-netem.txt) to A, \$(grep -c \"> $TARGET_B_IP.$UDP_PORT\" /tmp/cap-netem.txt) to B\"
  echo \"replies seen:  \$(grep -c \"$TARGET_A_IP.$UDP_PORT >\" /tmp/cap-netem.txt) from A, \$(grep -c \"$TARGET_B_IP.$UDP_PORT >\" /tmp/cap-netem.txt) from B\"
" || true
echo "--- target-a: requests in / replies out ---"
docker exec "$TARGET_A" sh -c "
  echo \"requests seen: \$(grep -c \"> $TARGET_A_IP.$UDP_PORT\" /tmp/cap-target.txt)\"
  echo \"replies seen:  \$(grep -c \"$TARGET_A_IP.$UDP_PORT >\" /tmp/cap-target.txt)\"
" || true
echo "--- first/last 10 lines of sidecar capture ---"
docker exec "$NETEM" sh -c 'head -n 10 /tmp/cap-netem.txt; echo ...; tail -n 10 /tmp/cap-netem.txt' || true

section "tc state AFTER probe (with counters)"
tc_snapshot

section "UDP /proc counters AFTER probe"
docker exec "$NETEM" sh -c 'grep Udp: /proc/net/snmp' || true
docker exec "$TARGET_A" sh -c 'grep Udp: /proc/net/snmp' || true

section "target-a process state (echo servers)"
docker exec "$TARGET_A" sh -c 'ps -o pid,ppid,stat,args | head -n 30' || true

section "host: conntrack, bridge, dmesg"
sudo sysctl net.netfilter.nf_conntrack_count net.netfilter.nf_conntrack_max 2>/dev/null || true
docker network inspect "$(docker network ls -q --filter name=netem-targets)" --format '{{.Name}} {{.Driver}} {{json .Options}}' 2>/dev/null || true
sudo dmesg 2>/dev/null | tail -n 40 || true

# The probe's UDP datagrams cross the docker bridge, traversing the host's
# FORWARD path. A host-level firewall rule (e.g. a UDP rate limit injected by
# runner tooling) would drop them while leaving TCP and host<->container
# traffic alone, so dump the full rulesets with packet counters.
section "host: firewall rulesets with counters"
sudo iptables -L FORWARD -v -n 2>/dev/null || true
sudo iptables-save -c 2>/dev/null || true
sudo nft list ruleset 2>/dev/null || true

# Drops could also come from eBPF programs attached by host tooling, which
# would not appear in the nftables/iptables rulesets.
section "host: eBPF programs (if visible)"
sudo bpftool prog list 2>/dev/null || echo "bpftool unavailable"
sudo bpftool net list 2>/dev/null || true
ls /sys/fs/bpf 2>/dev/null || true

echo
echo "diagnostics complete"
