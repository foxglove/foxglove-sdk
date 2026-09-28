# Dataset training input

Run either the PyTorch or Ray adapter against a committed dataset version. The
example expects JSON messages on `/training/measurements` with numeric `speed`,
`acceleration`, and `steering` fields. Adapt `read_episode` for your own schemas.

```sh
export FOXGLOVE_API_TOKEN=your-token
uv run main.py --dataset-id ds_123 --version 7 --framework torch
uv run main.py --dataset-id ds_123 --version 7 --framework ray
```

The script prints batch shapes where your model's training step would go. Topic
filtering happens on the server. Factories and callbacks run in workers, which
need the token and Python dependencies. The example uses the local SDK and
foxglove-client 0.20.0 or newer from PyPI.

No cache is maintained; running both commands reads the selected data twice.
PyTorch callbacks may yield any sample shape supported by `DataLoader`; Ray
callbacks must yield dictionaries with Arrow-compatible columns. This example uses
dictionaries so the same callback works with both frameworks.

For Protobuf or ROS messages, install the matching `mcap-protobuf-support`,
`mcap-ros1-support`, or `mcap-ros2-support` package on every worker; the client
automatically adds installed packages to its default decoders. To select custom
decoders explicitly, construct them inside `read_episode` and pass them to
`episode.iter_messages(decoder_factories=...)`. An explicit list replaces the
defaults. This deserializes MCAP messages; decoding media payloads such as H.264 into
frames is a separate step and is not part of this example.
