"""Read a committed Foxglove dataset with PyTorch or Ray.

The selected JSON topic contains speed, acceleration, and steering numbers.
Set FOXGLOVE_API_TOKEN on every worker before running this example.
"""

import argparse
import os
from collections.abc import Iterator

import numpy as np
from foxglove.client import Client
from foxglove.datasets import EpisodeReader


def make_client() -> Client:
    """Create a client using credentials available in the current worker."""
    return Client(token=os.environ["FOXGLOVE_API_TOKEN"])


def read_episode(episode: EpisodeReader) -> Iterator[dict[str, np.ndarray]]:
    """Convert selected messages to framework-independent training samples."""
    for _schema, _channel, _message, payload in episode.iter_messages():
        yield {
            "inputs": np.asarray(
                [payload["speed"], payload["acceleration"]], dtype=np.float32
            ),
            "target": np.asarray([payload["steering"]], dtype=np.float32),
        }


def run_torch(dataset_id: str, version: int) -> None:
    """Load batches directly with PyTorch."""
    from foxglove.datasets.torch import read_dataset
    from torch.utils.data import DataLoader

    dataset = read_dataset(
        dataset_id,
        version=version,
        topics=["/training/measurements"],
        read_episode=read_episode,
        client_factory=make_client,
    )
    loader = DataLoader(dataset, batch_size=32, num_workers=2, prefetch_factor=1)
    for batch in loader:
        print(batch["inputs"].shape, batch["target"].shape)


def run_ray(dataset_id: str, version: int) -> None:
    """Load with Ray and consume batches as PyTorch tensors."""
    import ray
    from foxglove.datasets.ray import read_dataset

    ray.init()
    try:
        dataset = read_dataset(
            dataset_id,
            version=version,
            topics=["/training/measurements"],
            read_episode=read_episode,
            client_factory=make_client,
            concurrency=2,
        )
        for batch in dataset.iter_torch_batches(batch_size=32, device="cpu"):
            print(batch["inputs"].shape, batch["target"].shape)
    finally:
        ray.shutdown()


def main() -> None:
    """Select one framework so the data is read through only one path."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dataset-id", required=True)
    parser.add_argument("--version", type=int, required=True)
    parser.add_argument("--framework", choices=["torch", "ray"], default="torch")
    args = parser.parse_args()
    if args.framework == "torch":
        run_torch(args.dataset_id, args.version)
    else:
        run_ray(args.dataset_id, args.version)


if __name__ == "__main__":
    main()
