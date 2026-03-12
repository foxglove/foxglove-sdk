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
#   scripts/netem-livekit.sh up perlink      Start two-container per-link stack (gateway + viewer)
#   scripts/netem-livekit.sh test perlink    Run per-link tests (gateway + viewer)
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
    echo "Per-link mode (gateway + viewer in separate containers):"
    echo "  up perlink        Start the two-container per-link stack"
    echo "  test perlink      Run per-link tests (starts stack if needed)"
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
    echo "  $0 up perlink                          # start per-link stack"
    echo "  $0 test perlink                        # run per-link tests"
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
        if [ "${1:-}" = "perlink" ]; then
            # Start the two-container per-link stack (gateway + viewer).
            export LIVEKIT_NODE_IP=10.99.0.2
            COMPOSE_PERLINK="$COMPOSE --profile perlink"
            $COMPOSE_PERLINK up -d --wait
            # Pre-build the test binary. The viewer-runner shares the same
            # target volume so it will reuse the build.
            echo "Building test binary in gateway-runner..."
            $COMPOSE_PERLINK exec gateway-runner \
                cargo test -p remote_access_tests --no-run
            echo ""
            echo "Per-link stack is up. Run tests with: $0 test perlink"
        else
            if [ $# -gt 0 ]; then
                export NETEM_ARGS="$*"
            fi
            $COMPOSE up -d --wait
            echo ""
            echo "Stack is up. Run tests with: $0 test livekit_"
        fi
        ;;

    reup)
        shift
        $COMPOSE exec netem /bin/sh /netem-impair.sh "$@"
        ;;

    down)
        # Include --profile perlink so gateway-runner and viewer-runner are
        # also stopped. This is safe even when they aren't running.
        $COMPOSE --profile perlink down
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
        if [ "$filter" = "perlink" ]; then
            # Two-container per-link test orchestration. Starts the stack if
            # not already running, then runs gateway and viewer tests.
            COMPOSE_PERLINK="$COMPOSE --profile perlink"
            $COMPOSE_PERLINK up -d --wait
            # Clean coordination dir from any previous run.
            $COMPOSE_PERLINK exec gateway-runner sh -c 'rm -f /coordination/*'
            # Build if needed (no-op if `up perlink` already built).
            $COMPOSE_PERLINK exec gateway-runner \
                cargo test -p remote_access_tests --no-run
            # Run both tests in foreground. The gateway runs in a background
            # shell job so we can wait on both and detect failures from either.
            echo "Starting gateway and viewer tests..."
            $COMPOSE_PERLINK exec gateway-runner \
                cargo test -p remote_access_tests -- --ignored perlink_docker_gateway --nocapture &
            gateway_pid=$!
            $COMPOSE_PERLINK exec viewer-runner \
                cargo test -p remote_access_tests -- --ignored perlink_docker_viewer --nocapture &
            viewer_pid=$!
            # Wait for both; capture exit statuses individually.
            gateway_rc=0; wait "$gateway_pid" || gateway_rc=$?
            viewer_rc=0; wait "$viewer_pid" || viewer_rc=$?
            if [ "$gateway_rc" -ne 0 ] || [ "$viewer_rc" -ne 0 ]; then
                echo "Per-link test FAILED (gateway=$gateway_rc, viewer=$viewer_rc)." >&2
                exit 1
            fi
            echo "Per-link test complete."
        else
            $COMPOSE up -d --wait
            echo "Running tests matching '$filter' inside test-runner container..."
            $COMPOSE exec test-runner \
                cargo test -p remote_access_tests -- --ignored "$filter"
        fi
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
