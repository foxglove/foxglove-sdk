# Animated Voxel Logo

An example from the Foxglove SDK.

This example demonstrates how to generate an animated 3D voxel logo using Foxglove's native `VoxelGrid` schema. The animation shows three rectangular groups of voxels moving from the edges of a sparse voxel grid toward the center. A 20x20x20 cube in the center always remains empty, creating a hollow core effect. After the 10-second animation completes, the voxels hold their final position for an additional 3 seconds.

## Features

- **Native VoxelGrid Schema**: Uses Foxglove's built-in VoxelGrid message schema
- **3D Animation**: Creates a 10-second animation at 10 FPS, plus 3 seconds of static final position
- **Sparse Voxel Grid**: Efficiently represents a large logical space (1000x1000x1000) using a scaled 100x100x100 grid
- **Three Moving Groups**: Simulates three rectangular voxel groups converging at the center
- **Hollow Core**: Maintains a 20x20x20 empty cube in the center throughout the animation
- **Static Hold**: After animation completes, holds the final position for additional seconds
- **Foxglove Purple**: All voxels are colored in Foxglove's signature purple (#664FFE)
- **MCAP Output**: Writes the animated voxel data to an MCAP file for viewing in Foxglove Studio

## Animation Details

The animation consists of three groups of voxels:

- **Group 1**: Horizontal bar (100x20x1 logical voxels) - moves from left edge along the X axis
- **Group 2**: Vertical bar (20x100x1 logical voxels) - moves from bottom edge along the Y axis
- **Group 3**: Depth bar (1x20x100 logical voxels) - moves from front edge along the Z axis

All groups move linearly toward the center (500, 500, 500 in logical space) over 10 seconds. A 20x20x20 cube (200x200x200 in logical space) at the center always remains empty, creating a hollow core effect. After reaching their final positions, the voxels remain static for an additional 3 seconds.

## Message Structure

The example uses Foxglove's `VoxelGrid` schema with the following fields:

- `timestamp`: Frame timestamp
- `frame_id`: "world"
- `pose`: Origin at (0, 0, 0)
- `row_count`, `column_count`: Grid dimensions (100x100)
- `cell_size`: Vector3(0.1, 0.1, 0.1) - 10cm per voxel
- `fields`: PackedElementField array defining data structure
  - "occupied" (uint8): 0 = empty, 1 = occupied
  - "red", "green", "blue", "alpha" (uint8): Color channels
- `data`: Binary data in depth-major, row-major (Z-Y-X) order

## Usage

This example uses Poetry: https://python-poetry.org/

```bash
# Install dependencies
poetry install

# Run with default settings (10 second animation)
poetry run python main.py

# Specify custom duration
poetry run python main.py --duration 5.0

# Change frame rate
poetry run python main.py --fps 20

# Specify output file
poetry run python main.py --path my_animation.mcap
```

## Command Line Options

- `--path`: Output MCAP file path (default: `voxel_logo_animated.mcap`)
- `--duration`: Animation duration in seconds (default: 10.0)
- `--static-duration`: Duration to hold final position in seconds (default: 3.0)
- `--fps`: Frames per second (default: 10)

## Viewing the Results

1. Run the example to generate the MCAP file
2. Open Foxglove Studio
3. Load the generated MCAP file
4. Add a 3D panel to your layout
5. The voxel animation will play automatically
6. Use the timeline controls to scrub through the animation

## Example Output

The example generates output like:

```
Generating animated voxel logo...
Animation duration: 10.0 seconds
Static duration: 3.0 seconds
Total duration: 13.0 seconds
FPS: 10
Total frames: 130 (100 animation + 30 static)
Grid size: 100x100x100 voxels (representing 1000x1000x1000 logical space)
Voxel size: 0.1m (10cm)
Effective space: 10m x 10m x 10m
Foxglove color: #664FFE

Writing animation to voxel_logo_animated.mcap...
  Frame 1/130 (0.8%) - animation
  Frame 11/130 (8.5%) - animation
  ...
  Frame 100/130 (76.9%) - animation
  Frame 110/130 (84.6%) - static
  ...
  Frame 130/130 (100.0%) - static

✓ Successfully generated animated voxel logo: voxel_logo_animated.mcap

Animation details:
  - Three voxel groups move from edges toward center
  - Group 1: Horizontal bar (100x20x1 logical voxels) - moves along X axis
  - Group 2: Vertical bar (20x100x1 logical voxels) - moves along Y axis
  - Group 3: Depth bar (1x20x100 logical voxels) - moves along Z axis
  - Central 20x20x20 cube always remains empty (hollow core)
  - Groups converge around the empty center
```

## Technical Details

### Scaling Approach

- **Logical Space**: 1000x1000x1000 voxels (as specified)
- **Physical Grid**: 100x100x100 voxels (for memory efficiency)
- **Scale Factor**: 10:1 (each physical voxel represents 10x10x10 logical voxels)
- **Cell Size**: 0.1m (10cm) per voxel
- **Total Space**: 10m x 10m x 10m effective visualization

### Data Format

- **Encoding**: Each voxel uses 5 bytes (1 for occupancy, 4 for RGBA)
- **Order**: Data is stored in depth-major, row-major (Z-Y-X) order
- **Memory**: ~50KB per frame (100x100x100 voxels × 5 bytes, mostly sparse)
- **File Size**: ~5MB for 10-second animation at 10 FPS

### Performance

- Efficient sparse voxel representation
- Linear interpolation for smooth animation
- Optimized for real-time playback in Foxglove Studio

## Implementation Notes

The example demonstrates several key concepts:

1. Using Foxglove's native schema types (`VoxelGrid`, `PackedElementField`, etc.)
2. Creating sparse voxel data efficiently
3. Animating 3D data over time
4. Working with the MCAP file format
5. Proper data ordering for voxel grids (Z-Y-X)

This example is useful for:

- Understanding VoxelGrid schema usage
- Learning 3D animation techniques
- Testing voxel visualization in Foxglove Studio
- Building more complex voxel-based applications
