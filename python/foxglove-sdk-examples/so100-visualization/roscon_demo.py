import argparse
import datetime
import logging
import math
import time

import foxglove
from foxglove.channels import RawImageChannel
from foxglove.schemas import (
    FrameTransform,
    FrameTransforms,
    Quaternion,
    RawImage,
    Vector3,
)
from lerobot.cameras import ColorMode, Cv2Rotation
from lerobot.cameras.opencv import OpenCVCamera, OpenCVCameraConfig

# Import SO101 leader and follower
from lerobot.teleoperators.so101_leader import SO101LeaderConfig, SO101Leader
from lerobot.robots.so101_follower import SO101FollowerConfig, SO101Follower
from scipy.spatial.transform import Rotation as R
from yourdfpy import URDF

WORLD_FRAME_ID = "world"
BASE_FRAME_ID = "base_link"
RATE_HZ = 30.0

# URDF file for SO101
SO101_URDF_FILE = "SO101/so101_new_calib.urdf"


def parse_args():
    parser = argparse.ArgumentParser(
        description="SO-101 robot arm visualization with Foxglove - ROSCon Demo",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )

    # Leader robot configuration
    leader_group = parser.add_argument_group("leader", "Leader robot configuration")
    leader_group.add_argument(
        "--leader.port",
        type=str,
        required=True,
        dest="leader_port",
        help="USB port to connect to the leader SO-101 arm (e.g., /dev/ttyUSB0)",
    )
    leader_group.add_argument(
        "--leader.id",
        type=str,
        required=True,
        dest="leader_id",
        help="Unique identifier for the leader robot arm",
    )
    leader_group.add_argument(
        "--leader.wrist_cam_id",
        type=int,
        help="Camera ID for leader wrist camera (disabled if not provided)",
        dest="leader_wrist_cam_id",
    )
    leader_group.add_argument(
        "--leader.env_cam_id",
        type=int,
        help="Camera ID for leader environment camera (disabled if not provided)",
        dest="leader_env_cam_id",
    )

    # Follower robot configuration
    follower_group = parser.add_argument_group("follower", "Follower robot configuration")
    follower_group.add_argument(
        "--follower.port",
        type=str,
        required=True,
        dest="follower_port",
        help="USB port for follower SO-101 robot (e.g., /dev/ttyUSB1)",
    )
    follower_group.add_argument(
        "--follower.id",
        type=str,
        required=True,
        dest="follower_id",
        help="Unique identifier for the follower robot",
    )

    # Output configuration
    output_group = parser.add_argument_group("output", "Output configuration")
    output_group.add_argument(
        "--output.write_mcap",
        action="store_true",
        dest="output_write_mcap",
        help="Write data to MCAP file",
    )
    output_group.add_argument(
        "--output.mcap_path",
        type=str,
        dest="output_mcap_path",
        help="Path for MCAP output file (auto-generated if not specified)",
    )

    return parser.parse_args()


def setup_camera(cam_id: int, topic_name: str) -> tuple[OpenCVCamera, RawImageChannel]:
    """Setup camera and return camera instance and channel."""
    cam_config = OpenCVCameraConfig(
        index_or_path=cam_id,
        fps=30,
        width=640,
        height=480,
        color_mode=ColorMode.RGB,
        rotation=Cv2Rotation.NO_ROTATION,
    )
    camera = OpenCVCamera(cam_config)
    camera.connect()
    image_channel = RawImageChannel(topic=topic_name)
    return camera, image_channel


def publish_camera_frame(camera: OpenCVCamera, image_channel: RawImageChannel) -> None:
    """Read and publish a camera frame."""
    frame = camera.async_read(timeout_ms=200)
    img_msg = RawImage(
        data=frame.tobytes(),
        width=frame.shape[1],
        height=frame.shape[0],
        step=frame.shape[1] * 3,
        encoding="rgb8",
    )
    image_channel.log(img_msg)


def create_so101_leader(port: str, robot_id: str):
    """Create and connect an SO101 leader robot."""
    config = SO101LeaderConfig(port=port, id=robot_id)
    leader = SO101Leader(config)

    leader.connect()
    if not leader.is_connected:
        raise ConnectionError(f"Failed to connect to SO-101 leader arm. Please check the connection.")

    return leader


def create_so101_follower(port: str, robot_id: str):
    """Create and connect an SO101 follower robot."""
    config = SO101FollowerConfig(port=port, id=robot_id)
    follower = SO101Follower(config)

    follower.connect()
    if not follower.is_connected:
        raise ConnectionError(f"Failed to connect to SO-101 follower arm. Please check the connection.")

    return follower


def update_follower_position(leader, follower):
    """Update follower robot position to match leader robot position."""
    if follower is None:
        return

    try:
        # Get action from leader
        action = leader.get_action()

        # Send action to follower
        follower.send_action(action)

    except Exception as e:
        print(f"Error updating follower position: {e}")


