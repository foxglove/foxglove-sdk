#!/bin/sh
# Set up and manage a LiveKit + netem Docker stack for testing the SDK under
# degraded network conditions. The stack shapes traffic between the test runner
# and LiveKit using tc/netem rules.
#
# Most tests can be run directly with cargo once the stack is up:
#   scripts/netem-livekit.sh up
#   cargo test -p remote_access_tests -- --ignored livekit_
#
# The only tests that require script orchestration are the two-container
# per-link tests (`test perlink`), which coordinate gateway and viewer
# processes across separate Docker containers.
#
# Usage:
#   scripts/netem-livekit.sh up [netem-args]  Start the stack (optional impairment args)
#   scripts/netem-livekit.sh up perlink       Start two-container per-link stack
#   scripts/netem-livekit.sh reup <netem-args> Update impairment live (no reconnect)
#   scripts/netem-livekit.sh down             Stop the stack
#   scripts/netem-livekit.sh inspect          Show tc hierarchy inside the netem container
#   scripts/netem-livekit.sh digest           Show human-readable netem digest summary
#   scripts/netem-livekit.sh test perlink     Run per-link tests (gateway + viewer)
#   scripts/netem-livekit.sh shell            Open a shell in the test-runner container
#   scripts/netem-livekit.sh build            Pre-build the test binary (no test run)
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
#   demos where the Foxglove app connects from the host browser. The 'test
#   perlink', 'build', and 'shell' commands automatically override to
#   10.99.0.2 so that ICE candidates are reachable from containers on the
#   perlink network. You can also set LIVEKIT_NODE_IP explicitly:
#     LIVEKIT_NODE_IP=10.99.0.2 scripts/netem-livekit.sh up

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
    echo "  up perlink        Start two-container per-link stack (gateway + viewer)"
    echo "  reup <netem-args> Update impairment live (no restart, no reconnect)"
    echo "  down              Stop and remove the stack"
    echo "  inspect           Show tc qdiscs, classes, and filters inside the netem container"
    echo "  digest            Show human-readable netem digest summary"
    echo "  test perlink      Run per-link tests (gateway + viewer orchestration)"
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
    echo "  $0 up perlink                          # start per-link stack"
    echo "  $0 test perlink                        # run per-link tests"
    echo ""
    echo "Once the stack is up, run tests directly with cargo:"
    echo "  cargo test -p remote_access_tests -- --ignored livekit_"
    echo "  cargo test -p remote_access_tests -- --ignored netem_"
    echo ""
    echo "Netem args are order-independent keywords: delay, loss, rate, duplicate,"
    echo "corrupt, reorder. Sub-args within a keyword are positional:"
    echo "  delay TIME [JITTER [CORRELATION]]"
    echo "  loss PERCENT [CORRELATION]"
    echo "  rate BANDWIDTH"
}

# Export netem link env vars for per-link mode. These are picked up by the
# netem sidecar's NETEM_LINK_*_DST auto-discovery. Only exported in perlink
# paths so that single-container `digest` doesn't show phantom links.
export_perlink_netem_vars() {
    export NETEM_LINK_GATEWAY_DST="10.99.0.31"
    export NETEM_LINK_GATEWAY_ARGS="${NETEM_LINK_GATEWAY_ARGS:-delay 200ms 50ms loss 5%}"
    export NETEM_LINK_VIEWER_DST="10.99.0.40"
    export NETEM_LINK_VIEWER_ARGS="${NETEM_LINK_VIEWER_ARGS:-delay 10ms 2ms}"
}

# Generate and export a unique room name for perlink tests. Both containers
# read this from the PERLINK_ROOM_NAME env var instead of using file-based
# coordination to discover the room name.
export_perlink_room_name() {
    export PERLINK_ROOM_NAME="test-room-$(cat /proc/sys/kernel/random/uuid 2>/dev/null || uuidgen)"
}

case "${1:-}" in
    up)
        shift
        if [ "${1:-}" = "perlink" ]; then
            # Start the two-container per-link stack (gateway + viewer).
            export LIVEKIT_NODE_IP=10.99.0.2
            export_perlink_netem_vars
            export_perlink_room_name
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
            echo "Stack is up. Run tests with:"
            echo "  cargo test -p remote_access_tests -- --ignored livekit_"
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
        if [ "${1:-}" != "perlink" ]; then
            echo "Usage: $0 test perlink" >&2
            echo "" >&2
            echo "Only the two-container per-link tests require script orchestration." >&2
            echo "Run other tests directly with cargo after starting the stack:" >&2
            echo "  $0 up" >&2
            echo "  cargo test -p remote_access_tests -- --ignored <filter>" >&2
            exit 1
        fi
        # Two-container per-link test orchestration. Starts the stack if
        # not already running, then runs gateway and viewer tests.
        export LIVEKIT_NODE_IP=10.99.0.2
        export_perlink_netem_vars
        export_perlink_room_name
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
        # Kill background cargo processes on Ctrl-C so they don't linger
        # until their coordination timeouts expire.
        trap 'kill "$gateway_pid" "$viewer_pid" 2>/dev/null; wait "$gateway_pid" "$viewer_pid" 2>/dev/null' INT TERM
        $COMPOSE_PERLINK exec -T gateway-runner \
            cargo test -p remote_access_tests -- --ignored perlink_docker_gateway --nocapture &
        gateway_pid=$!
        $COMPOSE_PERLINK exec -T viewer-runner \
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
