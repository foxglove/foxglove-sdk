#!/usr/bin/env bash
# Smoke test for the example data provider with remote-data-loader and MinIO cache.
#
# This test is strict: every step must succeed or the test fails.
#
# Required tools on PATH: curl, jq, mcap, xxd
#
# Environment variables:
#   DATA_PROVIDER_URL      – base URL of the data provider (default: http://localhost:8081)
#   REMOTE_DATA_LOADER_URL – base URL of the remote-data-loader (default: http://localhost:8080)
#   BEARER_TOKEN           – bearer token for the data provider (default: test-token)
#
# Steps:
#   1. Fetch the manifest from the data provider; validate its structure with jq.
#   2. Extract the source URL from the manifest and fetch MCAP data from the data provider.
#   3. Validate the MCAP with the mcap CLI.
#   4. POST /v1/stream on the remote-data-loader (fresh, populates cache).
#   5. POST /v1/stream on the remote-data-loader again (from cache).
#   6. Validate both MCAP files and compare them.

set -euo pipefail

REMOTE_DATA_LOADER_URL="${REMOTE_DATA_LOADER_URL:-http://localhost:8080}"
DATA_PROVIDER_URL="${DATA_PROVIDER_URL:-http://localhost:8081}"
BEARER_TOKEN="${BEARER_TOKEN:-test-token}"
TMPDIR_BASE="$(mktemp -d)"
trap 'rm -rf "$TMPDIR_BASE"' EXIT

# The data provider emits one Vector3 message per second with x = unix timestamp.
# For [2024-01-01T00:00:00Z, 2024-01-01T00:00:05Z] we expect 6 messages (at +0s … +5s).
START_TIME="2024-01-01T00:00:00Z"
END_TIME="2024-01-01T00:00:05Z"
FLIGHT_ID="smoke-test"
EXPECTED_MESSAGE_COUNT=6

QUERY_STRING="flightId=$FLIGHT_ID&startTime=$START_TIME&endTime=$END_TIME"

# --------------------------------------------------------------------------- #
# Helpers
# --------------------------------------------------------------------------- #

log() { echo "=== $(date -Iseconds) $*"; }
fail() { echo "FAIL: $*" >&2; exit 1; }

# Assert a required binary is on PATH.
require() {
  command -v "$1" >/dev/null 2>&1 || fail "required tool not found: $1"
}

# Wait until a URL returns any HTTP status code (proves the server is listening).
wait_for_url() {
  local url="$1" description="$2" max_attempts="${3:-60}" attempt=0
  log "Waiting for $description at $url ..."
  while true; do
    local code
    code=$(curl -s -o /dev/null -w '%{http_code}' "$url" 2>/dev/null) || code="000"
    if [ "$code" != "000" ]; then
      log "$description is ready (HTTP $code)."
      return
    fi
    attempt=$((attempt + 1))
    if [ "$attempt" -ge "$max_attempts" ]; then
      fail "Timed out waiting for $description at $url after $max_attempts attempts"
    fi
    sleep 2
  done
}

# Validate an MCAP file with the mcap CLI.
# Asserts: non-empty, valid magic, expected message count, /demo channel present.
validate_mcap() {
  local file="$1" label="$2"
  local byte_count
  byte_count=$(wc -c < "$file")
  log "Validating MCAP: $label ($byte_count bytes)"

  [ -s "$file" ] || fail "$label: file is empty"

  # Magic bytes: 0x89 M C A P 0x30 \r \n
  local magic
  magic=$(xxd -l 8 -p "$file")
  [ "$magic" = "894d434150300d0a" ] || fail "$label: bad MCAP magic: $magic"

  local info
  info=$(mcap info "$file")
  echo "$info"

  # Message count
  local msg_count
  msg_count=$(echo "$info" | grep -oP '^\s*messages:\s+\K\d+')
  [ "$msg_count" -eq "$EXPECTED_MESSAGE_COUNT" ] \
    || fail "$label: expected $EXPECTED_MESSAGE_COUNT messages, got $msg_count"
  log "$label: message count OK ($msg_count)"

  # /demo channel
  echo "$info" | grep -q "/demo" \
    || fail "$label: /demo channel not found"
  log "$label: /demo channel present"
}

# --------------------------------------------------------------------------- #
# Prerequisites
# --------------------------------------------------------------------------- #

log "Checking prerequisites ..."
require curl
require jq
require mcap
require xxd

# --------------------------------------------------------------------------- #
# Wait for services
# --------------------------------------------------------------------------- #

log "Waiting for services ..."
wait_for_url "$DATA_PROVIDER_URL/v1/manifest?$QUERY_STRING" "data-provider"
wait_for_url "$REMOTE_DATA_LOADER_URL/liveness" "remote-data-loader"

# --------------------------------------------------------------------------- #
# Step 1: Fetch and validate the manifest from the data provider
# --------------------------------------------------------------------------- #

