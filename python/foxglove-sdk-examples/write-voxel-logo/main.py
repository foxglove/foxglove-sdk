#!/usr/bin/env python3
"""
Generate an animated 3D voxel Foxglove logo using VoxelGrid schema.

This example demonstrates:
1. Using Foxglove's native VoxelGrid schema
2. Creating sparse voxel data with three moving groups
3. Animating voxels from edges toward center
4. Maintaining a 20x20x20 empty cube in the center
5. Writing animated voxel data to an MCAP file

The animation shows three rectangular voxel groups moving from the edges
of the space toward the center. A 20x20x20 cube in the center always
remains empty, creating a hollow core effect.
"""

import argparse
import time
from typing import List, Tuple, Set

import foxglove
from foxglove.channels import VoxelGridChannel
from foxglove.schemas import (
    VoxelGrid,
    PackedElementField,
    Pose,
    Quaternion,
    Vector3,
    Timestamp,
)
from foxglove.schemas import PackedElementFieldNumericType as NumericType


# Foxglove purple color #664FFE
FOXGLOVE_COLOR = (102, 79, 254, 255)  # RGBA


def lerp(start: float, end: float, t: float) -> float:
    """Linear interpolation between start and end."""
    return start + (end - start) * t


def create_voxel_group(
    center: Tuple[float, float, float],
    dimensions: Tuple[int, int, int],
) -> Set[Tuple[int, int, int]]:
    """
    Generate voxel positions for a rectangular group.

    Args:
        center: Center position (x, y, z)
        dimensions: Dimensions in voxels (width, height, depth)

    Returns:
        Set of (x, y, z) voxel coordinates
    """
    voxels = set()
    cx, cy, cz = center
    dx, dy, dz = dimensions

    # Calculate half dimensions
    half_dx = dx / 2.0
    half_dy = dy / 2.0
    half_dz = dz / 2.0

    # Generate voxel positions
    for x in range(int(cx - half_dx), int(cx - half_dx + dx)):
        for y in range(int(cy - half_dy), int(cy - half_dy + dy)):
            for z in range(int(cz - half_dz), int(cz - half_dz + dz)):
                # Ensure voxels are within grid bounds
                if 0 <= x < 100 and 0 <= y < 100 and 0 <= z < 100:
                    voxels.add((x, y, z))

    return voxels


def create_sparse_voxel_data(
    occupied_voxels: Set[Tuple[int, int, int]],
    grid_dims: Tuple[int, int, int],
    color: Tuple[int, int, int, int],
) -> bytes:
    """
    Create voxel data in depth-major, row-major (Z-Y-X) order.

    Each voxel has 5 bytes: occupied (1 byte) + RGBA (4 bytes)

    Args:
        occupied_voxels: Set of (x, y, z) coordinates for occupied voxels
        grid_dims: Grid dimensions (width, height, depth)
        color: RGBA color tuple

    Returns:
        Binary data for voxel grid
    """
    width, height, depth = grid_dims

    # Calculate data size: 5 bytes per voxel (occupied + RGBA)
    data_size = width * height * depth * 5
    data = bytearray(data_size)

    # Fill in occupied voxels
    for x, y, z in occupied_voxels:
        # Calculate index in Z-Y-X order
        index = (z * height * width + y * width + x) * 5

        # Set occupied flag and color
        data[index] = 1  # occupied
        data[index + 1] = color[0]  # red
        data[index + 2] = color[1]  # green
        data[index + 3] = color[2]  # blue
        data[index + 4] = color[3]  # alpha

    return bytes(data)


