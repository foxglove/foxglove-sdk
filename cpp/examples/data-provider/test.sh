#!/usr/bin/env bash
#
# Run the data provider conformance tests against the C++ example.
#
# Usage:
#   cpp/examples/data-provider/test.sh [path/to/example_data_provider]
#
# If no path is given, defaults to cpp/build/example_data_provider.
# The conformance test crate is built and run automatically via cargo.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

BINARY="${1:-$REPO_ROOT/cpp/build/example_data_provider}"
HOST="127.0.0.1"
PORT="8081"
ADDR="$HOST:$PORT"

if [[ ! -x "$BINARY" ]]; then
  echo "error: binary not found or not executable: $BINARY" >&2
  echo "Build it first: cd cpp/build && cmake --build . --target example_data_provider" >&2
  exit 1
fi

# Check if a port is accepting connections using bash builtins.
check_port() {
  (echo >/dev/tcp/"$HOST"/"$PORT") 2>/dev/null
}

# Ensure no stale server is running on our port.
if check_port; then
  echo "error: something is already listening on $ADDR" >&2
  exit 1
fi

# Start the C++ example server.
"$BINARY" &
SERVER_PID=$!
trap 'kill $SERVER_PID 2>/dev/null; wait $SERVER_PID 2>/dev/null' EXIT

# Wait for the server to accept connections.
for _ in $(seq 1 100); do
  if check_port; then
    break
  fi
  if ! kill -0 $SERVER_PID 2>/dev/null; then
    echo "error: server process exited unexpectedly" >&2
    exit 1
  fi
  sleep 0.05
done

if ! check_port; then
  echo "error: server did not become ready within 5 s" >&2
  exit 1
fi

# Run the conformance tests.
cd "$REPO_ROOT"
DATA_PROVIDER_ADDR="$ADDR" cargo run -p data_provider_conformance
RESULT=$?

# Clean up the server before exiting (the trap also does this, but being
# explicit avoids inheriting the kill signal's exit code).
kill $SERVER_PID 2>/dev/null
wait $SERVER_PID 2>/dev/null || true
trap - EXIT
exit $RESULT
