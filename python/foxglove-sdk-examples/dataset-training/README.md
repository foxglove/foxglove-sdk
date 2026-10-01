# Dataset training input

Run either the PyTorch or Ray adapter on a dataset version. The example expects JSON messages on
`/training/measurements` with numeric `speed`, `acceleration`, and `steering` fields.

Adapt `read_episode` for your own schemas.

```sh
export FOXGLOVE_API_TOKEN=your-token
uv run main.py --dataset-id ds_123 --version 7 --framework torch
uv run main.py --dataset-id ds_123 --version 7 --framework ray
```
