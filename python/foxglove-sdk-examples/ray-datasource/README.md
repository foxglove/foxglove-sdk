# Ray Data datasource

An example from the Foxglove SDK demonstrating how to load Foxglove dataset episodes as a
[Ray Data](https://docs.ray.io/en/latest/data/data.html) dataset.

`FoxgloveDataset` downloads every episode in a Foxglove dataset in parallel across the Ray
cluster, emitting one row of raw MCAP bytes per episode. Decoding the bytes into
domain-specific samples is left to a downstream Ray Data transform, so the datasource is
reusable across projects.

## Install dependencies

This example uses [uv](https://docs.astral.sh/uv/). The `ray` extra of the SDK pulls in
`ray[data]`, `pandas`, and `requests`.

```bash
uv sync
```

## Run

You need a Foxglove API token and the id of a dataset to load.

```bash
export FOXGLOVE_API_TOKEN="fox_sk_..."
uv run main.py --dataset-id ds_...
```

This downloads each episode in the dataset and prints the number of messages it contains.
