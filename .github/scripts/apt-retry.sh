#!/usr/bin/env bash
# Runs `apt-get update` followed by `apt-get install` with bounded retries, to
# survive transient Ubuntu mirror failures (Failed to fetch, hash-sum mismatch,
# 5xx) that would otherwise fail an entire CI job. All arguments are forwarded
# verbatim to `apt-get install -y`, so callers pass any flags (e.g.
# --no-install-recommends) and the package list. Exits non-zero after 3 failed
# attempts so a genuine breakage is not masked.
set -euo pipefail

for attempt in 1 2 3; do
  if sudo apt-get update && sudo apt-get install -y "$@"; then
    exit 0
  fi
  if [ "$attempt" -eq 3 ]; then
    echo "::error::apt-get failed after 3 attempts"
    exit 1
  fi
  echo "::warning::apt-get failed (attempt $attempt/3); retrying in 15s"
  sleep 15
done
