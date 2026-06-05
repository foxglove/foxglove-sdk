#!/usr/bin/env python3
"""Publish a scrolling color-bar test pattern as sensor_msgs/Image.

Used for testing image/video delivery through the bridge, in particular the
remote-access video-track transcode path. Pure-python byte slicing keeps frame
generation cheap; no numpy/cv dependencies.
"""
import rospy
from sensor_msgs.msg import Image

WIDTH = 640
HEIGHT = 360
FPS = 15
BYTES_PER_PIXEL = 3

# Classic color bars (rgb8).
COLORS = [
    (255, 255, 255),  # white
    (255, 255, 0),    # yellow
    (0, 255, 255),    # cyan
    (0, 255, 0),      # green
    (255, 0, 255),    # magenta
    (255, 0, 0),      # red
    (0, 0, 255),      # blue
    (40, 40, 40),     # near-black
]


def build_row():
    segment = WIDTH // len(COLORS)
    row = bytearray()
    for color in COLORS:
        row += bytes(color) * segment
    row += bytes(COLORS[-1]) * (WIDTH - segment * len(COLORS))
    return bytes(row)


def main():
    rospy.init_node("image_publisher")
    topic = rospy.get_param("~topic", "/camera/image")
    publisher = rospy.Publisher(topic, Image, queue_size=1)

    row = build_row()

    msg = Image()
    msg.header.frame_id = "camera"
    msg.width = WIDTH
    msg.height = HEIGHT
    msg.encoding = "rgb8"
    msg.is_bigendian = 0
    msg.step = WIDTH * BYTES_PER_PIXEL

    rate = rospy.Rate(FPS)
    offset = 0
    while not rospy.is_shutdown():
        # Scroll horizontally by rotating the row (pixel-aligned).
        byte_offset = (offset * BYTES_PER_PIXEL) % len(row)
        shifted = row[byte_offset:] + row[:byte_offset]
        msg.data = shifted * HEIGHT
        msg.header.stamp = rospy.Time.now()
        publisher.publish(msg)
        offset += 4
        rate.sleep()


if __name__ == "__main__":
    main()