def main():
    args = parse_args()

    foxglove.set_log_level(logging.INFO)

    # Load SO101 URDF file
    print(f"Loading URDF from {SO101_URDF_FILE} ...")
    robot = URDF.load(SO101_URDF_FILE)

    # Setup MCAP writer if requested
    writer = None
    if args.output_write_mcap:
        if args.output_mcap_path:
            file_name = args.output_mcap_path
        else:
            now_str = datetime.datetime.now().strftime("%Y-%m-%d_%H-%M-%S")
            file_name = f"so101_dual_arm_{args.leader_id}_{now_str}.mcap"
        print(f"Writing data to MCAP file: {file_name}")
        writer = foxglove.open_mcap(file_name)

    # Start the Foxglove server
    server = foxglove.start_server()
    print(f"Foxglove server started at {server.app_url()}")

    # Setup cameras if requested
    wrist_camera = None
    wrist_image_channel = None
    env_camera = None
    env_image_channel = None

    if args.leader_wrist_cam_id is not None:
        print(f"Setting up leader wrist camera (ID: {args.leader_wrist_cam_id})...")
        try:
            wrist_camera, wrist_image_channel = setup_camera(
                args.leader_wrist_cam_id, "leader_wrist_image"
            )
            print("Leader wrist camera connected successfully.")
        except Exception as e:
            print(f"Failed to setup leader wrist camera: {e}")

    if args.leader_env_cam_id is not None:
        print(f"Setting up leader environment camera (ID: {args.leader_env_cam_id})...")
        try:
            env_camera, env_image_channel = setup_camera(
                args.leader_env_cam_id, "leader_env_image"
            )
            print("Leader environment camera connected successfully.")
        except Exception as e:
            print(f"Failed to setup leader environment camera: {e}")

    # Connect to leader SO101 robot
    print("Connecting to SO-101 leader arm...")
    try:
        leader = create_so101_leader(args.leader_port, args.leader_id)
        print("SO-101 leader arm connected successfully.")
    except Exception as e:
        print(f"Failed to connect to leader arm: {e}")
        return

    # Connect to follower SO101 robot
    print("Connecting to SO-101 follower arm...")
    try:
        follower = create_so101_follower(args.follower_port, args.follower_id)
        print("SO-101 follower arm connected successfully.")
    except Exception as e:
        print(f"Failed to connect to follower arm: {e}")
        print("Continuing with leader arm only...")
        follower = None

    # Define initial joint positions (all zeros for now)
    joint_positions = {}
    for joint in robot.robot.joints:
        joint_positions[joint.name] = 0.0

    print(f"Available joints: {list(joint_positions.keys())}")

    try:
        while True:
            # Read and publish leader wrist camera frame if available
            if wrist_camera and wrist_image_channel:
                try:
                    publish_camera_frame(wrist_camera, wrist_image_channel)
                except Exception as e:
                    print(f"Error reading leader wrist camera: {e}")

            # Read and publish leader environment camera frame if available
            if env_camera and env_image_channel:
                try:
                    publish_camera_frame(env_camera, env_image_channel)
                except Exception as e:
                    print(f"Error reading leader environment camera: {e}")

            # Get action from leader and update follower
            if follower:
                update_follower_position(leader, follower)

            # For visualization, we need to get the current state from the leader
            # Note: SO101Leader doesn't have get_observation, so we'll use the follower's state
            # or create a mock state for visualization
            if follower and follower.is_connected:
                # Get current state from follower for visualization
                obs = follower.get_observation()

                joint_positions["shoulder_pan"] = math.radians(
                    obs.get("shoulder_pan.pos", 0.0)
                )
                joint_positions["shoulder_lift"] = math.radians(
                    obs.get("shoulder_lift.pos", 0.0)
                )
                joint_positions["elbow_flex"] = math.radians(obs.get("elbow_flex.pos", 0.0))
                joint_positions["wrist_flex"] = math.radians(obs.get("wrist_flex.pos", 0.0))
                joint_positions["wrist_roll"] = math.radians(obs.get("wrist_roll.pos", 0.0))
                # Convert gripper percent (0-100) to radians (0 to pi)
                joint_positions["gripper"] = (
                    (obs.get("gripper.pos", 0.0) - 10) / 100.0
                ) * math.pi
            else:
                # If no follower, keep current joint positions (or use default)
                pass

            # Update robot configuration for forward kinematics
            robot.update_cfg(joint_positions)

            transforms = []
            # World -> Base
            transforms.append(
                FrameTransform(
                    parent_frame_id=WORLD_FRAME_ID,
                    child_frame_id=BASE_FRAME_ID,
                    translation=Vector3(x=0.0, y=0.0, z=0.0),
                    rotation=Quaternion(x=0.0, y=0.0, z=0.0, w=1.0),
                )
            )
            # Per-joint transforms
            for joint in robot.robot.joints:
                parent_link = joint.parent
                child_link = joint.child
                # Get transform from parent to child using yourdfpy's get_transform method
                T_local = robot.get_transform(
                    frame_to=child_link, frame_from=parent_link
                )
                trans = T_local[:3, 3]
                # Use scipy to convert rotation matrix to quaternion (x, y, z, w)
                quat = R.from_matrix(T_local[:3, :3]).as_quat()
                transforms.append(
                    FrameTransform(
                        parent_frame_id=parent_link,
                        child_frame_id=child_link,
                        translation=Vector3(
                            x=float(trans[0]), y=float(trans[1]), z=float(trans[2])
                        ),
                        rotation=Quaternion(
                            x=float(quat[0]),
                            y=float(quat[1]),
                            z=float(quat[2]),
                            w=float(quat[3]),
                        ),
                    )
                )

            foxglove.log("/tf", FrameTransforms(transforms=transforms))

            time.sleep(1.0 / RATE_HZ)

    except KeyboardInterrupt:
        print("\nShutting down SO-101 dual arm visualization...")
    finally:
        server.stop()
        leader.disconnect()
        if follower:
            follower.disconnect()

        if wrist_camera:
            wrist_camera.disconnect()
        if env_camera:
            env_camera.disconnect()
        if writer:
            writer.close()
            print("MCAP file saved successfully.")


if __name__ == "__main__":
    main()
