#!/bin/bash
SHARED_DIR=".ai-tooling"
REPO_URL="https://github.com/foxglove/ai-tooling.git"
if [ ! -d "$SHARED_DIR" ]; then
  git clone --depth=1 "$REPO_URL" "$SHARED_DIR" >&2
  bash "$SHARED_DIR/init.sh" >&2
else
  (cd "$SHARED_DIR" && git pull --ff-only) >&2
  bash "$SHARED_DIR/init.sh" >&2
fi
