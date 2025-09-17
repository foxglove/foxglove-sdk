# Write LocationFixes Messages

An example from the Foxglove SDK demonstrating how to generate `LocationFixes` messages containing multiple location fixes that move around independently in the Puget Sound area.

## Overview

This example creates an MCAP file containing `LocationFixes` messages. Each message contains an array of 5 location fixes, each with:

- **Different colors**: Red, Green, Blue, Yellow, and Magenta
- **Different covariance types**: Known, DiagonalKnown, Approximated
- **Different movement patterns**: Circle, Figure-8, Zigzag, Spiral, and Random Walk
- **Closely grouped positions**: All in Elliott Bay waters, just west of downtown Seattle

All location fixes are positioned in the waters of Elliott Bay (part of Puget Sound next to Seattle) and move independently over a 10-second recording period.

## Features

- Generates 5 simultaneous location fixes in a single `LocationFixes` message
- Each location fix has its own movement pattern and visual properties
- Includes position covariance data for uncertainty visualization
- Altitude variations using sine wave patterns
- Configurable recording duration and publishing rate

## Usage

This example uses Poetry: https://python-poetry.org/

```bash
# Install dependencies
poetry install

# Run with default settings (10 seconds, 10 Hz)
poetry run python main.py

# Specify custom output file
poetry run python main.py --path my_locations.mcap

# Adjust duration and rate
poetry run python main.py --duration 20 --rate 5
```

### Command Line Arguments

- `--path`: Output MCAP file path (default: `location_fixes.mcap`)
- `--duration`: Duration of the recording in seconds (default: 10.0)
- `--rate`: Publishing rate in Hz (default: 10.0)

## Movement Patterns

The example implements 5 different movement patterns:

1. **Circle**: Circular motion around the starting point
2. **Figure-8**: Figure-eight pattern movement
3. **Zigzag**: Back and forth zigzag movement
4. **Spiral**: Expanding spiral pattern
5. **Random Walk**: Pseudo-random movement using sine waves

## Viewing the Recording

After generating the MCAP file, open it in Foxglove Studio to visualize:

1. The location fixes will appear on the map panel
2. Each fix will have a different color for easy identification
3. You can play back the recording to see the fixes moving around Puget Sound
4. The covariance ellipses will show the uncertainty for each position

## Location Fix Properties

Each location fix includes:

- **Timestamp**: Synchronized timestamps for all fixes
- **Frame ID**: Unique identifier for each tracker (e.g., "location_tracker_1")
- **Position**: Latitude, longitude, and altitude coordinates
- **Covariance**: 3x3 position covariance matrix for uncertainty
- **Color**: RGBA color for visualization in Foxglove Studio

## Elliott Bay Coordinates

The example places all 5 location fixes in Elliott Bay (the body of water just west of downtown Seattle):

- All trackers are positioned between approximately:
  - Latitude: 47.600°N to 47.615°N
  - Longitude: 122.375°W to 122.385°W

This ensures all location fixes are closely grouped in the water, visible on a single zoomed-in map view when opened in Foxglove Studio, making it easy to observe their different movement patterns.
