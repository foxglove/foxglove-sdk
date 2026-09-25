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
