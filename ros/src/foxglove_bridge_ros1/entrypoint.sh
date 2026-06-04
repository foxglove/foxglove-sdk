#!/bin/bash
set -e
source /opt/ros/noetic/setup.bash
source /opt/foxglove/setup.bash --extend
# The install space's setup file extends CMAKE_PREFIX_PATH / LD_LIBRARY_PATH /
# PATH, but ROS_PACKAGE_PATH is managed by a ros_environment profile hook that
# only ships with the noetic install space and already ran above. Extend it
# manually so rospack/rosrun can find the bridge package.
export ROS_PACKAGE_PATH=/opt/foxglove/share:${ROS_PACKAGE_PATH}
# Line-buffer stdout: the rosconsole print backend writes via printf, which is
# block-buffered when stdout is a pipe (e.g. `docker logs`).
exec stdbuf -oL -eL "$@"
