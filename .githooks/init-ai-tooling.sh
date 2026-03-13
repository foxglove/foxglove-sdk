#!/bin/bash
# Clone or update the shared AI tooling repo, then run its init script.
SHARED_DIR=".ai-tooling"
REPO_URL="https://github.com/foxglove/ai-tooling.git"
if [ ! -d "$SHARED_DIR" ]; then
  git clone --depth=1 "$REPO_URL" "$SHARED_DIR" >&2 || { echo "Warning: failed to clone $REPO_URL" >&2; exit 0; }
else
  (cd "$SHARED_DIR" && git pull --ff-only) >&2 || { echo "Warning: failed to update $SHARED_DIR" >&2; exit 0; }
fi
bash "$SHARED_DIR/init.sh" >&2
