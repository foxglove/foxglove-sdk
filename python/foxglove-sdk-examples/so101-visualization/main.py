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
from lerobot.robots.so101_follower import SO101Follower, SO101FollowerConfig
from scipy.spatial.transform import Rotation as R
from yourdfpy import URDF

WORLD_FRAME_ID = "world"
BASE_FRAME_ID = "base_link"
RATE_HZ = 30.0
URDF_FILE = "SO101/so101_new_calib.urdf"


def parse_args():
    parser = argparse.ArgumentParser(
        description="SO-101 robot arm visualization with Foxglove",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )

    # Robot configuration
    robot_group = parser.add_argument_group("robot", "Robot configuration")
    robot_group.add_argument(
        "--robot.port",
        type=str,
        required=True,
        dest="robot_port",
        help="USB port to connect to the SO-101 arm (e.g., /dev/ttyUSB0)",
    )
    robot_group.add_argument(
        "--robot.id",
        type=str,
        required=True,
        dest="robot_id",
        help="Unique identifier for the robot arm",
    )
    robot_group.add_argument(
        "--robot.wrist_cam_id",
        type=int,
        help="Camera ID for wrist camera (disabled if not provided)",
        dest="robot_wrist_cam_id",
    )
    robot_group.add_argument(
        "--robot.env_cam_id",
        type=int,
        help="Camera ID for environment camera (disabled if not provided)",
        dest="robot_env_cam_id",
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


def main():
    args = parse_args()

    foxglove.set_log_level(logging.INFO)

    print(f"Loading URDF from {URDF_FILE} ...")
    robot = URDF.load(URDF_FILE)

    # Setup MCAP writer if requested
    writer = None
    if args.output_write_mcap:
        if args.output_mcap_path:
            file_name = args.output_mcap_path
        else:
            now_str = datetime.datetime.now().strftime("%Y-%m-%d_%H-%M-%S")
            file_name = f"so_arm_101_{args.robot_id}_{now_str}.mcap"
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

    if args.robot_wrist_cam_id is not None:
        print(f"Setting up wrist camera (ID: {args.robot_wrist_cam_id})...")
        try:
            wrist_camera, wrist_image_channel = setup_camera(
                args.robot_wrist_cam_id, "wrist_image"
            )
            print("Wrist camera connected successfully.")
        except Exception as e:
            print(f"Failed to setup wrist camera: {e}")

    if args.robot_env_cam_id is not None:
        print(f"Setting up environment camera (ID: {args.robot_env_cam_id})...")
        try:
            env_camera, env_image_channel = setup_camera(
                args.robot_env_cam_id, "env_image"
            )
            print("Environment camera connected successfully.")
        except Exception as e:
            print(f"Failed to setup environment camera: {e}")

    # Connect to SO-101 arm
    config = SO101FollowerConfig(
        port=args.robot_port, id=args.robot_id, use_degrees=True
    )
    follower = SO101Follower(config)
    follower.connect(calibrate=False)
    if not follower.is_connected:
        print("Failed to connect to SO-101 Follower arm. Please check the connection.")
        return
    print("SO-101 Follower arm connected successfully.")
    follower.bus.disable_torque()  # Disable torque to be able to move the arm freely

    # Define initial joint positions (all zeros for now)
    joint_positions = {}
    for joint in robot.robot.joints:
        joint_positions[joint.name] = 0.0

    print(f"Available joints: {list(joint_positions.keys())}")

    try:
        while True:
            # Read and publish wrist camera frame if available
            if wrist_camera and wrist_image_channel:
                try:
                    publish_camera_frame(wrist_camera, wrist_image_channel)
                except Exception as e:
                    print(f"Error reading wrist camera: {e}")

            # Read and publish environment camera frame if available
            if env_camera and env_image_channel:
                try:
                    publish_camera_frame(env_camera, env_image_channel)
                except Exception as e:
                    print(f"Error reading environment camera: {e}")

            # Read actual joint angles from follower (in degrees)
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
        print("\nShutting down SO-101 visualization...")
    finally:
        server.stop()
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
