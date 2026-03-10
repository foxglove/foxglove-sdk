#!/bin/sh
# Manage the per-link netem Docker Compose stack for local development and
# testing. Wraps the three-file compose overlay and cargo test invocations.
#
# Usage:
#   scripts/netem-perlink.sh up            Start the per-link stack
#   scripts/netem-perlink.sh down          Stop the per-link stack
#   scripts/netem-perlink.sh inspect       Show tc hierarchy inside the netem container
#   scripts/netem-perlink.sh test          Run all perlink integration tests
#   scripts/netem-perlink.sh test infra    Run infrastructure tests only
#   scripts/netem-perlink.sh test product  Run product tests only
#
# The 'up' command accepts optional env var overrides for per-link impairment:
#   NETEM_LINK_A_ARGS="delay 500ms" scripts/netem-perlink.sh up

set -eu

REPO_ROOT="$(git rev-parse --show-toplevel)"

COMPOSE="docker compose \
  -f $REPO_ROOT/docker-compose.yaml \
  -f $REPO_ROOT/docker-compose.netem.yml \
  -f $REPO_ROOT/docker-compose.netem-perlink.yml"

# Find the running netem sidecar container ID. Matches containers whose name
# ends with "-netem-<N>" (the compose default naming convention).
netem_container_id() {
    id=$(docker ps -q --filter "name=-netem-[0-9]+$" | head -1)
    if [ -z "$id" ]; then
        echo "ERROR: no running netem container found — is the per-link stack running?" >&2
        echo "  Try: $0 up" >&2
        exit 1
    fi
    echo "$id"
}

usage() {
    echo "Usage: $0 <command>"
    echo ""
    echo "Commands:"
    echo "  up            Start the per-link netem stack (docker compose up --wait)"
    echo "  down          Stop and remove the per-link netem stack"
    echo "  inspect       Show tc qdiscs, classes, and filters from inside the netem container"
    echo "  test          Run all perlink integration tests"
    echo "  test infra    Run infrastructure tests only (hierarchy, latency, loss, default class)"
    echo "  test product  Run product tests only (viewer connect, burst delivery)"
}

case "${1:-}" in
    up)
        $COMPOSE up -d --wait
        ;;

    down)
        $COMPOSE down
        ;;

    inspect)
        cid=$(netem_container_id)
        echo "=== netem container: $cid ==="
        for resource in qdisc class filter; do
            echo ""
            echo "--- tc $resource ---"
            docker exec "$cid" tc -s "$resource" show 2>/dev/null || true
        done
        ;;

    test)
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
