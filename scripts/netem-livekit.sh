#!/bin/sh
# Run LiveKit integration tests under a degraded network. Use this script to
# see how the SDK behaves with latency, jitter, and packet loss applied to
# the LiveKit connection. Tests run inside a Docker container on the perlink
# network so that tc/netem rules shape traffic between the test runner and
# LiveKit.
#
# If you just want to test the tc/netem infrastructure itself (echo servers,
# u32 filter classification, packet counts), use scripts/netem-perlink.sh
# instead.
#
# Usage:
#   scripts/netem-livekit.sh up [netem-args]  Start the stack (optional impairment args)
#   scripts/netem-livekit.sh reup <netem-args> Update impairment live (no reconnect)
#   scripts/netem-livekit.sh down            Stop the stack
#   scripts/netem-livekit.sh inspect         Show tc hierarchy inside the netem container
#   scripts/netem-livekit.sh digest          Show human-readable netem digest summary
#   scripts/netem-livekit.sh test [filter]   Run tests inside the test-runner container
#   scripts/netem-livekit.sh shell           Open a shell in the test-runner container
#   scripts/netem-livekit.sh build           Pre-build the test binary (no test run)
#
# The 'up' and 'reup' commands accept inline netem args (order-independent):
#   scripts/netem-livekit.sh up delay 200ms loss 5%
#   scripts/netem-livekit.sh up loss 5% delay 200ms          # same thing
#   scripts/netem-livekit.sh reup delay 50ms rate 1mbit
#
# Per-link overrides still use env vars:
#   NETEM_LINK_RUNNER_ARGS="delay 500ms loss 20%" scripts/netem-livekit.sh up
#
# Node IP (WebRTC ICE candidates):
#   The 'up' command defaults to --node-ip 127.0.0.1, which works for live
#   demos where the Foxglove app connects from the host browser. The 'test',
#   'build', and 'shell' commands automatically override to 10.99.0.2 so that
#   ICE candidates are reachable from the test-runner container on the perlink
#   network. You can also set LIVEKIT_NODE_IP explicitly:
#     LIVEKIT_NODE_IP=10.99.0.2 scripts/netem-livekit.sh up
#
# The 'test' command passes extra arguments to cargo test:
#   scripts/netem-livekit.sh test livekit_          # run all livekit_ tests
#   scripts/netem-livekit.sh test viewer_connects   # run a specific test

set -eu

REPO_ROOT="$(git rev-parse --show-toplevel)"
. "$REPO_ROOT/scripts/netem-lib.sh"

COMPOSE="docker compose \
  -f $REPO_ROOT/docker-compose.yaml \
  -f $REPO_ROOT/docker-compose.netem.yml \
  -f $REPO_ROOT/docker-compose.netem-livekit.yml"

usage() {
    echo "Usage: $0 <command>"
    echo ""
    echo "Commands:"
    echo "  up [netem-args]   Start the stack with optional impairment"
    echo "  reup <netem-args> Update impairment live (no restart, no reconnect)"
    echo "  down              Stop and remove the stack"
    echo "  inspect           Show tc qdiscs, classes, and filters inside the netem container"
    echo "  digest            Show human-readable netem digest summary"
    echo "  test [filter]     Run tests inside the test-runner container"
    echo "  shell             Open a shell in the test-runner container"
    echo "  build             Pre-build the test binary without running tests"
    echo ""
    echo "Examples:"
    echo "  $0 up                                  # start with default impairment"
    echo "  $0 up delay 200ms loss 5%              # start with custom impairment"
    echo "  $0 reup delay 300ms 50ms loss 10%      # change impairment live"
    echo "  $0 reup delay 50ms rate 1mbit          # add bandwidth cap"
    echo "  $0 reup delay 150ms 50ms loss 5%       # poor wifi"
    echo "  $0 reup default delay 80ms 20ms        # change only the default class"
    echo "  $0 digest                              # verify current settings"
    echo "  $0 test livekit_                       # run all livekit_ tests"
    echo "  $0 test viewer_connects                # run a specific test"
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
        echo ""
        echo "Stack is up. Run tests with: $0 test livekit_"
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
        shift
        filter="${1:-livekit_}"
        # Containerized tests need ICE candidates pointing to the perlink IP.
        export LIVEKIT_NODE_IP=10.99.0.2
        $COMPOSE up -d --wait
        echo "Running tests matching '$filter' inside test-runner container..."
        $COMPOSE exec test-runner \
            cargo test -p remote_access_tests -- --ignored "$filter"
        ;;

    shell)
        # Containerized tests need ICE candidates pointing to the perlink IP.
        export LIVEKIT_NODE_IP=10.99.0.2
        $COMPOSE up -d --wait
        $COMPOSE exec test-runner bash
        ;;

    build)
        # Containerized tests need ICE candidates pointing to the perlink IP.
        export LIVEKIT_NODE_IP=10.99.0.2
        $COMPOSE up -d --wait
        echo "Building test binary inside test-runner container..."
        $COMPOSE exec test-runner \
            cargo test -p remote_access_tests --no-run
        ;;

    *)
        usage >&2
        exit 1
        ;;
esac
