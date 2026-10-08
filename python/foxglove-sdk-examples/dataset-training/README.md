# Dataset training input

Run either the PyTorch or Ray adapter on a dataset version. The example expects JSON messages on
`/training/measurements` with numeric `speed`, `acceleration`, and `steering` fields.

Adapt `read_episode` for your own schemas. Install `foxglove-sdk[torch]` for PyTorch
or `foxglove-sdk[torch,ray]` for this example's Ray-to-PyTorch batches. These extras
include the object storage dependencies.

```sh
export FOXGLOVE_API_TOKEN=your-token
uv run main.py --dataset-id ds_123 --version 7 --framework torch
uv run main.py --dataset-id ds_123 --version 7 --framework ray
```

The default `--source foxglove` downloads through Foxglove. To read original MCAP
objects from customer-managed indexed storage, use `--source direct` with either
framework:

```sh
uv run main.py --dataset-id ds_123 --version 7 --framework torch --source direct
```

Direct reads require the API and client to provide recording locations with
`location.scheme` (`s3`, `gs`, or `az`). The SDK selects the cloud filesystem,
discovers and caches S3 bucket regions, and uses the Azure account from the
location. Configure cloud credentials on every worker and grant read access to
the original objects. The Foxglove API token provides dataset metadata access;
cloud credentials provide object access. Direct reads require immutable,
chunk-indexed MCAP recordings with summary indexes and raise an error if they
cannot read the objects.
