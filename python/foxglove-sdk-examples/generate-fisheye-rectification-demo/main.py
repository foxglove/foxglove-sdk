import os
import io
import time
import imageio.v2 as imageio  # type: ignore
import numpy as np
from typing import Tuple, Dict, Any
from datetime import datetime, timezone, timedelta
from foxglove import open_mcap  # type: ignore
from foxglove.channels import CameraCalibrationChannel, CompressedImageChannel  # type: ignore
from foxglove.schemas import CameraCalibration, CompressedImage, Timestamp  # type: ignore

PNG_FILENAME = "00000_MVR.png"
MCAP_FILENAME = "fisheye_rectification_demo.mcap"

# PDT is UTC-7
PDT = timezone(timedelta(hours=-7))
INITIAL_TIME = datetime(2024, 10, 18, 13, 6, 12, 753000, tzinfo=PDT)
INITIAL_TIME_NS = int(INITIAL_TIME.timestamp() * 1e9)


def get_hardcoded_kannala_brandt_params(image_shape: Tuple[int, int]) -> Dict[str, Any]:
    h: int = 200
    w: int = 232
    fx: float = 112.5
    fy: float = 112.5
    cx: float = w / 2.0
    cy: float = h / 2.0
    k1: float = 0.14
    k2: float = -0.09
    k3: float = 0.05
    k4: float = -0.005
    return dict(
        model="kannala_brandt",
        intrinsics=[fx, fy, cx, cy, k1, k2, k3, k4],
        width=w,
        height=h,
    )


def write_mcap_with_image_and_calib(
    image_path: str, calib_params: Dict[str, Any], out_path: str
) -> None:
    frame_id = "cam_left"
    print(f"Writing image and calibration to {out_path}")
    with open_mcap(out_path, allow_overwrite=True) as mcap_writer:
        calib_channel = CameraCalibrationChannel("/camera3/camera_info/fisheye")
        image_channel = CompressedImageChannel("/camera/image/compressed")

        width: int = int(calib_params["width"])
        height: int = int(calib_params["height"])
        model: str = str(calib_params["model"])
        intrinsics = calib_params["intrinsics"]
        D = [float(x) for x in intrinsics[4:]]
        K = [
            float(intrinsics[0]),
            0.0,
            float(intrinsics[2]),
            0.0,
            float(intrinsics[1]),
            float(intrinsics[3]),
            0.0,
            0.0,
            1.0,
        ]
        R = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
        P = [
            float(intrinsics[0]),
            0.0,
            float(intrinsics[2]),
            0.0,
            0.0,
            float(intrinsics[1]),
            float(intrinsics[3]),
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
        ]
        # Set initial timestamp
        t_ns = INITIAL_TIME_NS
        img = imageio.imread(image_path)
        for i in range(10):
            # Publish calibration message at this timestamp
            calib_msg = CameraCalibration(
                timestamp=Timestamp.from_epoch_secs(t_ns / 1e9),
                frame_id=frame_id,
                width=width,
                height=height,
                distortion_model=model,
                D=D,
                K=K,
                R=R,
                P=P,
            )
            calib_channel.log(calib_msg, log_time=t_ns)

            buf = io.BytesIO()
            imageio.imwrite(buf, img, format="jpeg")
            jpeg_bytes = buf.getvalue()
            img_msg = CompressedImage(
                timestamp=Timestamp.from_epoch_secs(t_ns / 1e9),
                frame_id=frame_id,
                data=jpeg_bytes,
                format="jpeg",
            )
            image_channel.log(img_msg, log_time=t_ns)
            if i < 4:
                time.sleep(1)
            t_ns += int(1e9)  # increment by 1 second


def main() -> None:
    if not os.path.exists(PNG_FILENAME):
        raise RuntimeError(f"Image file {PNG_FILENAME} not found!")
    img = imageio.imread(PNG_FILENAME)
    if not isinstance(img, np.ndarray):
        raise RuntimeError("Loaded image is not a numpy array!")
    img_shape: Tuple[int, int] = (int(img.shape[0]), int(img.shape[1]))
    params: Dict[str, Any] = get_hardcoded_kannala_brandt_params(img_shape)
    write_mcap_with_image_and_calib(PNG_FILENAME, params, MCAP_FILENAME)


if __name__ == "__main__":
    main()
