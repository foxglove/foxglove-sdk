# Foxglove SDK MP3 to RawAudio MCAP Example

This example demonstrates converting an MP3 file to a Foxglove MCAP file containing RawAudio messages.

## Usage

This example uses Poetry: https://python-poetry.org/

```bash
poetry install
poetry run python main.py --input path/to/input.mp3 --output path/to/output.mcap
```

- `--input`: Path to the input MP3 file (required)
- `--output`: Path to the output MCAP file (default: `rawaudio.mcap`)
