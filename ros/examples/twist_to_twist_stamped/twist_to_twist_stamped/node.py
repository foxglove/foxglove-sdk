"""ROS 2 node that republishes Twist as TwistStamped for the Dexmate chassis."""

from __future__ import annotations

import math
import threading

import rclpy
from geometry_msgs.msg import Twist, TwistStamped
from rclpy.node import Node
from rclpy.qos import DurabilityPolicy, HistoryPolicy, QoSProfile, ReliabilityPolicy

from twist_to_twist_stamped.hold import (
    CHASSIS_COMMAND_WATCHDOG_S,
    DEFAULT_COMMAND_TIMEOUT_S,
    DEFAULT_REPEAT_HZ,
    CommandHold,
    TwistSample,
)

# Match subscribe_chassis_vel.py, which subscribes with depth 10 (reliable, volatile).
# foxglove_bridge's client publisher uses the system-default QoS, which is reliable
# and volatile on Fast DDS and Cyclone DDS, so the two sides connect.
COMMAND_QOS = QoSProfile(
    history=HistoryPolicy.KEEP_LAST,
    depth=10,
    reliability=ReliabilityPolicy.RELIABLE,
    durability=DurabilityPolicy.VOLATILE,
)


class TwistToTwistStamped(Node):
    """Subscribe to Teleop ``Twist`` messages and publish chassis ``TwistStamped``.

    The input and output topics must be different. Publishing both types on one
    name makes ROS 2 drop the command.
    """

    def __init__(self) -> None:
        super().__init__("twist_to_twist_stamped")
        self._input_topic = self._declare_string("input_topic", "/teleop/cmd_vel")
        self._output_topic = self._declare_string("output_topic", "/cmd_vel")
        self._frame_id = self._declare_string("frame_id", "base_link")
        repeat_hz = self._declare_double("repeat_hz", DEFAULT_REPEAT_HZ)
        command_timeout_s = self._declare_double(
            "command_timeout_s", DEFAULT_COMMAND_TIMEOUT_S
        )

        if self._input_topic == self._output_topic:
            raise ValueError(
                "input_topic and output_topic must differ "
                f"(both are {self._input_topic!r}). Foxglove Teleop publishes "
                "geometry_msgs/msg/Twist and the chassis subscribes to "
                "geometry_msgs/msg/TwistStamped; one topic cannot carry both."
            )
        if not math.isfinite(repeat_hz) or repeat_hz <= 0.0:
            raise ValueError("repeat_hz must be a positive finite number")

        self._lock = threading.Lock()
        self._announced_motion = False
        period_s = 1.0 / repeat_hz
        if period_s >= CHASSIS_COMMAND_WATCHDOG_S:
            self.get_logger().warning(
                "repeat_hz=%.1f publishes every %.0f ms, which is not faster than "
                "the dexcontrol chassis watchdog (%.0f ms). The base will stop "
                "between repeats.",
                repeat_hz,
                period_s * 1000.0,
                CHASSIS_COMMAND_WATCHDOG_S * 1000.0,
            )
        # Teleop's default publish rate is 1 Hz. A shorter hold treats the gap
        # between those messages as a lost link and stops the base.
        if command_timeout_s < 1.0:
            self.get_logger().warning(
                "command_timeout_s=%.2f is shorter than the Teleop panel's default "
                "1 Hz publish period. Raise the panel publish rate above %.0f Hz, "
                "or the relay will stop the base between Teleop messages.",
                command_timeout_s,
                1.0 / command_timeout_s,
            )

        self._hold = CommandHold(command_timeout_s)
        self._publisher = self.create_publisher(
            TwistStamped, self._output_topic, COMMAND_QOS
        )
        self._subscription = self.create_subscription(
            Twist, self._input_topic, self._on_twist, COMMAND_QOS
        )
        self._timer = self.create_timer(period_s, self._on_timer)

        self.get_logger().info(
            "Converting %s (geometry_msgs/msg/Twist) -> %s "
            "(geometry_msgs/msg/TwistStamped), repeating at %.0f Hz for %.2f s",
            self._input_topic,
            self._output_topic,
            repeat_hz,
            command_timeout_s,
        )

    def _declare_string(self, name: str, default: str) -> str:
        self.declare_parameter(name, default)
        return self.get_parameter(name).get_parameter_value().string_value

    def _declare_double(self, name: str, default: float) -> float:
        self.declare_parameter(name, default)
        return self.get_parameter(name).get_parameter_value().double_value

    def _now_s(self) -> float:
        return self.get_clock().now().nanoseconds / 1e9

    def _on_twist(self, msg: Twist) -> None:
        sample = TwistSample(
            linear_x=msg.linear.x,
            linear_y=msg.linear.y,
            linear_z=msg.linear.z,
            angular_x=msg.angular.x,
            angular_y=msg.angular.y,
            angular_z=msg.angular.z,
        )
        with self._lock:
            was_moving = self._hold.is_latched
            to_publish = self._hold.on_twist(sample, self._now_s())
        if was_moving and sample.is_stopped():
            self.get_logger().info(
                "Teleop stop received; publishing a zero TwistStamped"
            )
        elif not sample.is_stopped() and not self._announced_motion:
            self._announced_motion = True
            self.get_logger().info("Repeating Teleop command on %s", self._output_topic)
        self._publish(to_publish)

    def _on_timer(self) -> None:
        with self._lock:
            to_publish = self._hold.poll(self._now_s())
            timed_out = to_publish is not None and to_publish.is_stopped()
        if to_publish is None:
            return
        if timed_out:
            self.get_logger().info(
                "No Twist for %.2f s; publishing a zero TwistStamped",
                self._hold.command_timeout_s,
            )
        self._publish(to_publish)

    def _publish(self, sample: TwistSample) -> None:
        msg = TwistStamped()
        msg.header.stamp = self.get_clock().now().to_msg()
        msg.header.frame_id = self._frame_id
        msg.twist.linear.x = float(sample.linear_x)
        msg.twist.linear.y = float(sample.linear_y)
        msg.twist.linear.z = float(sample.linear_z)
        msg.twist.angular.x = float(sample.angular_x)
        msg.twist.angular.y = float(sample.angular_y)
        msg.twist.angular.z = float(sample.angular_z)
        self._publisher.publish(msg)


def main(args: list[str] | None = None) -> None:
    rclpy.init(args=args)
    node = TwistToTwistStamped()
    try:
        rclpy.spin(node)
    except KeyboardInterrupt:
        pass
    finally:
        node.destroy_node()
        if rclpy.ok():
            rclpy.shutdown()
