# Twist to TwistStamped

Foxglove's Teleop panel publishes `geometry_msgs/msg/Twist`. The Dexmate chassis
subscriber (`dexcontrol-rosbridge` `scripts/subscribe_chassis_vel.py`) listens
on `/cmd_vel` for `geometry_msgs/msg/TwistStamped` and zeros the base when no
message arrives for 100 ms. Teleop's default rate is 1 Hz, and ROS 2 will not
deliver a `Twist` publisher to a `TwistStamped` subscriber on the same topic.

This node subscribes to the Teleop `Twist`, republishes it as `TwistStamped`,
and repeats the last non-zero command at 30 Hz. A zero Twist (Teleop "Stop on
release") is forwarded once. If Teleop goes silent for `command_timeout_s`
(default 2 s, longer than the panel's 1 Hz period), the node publishes one
zero command and stops.

`input_topic` and `output_topic` must be different names.

## Run

Use the same ROS 2 Jazzy environment, `RMW_IMPLEMENTATION`, and `ROS_DOMAIN_ID`
as `foxglove_bridge` and `subscribe_chassis_vel.py`.

```bash
# From ros/examples, with that environment sourced. No colcon install required.
python3 -m twist_to_twist_stamped --ros-args \
  -p input_topic:=/teleop/cmd_vel \
  -p output_topic:=/cmd_vel
```

Or, from a ROS workspace:

```bash
cd ros
colcon build --paths examples/twist_to_twist_stamped
source install/setup.bash
ros2 launch twist_to_twist_stamped twist_to_twist_stamped.launch.py
```

`ros/examples/COLCON_IGNORE` keeps this package out of the foxglove_bridge
build. `--paths` still builds it.

Point the Teleop panel at `/teleop/cmd_vel` and enable **Stop on release**.
Leave the chassis script on its default `/cmd_vel`. If `client_topic_whitelist`
is narrowed, include `/teleop/cmd_vel` — that is the topic the bridge publishes.

The default 2 s hold matches Teleop at 1 Hz, so a held button keeps the base
moving, and a dropped remote-access link stops it after about 2 s. For a
shorter coast, set the panel publish rate to 10 Hz and launch with
`command_timeout_s:=0.5`.

`repeat_hz` must stay above 10 Hz. The chassis watchdog is 100 ms; a slower
repeat lets the base stop between setpoints.

Start `subscribe_chassis_vel.py` as well. Pass `--omni` on that script for a
steerable base. This relay forwards `linear.y` unchanged and does not pick the
chassis mode.
