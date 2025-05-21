# This example requires the foxglove-sdk Python package.
import argparse
import math
import struct
import time

import foxglove
from foxglove.channels import RawAudioChannel
from foxglove.schemas import RawAudio, Timestamp

# Parse command-line arguments for output file path
parser = argparse.ArgumentParser()
parser.add_argument("--path", type=str, default="rawaudio.mcap")
args = parser.parse_args()


def generate_sine_wave(
    frequency: float, duration_seconds: float, sample_rate: int
) -> bytes:
    amplitude = 32767  # Max amplitude for 16-bit signed PCM
    total_samples = int(sample_rate * duration_seconds)  # Total number of samples
    # Generate each sample as an integer value for the sine wave
    samples = [
        int(amplitude * math.sin(2 * math.pi * frequency * t / sample_rate))
        for t in range(total_samples)
    ]
    # Pack the samples into a bytes object as little-endian 16-bit signed integers
    return struct.pack("<" + "h" * total_samples, *samples)


def main() -> None:
    # Audio parameters
    sample_rate = 44100  # Samples per second (CD quality)
    duration_seconds = 5.0  # Total duration of the audio in seconds
    frequency = 220.0  # Frequency of the sine wave in Hz (A3 note)
    number_of_channels = 1  # Mono audio
    audio_format = "pcm-s16"  # 16-bit signed PCM, little-endian
    block_size = 1024  # Number of samples per RawAudio message

    # Generate the full list of audio samples for the sine wave
    amplitude = 32767  # Maximum amplitude for 16-bit audio
    total_samples = int(sample_rate * duration_seconds)
    samples = [
        int(amplitude * math.sin(2 * math.pi * frequency * t / sample_rate))
        for t in range(total_samples)
    ]

    # Open the MCAP file for writing
    with foxglove.open_mcap(args.path):
        # Create a channel for RawAudio messages
        audio_channel = RawAudioChannel(topic="/audio")
        # Record the wall-clock start time (seconds since epoch)
        start_time = time.time()
        # Loop over the samples in blocks of block_size
        for block_start in range(0, total_samples, block_size):
            block_end = min(block_start + block_size, total_samples)
            # Extract the current block of samples
            block_samples = samples[block_start:block_end]
            # Pack the samples as little-endian 16-bit signed integers
            audio_data = struct.pack("<" + "h" * len(block_samples), *block_samples)
            # Calculate the timestamp for the start of this block (in seconds)
            time_seconds = start_time + (block_start / sample_rate)
            # Create a Foxglove Timestamp object for this block
            timestamp = Timestamp.from_epoch_secs(time_seconds)
            # Write the RawAudio message to the MCAP file
            audio_channel.log(
                RawAudio(
                    data=audio_data,
                    format=audio_format,
                    sample_rate=sample_rate,
                    number_of_channels=number_of_channels,
                    timestamp=timestamp,
                ),
                # log_time is the timestamp in nanoseconds as a uint64
                log_time=int(time_seconds * 1_000_000_000),
            )


if __name__ == "__main__":
    main()
