"""Read a committed Foxglove dataset with PyTorch or Ray.

The selected JSON topic contains speed, acceleration, and steering numbers.
Set FOXGLOVE_API_TOKEN before running. Object storage mode uses cloud credentials
on workers; the default Foxglove mode also needs FOXGLOVE_API_TOKEN on workers.
"""

import argparse
from collections.abc import Iterator
from typing import Literal

import numpy as np
from foxglove.datasets import EpisodeReader


def read_episode(episode: EpisodeReader) -> Iterator[dict[str, np.ndarray]]:
    """Convert selected messages to framework-independent training samples."""
    for _schema, _channel, _message, payload in episode.iter_messages():
        yield {
            "inputs": np.asarray(
                [payload["speed"], payload["acceleration"]], dtype=np.float32
            ),
            "target": np.asarray([payload["steering"]], dtype=np.float32),
        }


def run_torch(
    dataset_id: str, version: int, source: Literal["foxglove", "object_storage"]
) -> None:
    """Load batches directly with PyTorch."""
    from foxglove.datasets.torch import read_dataset
    from torch.utils.data import DataLoader

    dataset = read_dataset(
        dataset_id,
        version=version,
        topics=["/training/measurements"],
        read_episode=read_episode,
        source=source,
    )
    loader = DataLoader(
        dataset,
        batch_size=32,
        num_workers=2,
        prefetch_factor=1,
        multiprocessing_context="spawn",
    )
    for batch in loader:
        print(batch["inputs"].shape, batch["target"].shape)


def run_ray(
    dataset_id: str, version: int, source: Literal["foxglove", "object_storage"]
) -> None:
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
            source=source,
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
    parser.add_argument(
        "--source", choices=["foxglove", "object_storage"], default="foxglove"
    )
    args = parser.parse_args()
    if args.framework == "torch":
        run_torch(args.dataset_id, args.version, args.source)
    else:
        run_ray(args.dataset_id, args.version, args.source)


if __name__ == "__main__":
    main()
