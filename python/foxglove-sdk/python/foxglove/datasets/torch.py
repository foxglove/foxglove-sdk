"""Load committed datasets with PyTorch; requires ``foxglove-sdk[torch]``."""

from __future__ import annotations

from collections.abc import Iterator, Sequence
from typing import Any

import torch.distributed as distributed
from torch.utils.data import IterableDataset, get_worker_info

from .reader import ClientFactory, ReadEpisode, _Plan, _plan


class _TorchDataset(IterableDataset[dict[str, Any]]):
    def __init__(
        self, plan: _Plan, read_episode: ReadEpisode, rank: int, world_size: int
    ) -> None:
        self._plan = plan
        self._read_episode = read_episode
        self._rank = rank
        self._world_size = world_size

    def __iter__(self) -> Iterator[dict[str, Any]]:
        # Partition ranks first, so ranks can use different numbers of loader workers.
        episodes = self._plan.episodes[self._rank :: self._world_size]
        worker = get_worker_info()
        if worker is not None:
            episodes = episodes[worker.id :: worker.num_workers]
        yield from self._plan.read(episodes, self._read_episode)


def read_dataset(
    dataset_id: str,
    *,
    version: int,
    topics: Sequence[str],
    read_episode: ReadEpisode,
    client_factory: ClientFactory,
    rank: int | None = None,
    world_size: int | None = None,
) -> IterableDataset[dict[str, Any]]:
    """Plan a topic-filtered dataset; download messages only during iteration.

    ``read_episode`` yields sample dictionaries. ``client_factory`` creates a
    Foxglove API client in each consuming process. Both must be pickleable for
    spawned workers. Only metadata is fetched here. Each new iteration rereads data.

    Distributed rank and world size are captured here, before DataLoader workers
    start, or can be supplied together explicitly. Episodes can yield unequal
    sample counts: use an uneven-input training policy such as DDP's join context.
    DataLoader shuffling and DistributedSampler do not apply to this iterable.
    """
    if (rank is None) != (world_size is None):
        raise ValueError("rank and world_size must be provided together")
    if rank is None or world_size is None:
        active = distributed.is_available() and distributed.is_initialized()
        resolved_rank = distributed.get_rank() if active else 0
        resolved_world_size = distributed.get_world_size() if active else 1
    else:
        resolved_rank, resolved_world_size = rank, world_size
    if not 0 <= resolved_rank < resolved_world_size:
        raise ValueError("Require world_size > 0 and 0 <= rank < world_size")
    return _TorchDataset(
        _plan(dataset_id, version, topics, client_factory),
        read_episode,
        resolved_rank,
        resolved_world_size,
    )
