# Requires: foxglove-sdk, mcap, pydub
import argparse
import time

from pydub import AudioSegment
import foxglove
from foxglove.channels import RawAudioChannel
from foxglove.schemas import RawAudio, Timestamp

# Parse command-line arguments
parser = argparse.ArgumentParser()
parser.add_argument("--input", type=str, required=True, help="Path to input MP3 file")
parser.add_argument(
    "--output", type=str, default="rawaudio.mcap", help="Path to output MCAP file"
)
args = parser.parse_args()


def main() -> None:
    # Load MP3 and decode to PCM
    audio = AudioSegment.from_mp3(args.input)
    sample_rate = audio.frame_rate
    channels = audio.channels
    sample_width = audio.sample_width  # in bytes
    audio_format = "pcm-s16"  # Only 16-bit signed PCM supported
    block_size = 1024  # Number of samples per RawAudio message (per channel)

    if sample_width != 2:
        raise ValueError(
            f"Only 16-bit PCM supported, got sample width {sample_width * 8} bits"
        )

    # Calculate total number of samples (per channel)
    total_samples = len(audio.raw_data) // (sample_width * channels)

    # Open the MCAP file for writing
    with foxglove.open_mcap(args.output):
        audio_channel = RawAudioChannel(topic="/audio")
        start_time = time.time()
        # Write audio in blocks
        for block_start in range(0, total_samples, block_size):
            block_end = min(block_start + block_size, total_samples)
            # Calculate byte offsets
            byte_start = block_start * sample_width * channels
            byte_end = block_end * sample_width * channels
            block_data = audio.raw_data[byte_start:byte_end]
            # Calculate timestamp for this block
            time_seconds = start_time + (block_start / sample_rate)
            timestamp = Timestamp.from_epoch_secs(time_seconds)
            audio_channel.log(
                RawAudio(
                    data=block_data,
                    format=audio_format,
                    sample_rate=sample_rate,
                    number_of_channels=channels,
                    timestamp=timestamp,
                ),
                log_time=int(time_seconds * 1_000_000_000),
            )


if __name__ == "__main__":
    main()
