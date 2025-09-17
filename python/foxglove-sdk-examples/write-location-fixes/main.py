"""
Example demonstrating generation of LocationFixes messages with multiple moving location fixes
in the Puget Sound area.
"""

import argparse
import math
import time

import foxglove
from foxglove.channels import LocationFixesChannel
from foxglove.schemas import (
    Color,
    LocationFix,
    LocationFixes,
    LocationFixPositionCovarianceType,
    Timestamp,
)


def parse_args() -> argparse.Namespace:
    """Parse command line arguments."""
    parser = argparse.ArgumentParser(
        description="Generate LocationFixes messages with multiple moving location fixes",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument(
        "--path", type=str, default="location_fixes.mcap", help="Output MCAP file path"
    )
    parser.add_argument(
        "--duration",
        type=float,
        default=10.0,
        help="Duration of the recording in seconds",
    )
    parser.add_argument(
        "--rate", type=float, default=10.0, help="Publishing rate in Hz"
    )
    return parser.parse_args()


class LocationFixTracker:
    """Tracks and updates a single location fix as it moves."""

    def __init__(
        self,
        id: str,
        start_lat: float,
        start_lon: float,
        start_alt: float,
        color: Color,
        covariance_type: LocationFixPositionCovarianceType,
        base_covariance: list[float],
        movement_pattern: str,
    ):
        self.id = id
        self.lat = start_lat
        self.lon = start_lon
        self.alt = start_alt
        self.color = color
        self.covariance_type = covariance_type
        self.base_covariance = base_covariance
        self.movement_pattern = movement_pattern
        self.time_offset = 0.0

    def update(self, elapsed_time: float):
        """Update the position based on the movement pattern."""
        # Different movement patterns for each location fix (scaled for closer proximity)
        if self.movement_pattern == "circle":
            # Circular motion
            radius = 0.003  # degrees (about 300m diameter)
            speed = 0.5  # radians per second
            self.lat = self.lat + radius * math.cos(speed * elapsed_time) * 0.01
            self.lon = self.lon + radius * math.sin(speed * elapsed_time) * 0.01

        elif self.movement_pattern == "figure8":
            # Figure-8 pattern
            scale = 0.0025  # smaller scale for closer proximity
            speed = 0.3
            t = speed * elapsed_time
            self.lat = self.lat + scale * math.sin(t) * 0.01
            self.lon = self.lon + scale * math.sin(2 * t) * 0.01

        elif self.movement_pattern == "zigzag":
            # Zigzag pattern
            amplitude = 0.002  # smaller amplitude
            period = 2.0  # seconds
            phase = (elapsed_time % period) / period
            if phase < 0.5:
                self.lat += amplitude * 0.01
                self.lon += amplitude * 0.01
            else:
                self.lat -= amplitude * 0.01
                self.lon -= amplitude * 0.01

        elif self.movement_pattern == "spiral":
            # Expanding spiral
            growth_rate = 0.0003  # slower growth for tighter spiral
            speed = 0.4
            radius = growth_rate * elapsed_time
            self.lat = self.lat + radius * math.cos(speed * elapsed_time) * 0.01
            self.lon = self.lon + radius * math.sin(speed * elapsed_time) * 0.01

        elif self.movement_pattern == "random_walk":
            # Random walk (using sine waves with different frequencies for pseudo-randomness)
            self.lat += 0.001 * math.sin(0.7 * elapsed_time + 1.2) * 0.01
            self.lon += 0.001 * math.cos(1.1 * elapsed_time + 0.5) * 0.01

        # Update altitude with a gentle sine wave
        self.alt = 100.0 + 20.0 * math.sin(0.2 * elapsed_time + hash(self.id) % 10)

        # Vary covariance slightly over time
        covariance_scale = 1.0 + 0.2 * math.sin(0.3 * elapsed_time)
        self.current_covariance = [v * covariance_scale for v in self.base_covariance]

    def to_location_fix(self, timestamp_ns: int) -> LocationFix:
        """Convert to a LocationFix message."""
        # Convert nanoseconds to seconds and nanoseconds
        sec = timestamp_ns // 1_000_000_000
        nsec = timestamp_ns % 1_000_000_000

        return LocationFix(
            timestamp=Timestamp(sec=sec, nsec=nsec),
            frame_id=f"location_{self.id}",
            latitude=self.lat,
            longitude=self.lon,
            altitude=self.alt,
            position_covariance=self.current_covariance,
            position_covariance_type=self.covariance_type,
            color=self.color,
        )


def create_location_trackers() -> list[LocationFixTracker]:
    """Create 5 location trackers with different properties, all in Elliott Bay near Seattle."""

    # Elliott Bay / Puget Sound waters just west of downtown Seattle
    # All trackers start close together in the water
    locations = [
        # Northwest position in Elliott Bay
        LocationFixTracker(
            id="tracker_1",
            start_lat=47.615,  # Elliott Bay - northwest
            start_lon=-122.385,
            start_alt=100.0,
            color=Color(r=1.0, g=0.0, b=0.0, a=1.0),  # Red
            covariance_type=LocationFixPositionCovarianceType.Known,
            base_covariance=[
                0.1,
                0.0,
                0.0,
                0.0,
                0.1,
                0.0,
                0.0,
                0.0,
                0.2,
            ],  # 3x3 covariance matrix
            movement_pattern="circle",
        ),
        # North-central position in Elliott Bay
        LocationFixTracker(
            id="tracker_2",
            start_lat=47.612,  # Elliott Bay - north-central
            start_lon=-122.378,
            start_alt=150.0,
            color=Color(r=0.0, g=1.0, b=0.0, a=1.0),  # Green
            covariance_type=LocationFixPositionCovarianceType.DiagonalKnown,
            base_covariance=[0.2, 0.0, 0.0, 0.0, 0.2, 0.0, 0.0, 0.0, 0.3],
            movement_pattern="figure8",
        ),
        # Central position in Elliott Bay
        LocationFixTracker(
            id="tracker_3",
            start_lat=47.608,  # Elliott Bay - central
            start_lon=-122.380,
            start_alt=80.0,
            color=Color(r=0.0, g=0.0, b=1.0, a=1.0),  # Blue
            covariance_type=LocationFixPositionCovarianceType.Approximated,
            base_covariance=[0.15, 0.05, 0.0, 0.05, 0.15, 0.0, 0.0, 0.0, 0.25],
            movement_pattern="zigzag",
        ),
        # Southwest position in Elliott Bay
        LocationFixTracker(
            id="tracker_4",
            start_lat=47.604,  # Elliott Bay - southwest
            start_lon=-122.382,
            start_alt=120.0,
            color=Color(r=1.0, g=1.0, b=0.0, a=1.0),  # Yellow
            covariance_type=LocationFixPositionCovarianceType.Known,
            base_covariance=[0.08, 0.02, 0.0, 0.02, 0.08, 0.0, 0.0, 0.0, 0.18],
            movement_pattern="spiral",
        ),
        # South position in Elliott Bay
        LocationFixTracker(
            id="tracker_5",
            start_lat=47.600,  # Elliott Bay - south
            start_lon=-122.375,
            start_alt=90.0,
            color=Color(r=1.0, g=0.0, b=1.0, a=1.0),  # Magenta
            covariance_type=LocationFixPositionCovarianceType.DiagonalKnown,
            base_covariance=[0.12, 0.0, 0.0, 0.0, 0.12, 0.0, 0.0, 0.0, 0.22],
            movement_pattern="random_walk",
        ),
    ]

    return locations


def main() -> None:
    args = parse_args()

    print(f"Generating LocationFixes messages for {args.duration} seconds...")
    print(f"Output file: {args.path}")
    print(f"Publishing rate: {args.rate} Hz")

    # Create location trackers
    trackers = create_location_trackers()

    # Create channel for LocationFixes messages
    location_fixes_channel = LocationFixesChannel(topic="/location_fixes")

    # Calculate timing
    period = 1.0 / args.rate
    num_messages = int(args.duration * args.rate)

    # Generate messages
    with foxglove.open_mcap(args.path):
        start_time = time.time()
        base_timestamp = int(start_time * 1e9)  # Convert to nanoseconds

        for i in range(num_messages):
            elapsed = i * period
            current_timestamp = base_timestamp + int(elapsed * 1e9)

            # Update all trackers
            for tracker in trackers:
                tracker.update(elapsed)

            # Create LocationFixes message with all location fixes
            fixes = [tracker.to_location_fix(current_timestamp) for tracker in trackers]
            location_fixes_msg = LocationFixes(fixes=fixes)

            # Log the message with the current timestamp
            location_fixes_channel.log(location_fixes_msg, log_time=current_timestamp)

            # Print progress
            if i % int(args.rate) == 0:
                print(f"  Generated {i}/{num_messages} messages ({elapsed:.1f}s)")

    print(f"✅ Successfully wrote {num_messages} LocationFixes messages to {args.path}")
    print("\nLocation fixes generated:")
    for tracker in trackers:
        print(
            f"  - {tracker.id}: lat={tracker.lat:.4f}, lon={tracker.lon:.4f}, pattern={tracker.movement_pattern}"
        )
    print(
        "\nView the recording in Foxglove Studio to see the location fixes moving on the map!"
    )


if __name__ == "__main__":
    main()
