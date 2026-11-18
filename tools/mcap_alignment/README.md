# Align an MCAP recording

Sample camera and telemetry topics on a common timeline and write an indexed MCAP. The output log time is the sample time. Each selected image is independently decodable, and the provenance topic identifies its source message.

## Run the four-camera example

From the repository root on Apple Silicon macOS, with Rust, Xcode Command Line Tools, and uv installed:

```sh
uv venv --python 3.14
uvx maturin build --release -m python/foxglove-sdk/Cargo.toml -i .venv/bin/python -o .venv/wheels
uv pip install .venv/wheels/*.whl 'foxglove-sdk[alignment]'
.venv/bin/python -m foxglove.experimental.mcap_alignment run tools/mcap_alignment/example/job.json
```

Open `tools/mcap_alignment/example/aligned.mcap` in Foxglove. The four `/aligned/camera/*` topics have five images each at 10 Hz. The JSON report should show 5 targets, 20 selections, and 0 missing slots. To rerun, choose a fresh destination in the job file.

## Use your own recording

Copy the [example job](example/job.json) and set the input and output paths, episode start and exclusive end in MCAP log-time nanoseconds, and the topics to sample. Paths resolve relative to the job file. For a recording with Protobuf `foxglove.CompressedVideo` camera channels, the helper can generate a starting job:

```sh
.venv/bin/python tools/mcap_alignment/examples/configure.py input.mcap aligned.mcap job.json
.venv/bin/python -m foxglove.experimental.mcap_alignment run job.json
```

Review the generated job before running the second command. The helper selects the previous camera frame at each tick with a one-second tolerance. Add `--telemetry /topic` for each independently copyable state or command stream. Use `--max-width 640 --max-height 400` for aspect-preserving resizing, and check any inferred calibration bindings and pixel convention.

- Choose a fixed rate or a reference topic. Each stream supports previous, nearest, or exact matching within a finite tolerance. A missing sample fails the job.
- Supported video includes Annex B H.264/H.265 with 8-bit YUV420p and no B pictures. Full-resolution PNG is the default output. Allow extra disk space for independent frames.
- Supply one local indexed input and a fresh output path. The message index is held in memory, with a default budget of 16 MiB. For larger recordings, increase `limits.index_memory_bytes` in the job file.