def create_voxel_grid_message(
    frame_num: int,
    animation_frames: int,
    total_frames: int,
    grid_dims: Tuple[int, int, int] = (100, 100, 100),
) -> VoxelGrid:
    """
    Create a VoxelGrid message for the given frame.

    Args:
        frame_num: Current frame number (0-based)
        animation_frames: Number of frames for the animation portion
        total_frames: Total number of frames (including static portion)
        grid_dims: Grid dimensions

    Returns:
        VoxelGrid message
    """
    # Calculate normalized time [0, 1] for animation
    # After animation completes, keep t at 1.0 for static frames
    if frame_num >= animation_frames:
        t = 1.0  # Keep at final position
    else:
        t = frame_num / (animation_frames - 1) if animation_frames > 1 else 0

    # Grid dimensions
    width, height, depth = grid_dims

    # Define starting and ending positions for each group
    # Using scaled coordinates (100x100x100 grid represents 1000x1000x1000 space)
    center = (50, 50, 50)

    # Group 1: 100x20x1 -> scaled to 10x2x1 in our grid
    group1_start = (10, 50, 50)  # Near left edge - travels along X axis
    group1_dims = (10, 2, 1)

    # Group 2: 20x100x1 -> scaled to 2x10x1 in our grid
    group2_start = (50, 10, 50)  # Near bottom edge - travels along Y axis
    group2_dims = (2, 10, 1)

    # Group 3: 1x20x100 -> scaled to 1x2x10 in our grid
    group3_start = (50, 50, 10)  # Near front edge - travels along Z axis
    group3_dims = (1, 2, 10)

    # Calculate current positions using linear interpolation
    group1_pos = (
        lerp(group1_start[0], center[0], t),
        lerp(group1_start[1], center[1], t),
        lerp(group1_start[2], center[2], t),
    )

    group2_pos = (
        lerp(group2_start[0], center[0], t),
        lerp(group2_start[1], center[1], t),
        lerp(group2_start[2], center[2], t),
    )

    group3_pos = (
        lerp(group3_start[0], center[0], t),
        lerp(group3_start[1], center[1], t),
        lerp(group3_start[2], center[2], t),
    )

    # Generate voxel positions for each group
    group1_voxels = create_voxel_group(group1_pos, group1_dims)
    group2_voxels = create_voxel_group(group2_pos, group2_dims)
    group3_voxels = create_voxel_group(group3_pos, group3_dims)

    # Combine all occupied voxels (union handles overlaps)
    all_occupied = group1_voxels | group2_voxels | group3_voxels

    # Remove voxels from the central 2x2x2 cube (always empty)
    # Center is at (50, 50, 50), so the cube spans from 40 to 59 (inclusive)
    center_empty_region = set()
    for x in range(49, 51):
        for y in range(49, 51):
            for z in range(49, 51):
                center_empty_region.add((x, y, z))

    # Remove central region from occupied voxels
    all_occupied = all_occupied - center_empty_region

    # Create voxel data
    voxel_data = create_sparse_voxel_data(all_occupied, grid_dims, FOXGLOVE_COLOR)

    # Create timestamp
    timestamp_ns = frame_num * 100_000_000  # 100ms per frame (10 fps)
    timestamp = Timestamp(
        sec=timestamp_ns // 1_000_000_000,
        nsec=timestamp_ns % 1_000_000_000,
    )

    # Define fields for the packed data
    fields = [
        PackedElementField(name="occupied", offset=0, type=NumericType.Uint8),
        PackedElementField(name="red", offset=1, type=NumericType.Uint8),
        PackedElementField(name="green", offset=2, type=NumericType.Uint8),
        PackedElementField(name="blue", offset=3, type=NumericType.Uint8),
        PackedElementField(name="alpha", offset=4, type=NumericType.Uint8),
    ]

    # Create VoxelGrid message
    # Note: Using 0.1m cell size means our 100x100x100 grid represents 10m x 10m x 10m
    # This scales our logical 1000x1000x1000 space appropriately
    voxel_grid = VoxelGrid(
        timestamp=timestamp,
        frame_id="world",
        pose=Pose(
            position=Vector3(x=0, y=0, z=0),
            orientation=Quaternion(x=0, y=0, z=0, w=1),
        ),
        row_count=height,
        column_count=width,
        cell_size=Vector3(x=0.1, y=0.1, z=0.1),  # 10cm per voxel
        slice_stride=width * height * 5,  # bytes per z-slice
        row_stride=width * 5,  # bytes per row
        cell_stride=5,  # bytes per cell (occupied + RGBA)
        fields=fields,
        data=voxel_data,
    )

    return voxel_grid


def main() -> None:
    """Main function to generate the animated voxel logo."""
    parser = argparse.ArgumentParser(
        description="Generate an animated 3D voxel Foxglove logo"
    )
    parser.add_argument(
        "--path",
        type=str,
        default="voxel_logo_animated.mcap",
        help="Output MCAP file path",
    )
    parser.add_argument(
        "--duration",
        type=float,
        default=10.0,
        help="Animation duration in seconds",
    )
    parser.add_argument(
        "--static-duration",
        type=float,
        default=3.0,
        help="Duration to hold final position in seconds",
    )
    parser.add_argument(
        "--fps",
        type=int,
        default=10,
        help="Frames per second",
    )
    args = parser.parse_args()

    # Calculate frames for animation and static portions
    animation_frames = int(args.duration * args.fps)
    static_frames = int(args.static_duration * args.fps)
    total_frames = animation_frames + static_frames

    print(f"Generating animated voxel logo...")
    print(f"Animation duration: {args.duration} seconds")
    print(f"Static duration: {args.static_duration} seconds")
    print(f"Total duration: {args.duration + args.static_duration} seconds")
    print(f"FPS: {args.fps}")
    print(
        f"Total frames: {total_frames} ({animation_frames} animation + {static_frames} static)"
    )
    print(f"Grid size: 100x100x100 voxels (representing 1000x1000x1000 logical space)")
    print(f"Voxel size: 0.1m (10cm)")
    print(f"Effective space: 10m x 10m x 10m")
    print(f"Foxglove color: #664FFE")
    print()

    # Create VoxelGrid channel
    voxel_channel = VoxelGridChannel("/voxel_grid")

    # Open MCAP file for writing
    with foxglove.open_mcap(args.path, allow_overwrite=True):
        print(f"Writing animation to {args.path}...")

        # Generate and log frames
        for frame in range(total_frames):
            # Create voxel grid for this frame
            voxel_grid = create_voxel_grid_message(
                frame, animation_frames, total_frames
            )

            # Log the message
            timestamp_ns = frame * (1_000_000_000 // args.fps)
            voxel_channel.log(voxel_grid, log_time=timestamp_ns)

            # Progress update
            if frame % 10 == 0 or frame == total_frames - 1:
                progress = (frame + 1) / total_frames * 100
                status = "animation" if frame < animation_frames else "static"
                print(
                    f"  Frame {frame + 1}/{total_frames} ({progress:.1f}%) - {status}"
                )

    print()
    print(f"✓ Successfully generated animated voxel logo: {args.path}")
    print()
    print("Animation details:")
    print("  - Three voxel groups move from edges toward center")
    print("  - Group 1: Horizontal bar (100x20x1 logical voxels) - moves along X axis")
    print("  - Group 2: Vertical bar (20x100x1 logical voxels) - moves along Y axis")
    print("  - Group 3: Depth bar (1x20x100 logical voxels) - moves along Z axis")
    print("  - Central 20x20x20 cube always remains empty (hollow core)")
    print("  - Groups converge around the empty center")
    print()
    print("To view the animation:")
    print("  1. Open Foxglove Studio")
    print("  2. Load the generated MCAP file")
    print("  3. Add a 3D panel")
    print("  4. The voxel animation will play automatically")


if __name__ == "__main__":
    main()
