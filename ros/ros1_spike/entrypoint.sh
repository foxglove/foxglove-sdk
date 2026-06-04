#!/bin/bash
# Combined-spike entrypoint: start a roscore and a std_msgs/String talker
# in-container, then run the spike (roscpp node + RemoteAccessGateway).
set -e

source /opt/ros/noetic/setup.bash

roscore &
until rostopic list >/dev/null 2>&1; do sleep 0.5; done
echo "[entrypoint] roscore is up"

rostopic pub /chatter std_msgs/String "data: 'hello from noetic-on-jammy'" -r 5 &

exec /spike/build/ros1_spike
