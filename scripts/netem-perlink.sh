#!/bin/sh
# Test the tc/netem per-link infrastructure itself: verify that the HTB + u32
# filter hierarchy is installed correctly, that different destination IPs get
# different impairment profiles, and that packet counts match expectations.
# Uses lightweight echo-server targets — no LiveKit involved.
#
# If you want to run LiveKit integration tests under a degraded network, use
# scripts/netem-livekit.sh instead.
#
# Usage:
#   scripts/netem-perlink.sh up [netem-args]   Start the per-link stack
#   scripts/netem-perlink.sh reup <netem-args> Update impairment live (no reconnect)
#   scripts/netem-perlink.sh down          Stop the per-link stack
#   scripts/netem-perlink.sh inspect       Show tc hierarchy inside the netem container
#   scripts/netem-perlink.sh digest        Show human-readable netem digest summary
#   scripts/netem-perlink.sh test          Run all perlink integration tests
#   scripts/netem-perlink.sh test infra    Run infrastructure tests only
#   scripts/netem-perlink.sh test product  Run product tests only
#
# The 'up' and 'reup' commands accept inline netem args for the default class:
#   scripts/netem-perlink.sh up delay 200ms loss 5%
#   scripts/netem-perlink.sh reup delay 50ms rate 1mbit
#
# Per-link overrides still use env vars:
#   NETEM_LINK_A_ARGS="delay 500ms" scripts/netem-perlink.sh up

set -eu

REPO_ROOT="$(git rev-parse --show-toplevel)"
. "$REPO_ROOT/scripts/netem-lib.sh"

COMPOSE="docker compose \
  -f $REPO_ROOT/docker-compose.yaml \
  -f $REPO_ROOT/docker-compose.netem.yml \
  -f $REPO_ROOT/docker-compose.netem-perlink.yml"

usage() {
    echo "Usage: $0 <command>"
    echo ""
    echo "Commands:"
    echo "  up [netem-args]   Start the stack with optional impairment"
    echo "  reup <netem-args> Update impairment live (no restart, no reconnect)"
    echo "  down              Stop and remove the stack"
    echo "  inspect           Show tc qdiscs, classes, and filters inside the netem container"
    echo "  digest            Show human-readable netem digest summary"
    echo "  test              Run all perlink integration tests"
    echo "  test infra        Run infrastructure tests only"
    echo "  test product      Run product tests only"
    echo ""
    echo "Examples:"
    echo "  $0 up                                  # start with default impairment"
    echo "  $0 up delay 200ms loss 5%              # start with custom impairment"
    echo "  $0 reup delay 300ms 50ms loss 10%      # change impairment live"
    echo "  $0 reup delay 50ms rate 1mbit          # add bandwidth cap"
    echo "  $0 reup delay 150ms 50ms loss 5%       # poor wifi"
    echo "  $0 reup default delay 80ms 20ms        # change only the default class"
    echo "  $0 digest                              # verify current settings"
    echo "  $0 test infra                          # run infrastructure tests"
    echo ""
    echo "Netem args are order-independent keywords: delay, loss, rate, duplicate,"
    echo "corrupt, reorder. Sub-args within a keyword are positional:"
    echo "  delay TIME [JITTER [CORRELATION]]"
    echo "  loss PERCENT [CORRELATION]"
    echo "  rate BANDWIDTH"
}

case "${1:-}" in
    up)
        shift
        if [ $# -gt 0 ]; then
            export NETEM_ARGS="$*"
        fi
        $COMPOSE up -d --wait
        ;;

    reup)
        shift
        $COMPOSE exec netem /bin/sh /netem-impair.sh "$@"
        ;;

    down)
        $COMPOSE down
        ;;

    inspect)
        netem_inspect
        ;;

    digest)
        netem_digest
        ;;

    test)
        $COMPOSE up -d --wait
        case "${2:-}" in
            "")
                cargo test -p remote_access_tests -- --ignored perlink_
                ;;
            infra)
                cargo test -p remote_access_tests -- --ignored perlink_qdisc_hierarchy
                cargo test -p remote_access_tests -- --ignored perlink_link_a
                cargo test -p remote_access_tests -- --ignored perlink_default_class
                ;;
            product)
                cargo test -p remote_access_tests -- --ignored perlink_viewer
                cargo test -p remote_access_tests -- --ignored perlink_burst
                ;;
            *)
                echo "ERROR: unknown test category '$2'" >&2
                echo "  Valid categories: infra, product" >&2
                exit 1
                ;;
        esac
        ;;

    *)
        usage >&2
        exit 1
        ;;
esac
