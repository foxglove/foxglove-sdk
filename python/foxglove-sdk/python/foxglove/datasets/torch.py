"""Load committed datasets with PyTorch; requires ``foxglove-sdk[torch]``.

These APIs are experimental and unstable and may change in backward-incompatible ways.
"""

from __future__ import annotations

from collections.abc import Iterator, Mapping, Sequence
from typing import Any, Literal, TypeVar

import torch.distributed as distributed
from torch import Tensor, as_tensor
from torch.utils.data import IterableDataset, get_worker_info

from .reader import ClientFactory, ReadEpisode, _Plan, _plan
from .storage import ObjectStoreFactory

_T = TypeVar("_T")


def to_image_tensor(frame: Mapping[str, Any]) -> Tensor:
    """Convert channels-last images to a contiguous channels-first tensor.

    Accepts a sample or batch whose ``image`` is a NumPy array or Tensor shaped
    ``[..., H, W, 3]``; returns ``[..., 3, H, W]``. Preserves dtype and device.
    Call once, either in the episode callback or after DataLoader. Resizing,
    normalization, and device placement remain up to the training pipeline.
    """
    image = as_tensor(frame["image"])
    if image.ndim < 3 or image.shape[-1] != 3:
        raise ValueError(
            f"Expected a channels-last RGB image [..., H, W, 3], got shape {tuple(image.shape)}"
        )
    return image.movedim(-1, -3).contiguous()


class _TorchDataset(IterableDataset[_T]):
    def __init__(
        self, plan: _Plan, read_episode: ReadEpisode[_T], rank: int, world_size: int
    ) -> None:
        self._plan = plan
        self._read_episode = read_episode
        self._rank = rank
        self._world_size = world_size

    def __iter__(self) -> Iterator[_T]:
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
    read_episode: ReadEpisode[_T],
    source: Literal["foxglove", "object_storage"] = "foxglove",
    client_factory: ClientFactory | None = None,
    object_store_factory: ObjectStoreFactory | None = None,
    rank: int | None = None,
    world_size: int | None = None,
) -> IterableDataset[_T]:
    """Plan a topic-filtered dataset; download messages only during iteration.

    .. warning::

        This API is experimental and unstable. It may change in backward-incompatible
        ways as we continue development and incorporate user feedback.

    ``read_episode`` yields samples, such as tensors, tuples, or dictionaries.
    Use DataLoader's ``collate_fn`` for custom sample types. Only metadata is fetched
    here. Each new iteration rereads data. The default client reads
    ``FOXGLOVE_API_TOKEN`` from the environment; override ``client_factory`` for
    custom authentication or endpoints.

    ``source="object_storage"`` reads indexed MCAPs from S3, GCS, or Azure using recording
    locations and the worker's cloud credentials. No filesystem configuration is
    needed. Missing locations, unsupported schemes, and access failures raise errors
    without falling back to Foxglove downloads. Use DataLoader's ``spawn``
    multiprocessing context with cloud storage.

    ``object_store_factory`` overrides built-in storage and requires
    ``source="object_storage"``. Factories and callbacks used by workers must be
    pickleable. The API client factory is used only during object storage planning
    and is not serialized; in default mode it also runs in each consuming process.

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
        _plan(
            dataset_id,
            version,
            topics,
            client_factory,
            object_store_factory,
            source=source,
        ),
        read_episode,
        resolved_rank,
        resolved_world_size,
    )