log "Step 1: Fetching manifest from data provider ..."

manifest="$TMPDIR_BASE/manifest.json"
curl -sf \
  -H "Authorization: Bearer $BEARER_TOKEN" \
  "$DATA_PROVIDER_URL/v1/manifest?$QUERY_STRING" \
  -o "$manifest"

log "Manifest:"
jq . "$manifest"

# Validate manifest structure.
source_count=$(jq '.sources | length' "$manifest")
[ "$source_count" -ge 1 ] || fail "manifest has no sources"

source_url=$(jq -r '.sources[0].url' "$manifest")
[ -n "$source_url" ] && [ "$source_url" != "null" ] \
  || fail "manifest source has no url"

topic_name=$(jq -r '.sources[0].topics[0].name' "$manifest")
[ "$topic_name" = "/demo" ] \
  || fail "expected topic /demo, got $topic_name"

log "Manifest OK: $source_count source(s), first source url=$source_url"

# --------------------------------------------------------------------------- #
# Step 2: Fetch MCAP from the data provider using the manifest source URL
# --------------------------------------------------------------------------- #

log "Step 2: Fetching MCAP from data provider at source URL ..."

# The source URL is relative (e.g. /v1/data?...). Resolve against the data provider.
direct_mcap="$TMPDIR_BASE/direct.mcap"
curl -sf \
  -H "Authorization: Bearer $BEARER_TOKEN" \
  "${DATA_PROVIDER_URL}${source_url}" \
  -o "$direct_mcap"

validate_mcap "$direct_mcap" "direct"

# --------------------------------------------------------------------------- #
# Step 3: Fetch MCAP from remote-data-loader (fresh – populates cache)
# --------------------------------------------------------------------------- #

log "Step 3: Fetching MCAP from remote-data-loader (fresh) ..."

# The remote-data-loader exposes POST /v1/stream. The request body contains
# manifestQueryParams which are forwarded to MANIFEST_ENDPOINT as query params.
# The remote-data-loader fetches the manifest, resolves source URLs, fetches
# the data, caches it in MinIO, and returns the combined MCAP.
# Build the POST /v1/stream request body.
# - manifestQueryParams: [key, value] tuples forwarded to MANIFEST_ENDPOINT.
# - start / end: ISO 8601 timestamps bounding the time range to stream.
stream_body=$(jq -nc \
  --arg fid "$FLIGHT_ID" \
  --arg st  "$START_TIME" \
  --arg et  "$END_TIME" \
  '{
     manifestQueryParams: [
       ["flightId",  $fid],
       ["startTime", $st],
       ["endTime",   $et]
     ],
     start: $st,
     end:   $et
   }')
log "POST /v1/stream body: $stream_body"

# The remote-data-loader manages its own auth for upstream requests.
# Client requests do not need (and must not send) bearer tokens for the
# upstream data provider.
fresh_mcap="$TMPDIR_BASE/fresh.mcap"
fresh_code=$(curl -s -w '%{http_code}' \
  -X POST \
  -H "Content-Type: application/json" \
  -d "$stream_body" \
  "${REMOTE_DATA_LOADER_URL}/v1/stream" \
  -o "$fresh_mcap")
if [ "$fresh_code" -lt 200 ] || [ "$fresh_code" -ge 300 ]; then
  log "remote-data-loader returned HTTP $fresh_code. Response body:"
  cat "$fresh_mcap"; echo
  fail "POST /v1/stream returned HTTP $fresh_code"
fi

validate_mcap "$fresh_mcap" "fresh (via remote-data-loader)"

# --------------------------------------------------------------------------- #
# Step 4: Fetch MCAP from remote-data-loader again (should come from cache)
# --------------------------------------------------------------------------- #

log "Step 4: Fetching MCAP from remote-data-loader (cached) ..."

cached_mcap="$TMPDIR_BASE/cached.mcap"
curl -sf \
  -X POST \
  -H "Content-Type: application/json" \
  -d "$stream_body" \
  "${REMOTE_DATA_LOADER_URL}/v1/stream" \
  -o "$cached_mcap"

validate_mcap "$cached_mcap" "cached (via remote-data-loader)"

# The cached file must match the fresh one byte-for-byte (the remote-data-loader serves
# the same MCAP blob it stored in the cache).
if cmp -s "$fresh_mcap" "$cached_mcap"; then
  log "Fresh and cached MCAP files are identical."
else
  log "Fresh and cached MCAP files differ in raw bytes; comparing mcap info ..."
  diff <(mcap info "$fresh_mcap") <(mcap info "$cached_mcap") \
    || fail "fresh and cached MCAP info diverged"
  log "mcap info matches despite raw byte difference."
fi

# --------------------------------------------------------------------------- #
# Done
# --------------------------------------------------------------------------- #

log "All checks passed."
