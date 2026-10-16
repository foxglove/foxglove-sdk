#!/bin/bash
# Verify that every version declaration for the ROS packages agrees.
#
# Usage: ros/check-versions.bash [--tag <ros-vX.Y.Z>]
#
# Checks foxglove_bridge's CMakeLists.txt project version, both package.xml
# versions, and the latest heading in both CHANGELOG.rst files. With --tag,
# also checks that the tag names the same version.

set -euo pipefail

TAG=""
while [[ $# -gt 0 ]]; do
  case "$1" in
  --tag)
    [[ $# -ge 2 ]] || {
      echo "usage: $0 [--tag <ros-vX.Y.Z>]" >&2
      exit 1
    }
    TAG=$2
    shift 2
    ;;
  *)
    echo "usage: $0 [--tag <ros-vX.Y.Z>]" >&2
    exit 1
    ;;
  esac
done

SRC=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/src

declare -A versions
versions["foxglove_bridge/CMakeLists.txt"]=$(
  sed -nE 's/^project\(foxglove_bridge .*VERSION ([0-9.]+)\).*/\1/p' "$SRC/foxglove_bridge/CMakeLists.txt"
)
for pkg in foxglove_bridge foxglove_msgs; do
  versions["$pkg/package.xml"]=$(
    sed -nE 's|^\s*<version>([^<]+)</version>.*|\1|p' "$SRC/$pkg/package.xml" | head -n1
  )
  versions["$pkg/CHANGELOG.rst"]=$(
    sed -nE 's/^([0-9]+\.[0-9]+\.[0-9]+) \(.*/\1/p' "$SRC/$pkg/CHANGELOG.rst" | head -n1
  )
done
if [[ -n $TAG ]]; then
  versions["tag $TAG"]=${TAG#ros-v}
fi

expected=${versions["foxglove_bridge/package.xml"]}
status=0
for source in "${!versions[@]}"; do
  actual=${versions[$source]}
  if [[ -z $actual ]]; then
    echo "error: no version found in $source" >&2
    status=1
  elif [[ $actual != "$expected" ]]; then
    echo "error: $source has version $actual, expected $expected" >&2
    status=1
  fi
done
if [[ $status -eq 0 ]]; then
  echo "ROS package versions agree: $expected"
fi
exit $status
