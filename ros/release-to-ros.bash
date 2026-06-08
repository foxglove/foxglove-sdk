#!/bin/bash
# Release the foxglove_bridge and foxglove_msgs packages to a ROS distro via bloom.
#
# Usage: ros/release-to-ros.bash --tag <tag> --distro <ros-distro> [--dry-run]
#
# The specified release tag must exist locally and at origin.
#
# This script depends on a github auth token. You can provide one through the
# `GITHUB_TOKEN` environment variable. If no token is provided, this script will
# attempt to obtain one by invoking `gh auth token`.
#
# This script requires the existence of a bloom config file, which must contain
# a classic GitHub PAT. This token is used by bloom-release to create PRs
# against the upstream ros/rosdistro repo. The path defaults to
# ~/.config/bloom; override by setting the BLOOM_CONFIG environment variable.
# The file should contain the following content:
#
#   {
#     "github_user": "your-github-username",
#     "oauth_token": "ghp_..."
#   }
#
# For more information on how to set up the PAT, see:
# https://docs.ros.org/en/rolling/How-To-Guides/Releasing/First-Time-Release.html

set -euo pipefail

TAG=""
DISTRO=""
DRY_RUN=false

usage() {
  echo "usage: $0 --tag <tag> --distro <ros-distro> [--dry-run]" >&2
}

while [[ $# -gt 0 ]]; do
  case "$1" in
  --tag)
    [[ $# -ge 2 ]] || {
      usage
      exit 1
    }
    TAG=$2
    shift 2
    ;;
  --distro)
    [[ $# -ge 2 ]] || {
      usage
      exit 1
    }
    DISTRO=$2
    shift 2
    ;;
  --dry-run)
    DRY_RUN=true
    shift
    ;;
  -h | --help)
    usage
    exit 0
    ;;
  *)
    echo "unknown argument: $1" >&2
    usage
    exit 1
    ;;
  esac
done

if [[ -z $TAG || -z $DISTRO ]]; then
  usage
  exit 1
fi

REMOTE=__bloom-release-tmp
REMOTE_URL=https://github.com/ros2-gbp/foxglove_bridge-release.git

REPO_ROOT=$(git rev-parse --show-toplevel)
BLOOM_CONFIG=${BLOOM_CONFIG:-$HOME/.config/bloom}

if [[ ! -f $BLOOM_CONFIG ]]; then
  echo "error: bloom config not found at $BLOOM_CONFIG; see prerequisites in $0" >&2
  exit 1
fi
if [[ -z ${GITHUB_TOKEN:-} ]]; then
  if ! command -v gh >/dev/null 2>&1; then
    echo "error: GITHUB_TOKEN unset and gh CLI not found; install gh or export GITHUB_TOKEN" >&2
    exit 1
  fi
  if ! GITHUB_TOKEN=$(gh auth token 2>/dev/null); then
    echo "error: 'gh auth token' failed; run 'gh auth login' or export GITHUB_TOKEN" >&2
    exit 1
  fi
fi
export GITHUB_TOKEN

GIT_USER_NAME=$(git config user.name 2>/dev/null || true)
GIT_USER_EMAIL=$(git config user.email 2>/dev/null || true)
if [[ -z $GIT_USER_NAME || -z $GIT_USER_EMAIL ]]; then
  echo "error: git user.name and user.email must be configured" >&2
  echo "  git config --global user.name 'Your Name'" >&2
  echo "  git config --global user.email 'you@example.com'" >&2
  exit 1
fi
export GIT_USER_NAME GIT_USER_EMAIL
if ! git rev-parse --verify "refs/tags/$TAG" >/dev/null 2>&1; then
  echo "error: tag '$TAG' not found locally" >&2
  exit 1
fi
if ! git ls-remote --exit-code origin "refs/tags/$TAG" >/dev/null 2>&1; then
  echo "error: tag '$TAG' not found on origin; push it before releasing" >&2
  exit 1
fi

# In dry-run mode, neither LFS nor bloom should mutate remote state.
LFS_PUSH_CMD=(git lfs push)
BLOOM_PRETEND_FLAG=""
if [[ $DRY_RUN == true ]]; then
  LFS_PUSH_CMD+=(--dry-run)
  BLOOM_PRETEND_FLAG="--pretend"
  echo ">>> DRY-RUN: no remote state will be modified"
fi
LFS_PUSH_CMD+=("$REMOTE" "$TAG")

cleanup() {
  git remote remove "$REMOTE" 2>/dev/null || true
}
trap cleanup EXIT

git remote remove "$REMOTE" 2>/dev/null || true
git remote add "$REMOTE" "$REMOTE_URL"

# Push only the LFS objects reachable from $TAG, so the source mirror bloom
# pushes resolves without leaking objects from unrelated local refs.
echo ">>> Syncing LFS objects for $TAG to $REMOTE"
git lfs fetch origin "$TAG"
"${LFS_PUSH_CMD[@]}"

echo ">>> Running bloom-release foxglove-sdk --ros-distro $DISTRO${BLOOM_PRETEND_FLAG:+ $BLOOM_PRETEND_FLAG}"
make -C "$REPO_ROOT/ros" docker-bloom-release \
  BLOOM_RELEASE_DISTRO="$DISTRO" \
  BLOOM_PRETEND_FLAG="$BLOOM_PRETEND_FLAG" \
  BLOOM_CONFIG="$BLOOM_CONFIG"
