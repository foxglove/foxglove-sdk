# Fisheye Rectification Demo Example

This example demonstrates how to generate an MCAP file containing real-world fisheye camera data and a corresponding CameraCalibration message using the Kannala-Brandt distortion model. The data can be used to demo Foxglove's support for rectifying fisheye camera data.

## Dataset

We use a real-world sequence from the [FAU Fisheye Data Set](https://www.lms.tf.fau.eu/research/downloads/fisheye-data-set), which provides both synthetic and real-world fisheye video sequences for research purposes.

## Steps

1. Download a real-world fisheye image sequence and calibration images from the FAU dataset.
2. Estimate Kannala-Brandt calibration parameters from the calibration images (or use a hardcoded guess if estimation fails).
3. Write the image sequence and CameraCalibration message to an MCAP file using the Foxglove Python SDK.

## Citation

If you use this data, please cite:

> A Data Set Providing Synthetic and Real-World Fisheye Video Sequences, Andrea Eichenseer and André Kaup, ICASSP 2016.

See the dataset page for more details: https://www.lms.tf.fau.eu/research/downloads/fisheye-data-set
