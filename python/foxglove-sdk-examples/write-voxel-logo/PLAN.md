# VoxelGrid Animation Implementation Plan

## Overview

Create an animated 3D Foxglove logo using the VoxelGrid schema. The animation will show three groups of voxels moving from the edges of a 1000x1000x1000 sparse voxel grid toward the center until they overlap.

## Key Requirements

- **Duration**: 13 seconds total (10 seconds animation + 3 seconds static)
- **Grid Size**: 1000x1000x1000 voxels (sparse - mostly empty)
- **Voxel Groups**: Three rectangular groups representing the Foxglove logo
  - Group 1: 100x20x1 voxels (horizontal bar)
  - Group 2: 20x100x1 voxels (vertical bar)
  - Group 3: 1x20x100 voxels (depth bar)
- **Hollow Core**: 20x20x20 cube in center always remains empty
- **Color**: Foxglove purple (#664FFE)
- **Animation**: Groups move from edges toward center

## Technical Design

### 1. VoxelGrid Schema Structure

Using Foxglove's native VoxelGrid schema:

- `timestamp`: Current time
- `frame_id`: "world"
- `pose`: Origin at (0, 0, 0)
- `row_count`, `column_count`, `slice_stride`: Grid dimensions (1000x1000x1000)
- `cell_size`: Size of each voxel (e.g., 0.01m = 1cm)
- `fields`: PackedElementField array defining data structure
  - "occupied" (uint8): 0 = empty, 1 = occupied
  - "red", "green", "blue", "alpha" (uint8): Color channels
- `data`: Binary data in depth-major, row-major (Z-Y-X) order

### 2. Data Structure

Since the grid is sparse (mostly empty), we need an efficient representation:

- Total voxels: 1,000,000,000 (1 billion)
- Data format: Each voxel needs 5 bytes (occupied + RGBA)
- Challenge: Full grid would be 5GB - too large!
- Solution: Use run-length encoding or only include occupied regions

### 3. Animation Strategy

**Frame Rate**: 10 frames per second (130 frames total: 100 animation + 30 static)

**Starting Positions**:

- Group 1 (100x20x1): Center at (50, 500, 500) - near left edge, travels along X axis
- Group 2 (20x100x1): Center at (500, 50, 500) - near bottom edge, travels along Y axis
- Group 3 (1x20x100): Center at (500, 500, 50) - near front edge, travels along Z axis

**Movement**:

- All groups move toward center (500, 500, 500)
- Linear interpolation over 10 seconds
- Central 20x20x20 cube (200x200x200 in logical space) always remains empty
- Groups converge around the hollow core in final frames
- After reaching final position, voxels remain static for 3 additional seconds

### 4. Implementation Steps

#### Step 1: Data Structure Optimization

Instead of storing the full 1 billion voxel grid, we'll:

1. Define a smaller "effective" grid (e.g., 200x200x200) for the animation
2. Set the VoxelGrid dimensions to show this is part of a larger 1000x1000x1000 space
3. Only encode the voxels in the effective region

#### Step 2: Voxel Group Generation

```python
def create_voxel_group(center, dimensions, color):
    """Generate voxel positions for a rectangular group"""
    # Calculate voxel positions relative to center
    # Return list of (x, y, z) coordinates
```

#### Step 3: Animation Loop

```python
for frame in range(100):  # 10 seconds at 10 fps
    t = frame / 100.0  # Normalized time [0, 1]

    # Calculate positions for each group
    group1_pos = lerp(start1, center, t)
    group2_pos = lerp(start2, center, t)
    group3_pos = lerp(start3, center, t)

    # Generate voxel data
    voxel_data = create_sparse_voxel_data(groups)

    # Create VoxelGrid message
    # Log to MCAP
```

#### Step 4: Sparse Data Encoding

```python
def encode_sparse_voxels(occupied_voxels, grid_dims):
    """Encode sparse voxel data efficiently"""
    # Option 1: Only encode bounding box around occupied voxels
    # Option 2: Use smaller cell_stride to skip empty regions
    # Option 3: Compress data using standard compression
```

### 5. Color Encoding

Foxglove purple #664FFE:

- Red: 0x66 = 102
- Green: 0x4F = 79
- Blue: 0xFE = 254
- Alpha: 0xFF = 255 (fully opaque)

### 6. Performance Considerations

- Memory usage: Keep under 100MB per frame
- Encoding time: Should complete in reasonable time
- File size: Target < 100MB for entire MCAP

### 7. Testing & Validation

- Verify voxel positions are correct
- Check animation smoothness
- Ensure colors display correctly
- Test MCAP file loads in Foxglove Studio

## Alternative Approach (If Full Grid is Too Large)

Instead of a true 1000x1000x1000 grid, we could:

1. Use a 100x100x100 grid with cell_size = 0.1m (10cm)
   - This gives an effective space of 10m x 10m x 10m
   - Scale the voxel groups proportionally
2. Or use dynamic grid bounds that adjust per frame
   - Set grid dimensions to encompass only occupied voxels
   - Update pose/dimensions each frame

## Next Steps

1. Implement basic VoxelGrid creation
2. Test with small grid first
3. Add animation logic
4. Optimize for sparse data
5. Fine-tune colors and timing
