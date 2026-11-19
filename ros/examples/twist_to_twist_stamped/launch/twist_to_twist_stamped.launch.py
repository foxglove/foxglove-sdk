from launch import LaunchDescription
from launch.actions import DeclareLaunchArgument
from launch.substitutions import LaunchConfiguration
from launch_ros.actions import Node
from launch_ros.parameter_descriptions import ParameterValue


def generate_launch_description():
    repeat_hz = LaunchConfiguration("repeat_hz")
    command_timeout_s = LaunchConfiguration("command_timeout_s")
    return LaunchDescription(
        [
            DeclareLaunchArgument("input_topic", default_value="/teleop/cmd_vel"),
            DeclareLaunchArgument("output_topic", default_value="/cmd_vel"),
            DeclareLaunchArgument("frame_id", default_value="base_link"),
            DeclareLaunchArgument("repeat_hz", default_value="30.0"),
            DeclareLaunchArgument("command_timeout_s", default_value="2.0"),
            Node(
                package="twist_to_twist_stamped",
                executable="twist_to_twist_stamped",
                name="twist_to_twist_stamped",
                output="screen",
                parameters=[
                    {
                        "input_topic": LaunchConfiguration("input_topic"),
                        "output_topic": LaunchConfiguration("output_topic"),
                        "frame_id": LaunchConfiguration("frame_id"),
                        "repeat_hz": ParameterValue(repeat_hz, value_type=float),
                        "command_timeout_s": ParameterValue(
                            command_timeout_s, value_type=float
                        ),
                    }
                ],
            ),
        ]
    )
