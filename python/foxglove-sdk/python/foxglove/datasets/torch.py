"""Load committed datasets with PyTorch; requires ``foxglove-sdk[torch]``.

These APIs are experimental and unstable and may change in backward-incompatible ways.
"""

from __future__ import annotations

from collections.abc import Iterator, Mapping, Sequence
from random import Random
from typing import TYPE_CHECKING, Any, Literal, TypeVar

import torch.distributed as distributed
from torch import Tensor, as_tensor, int64, tensor
from torch.utils.data import IterableDataset, get_worker_info

from .reader import ClientFactory, ReadEpisode, _Plan, _plan
from .storage import ObjectStoreFactory

if TYPE_CHECKING:
    from .reader import _Episode

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


class TorchDataset(IterableDataset[_T]):
    """An episode iterable created by :func:`read_dataset`.

    Call :meth:`set_epoch` before each new DataLoader pass to select its episode
    order. Epoch updates are shared with spawned and persistent loader workers.
    """

    def __init__(
        self,
        plan: _Plan,
        read_episode: ReadEpisode[_T],
        rank: int,
        world_size: int,
        *,
        shuffle: bool = False,
        seed: int = 0,
    ) -> None:
        self._plan = plan
        self._read_episode = read_episode
        self._rank = rank
        self._world_size = world_size
        self._seed = seed
        self._epoch = (
            tensor(0, dtype=int64, device="cpu").share_memory_() if shuffle else None
        )

    def set_epoch(self, epoch: int) -> None:
        """Select the epoch for the next iteration (initially zero).

        ``epoch`` must be a nonnegative signed 64-bit integer. Set it in every
        rank before starting the next DataLoader iteration, after completing the
        previous pass. Active iterators retain the order they started with.
        """
        if isinstance(epoch, bool) or not isinstance(epoch, int):
            raise TypeError("epoch must be an integer")
        if not 0 <= epoch < 2**63:
            raise ValueError("epoch must be between 0 and 2**63 - 1")
        if self._epoch is not None:
            self._epoch.fill_(epoch)

    def __iter__(self) -> Iterator[_T]:
        episodes: Sequence[_Episode] = self._plan.episodes
        if self._epoch is not None:
            shuffled = list(episodes)
            Random(self._seed + int(self._epoch.item())).shuffle(shuffled)
            episodes = shuffled
        # Partition ranks first, so ranks can use different numbers of loader workers.
        episodes = episodes[self._rank :: self._world_size]
        worker = get_worker_info()
        if worker is not None:
            episodes = episodes[worker.id :: worker.num_workers]
        return self._plan.read(episodes, self._read_episode)


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
    shuffle: bool = False,
    seed: int | None = None,
) -> TorchDataset[_T]:
    """Plan a topic-filtered dataset; download messages only during iteration.

    .. warning::

        This API is experimental and unstable. It may change in backward-incompatible
        ways as we continue development and incorporate user feedback.

    ``read_episode`` yields samples, such as tensors, tuples, or dictionaries.
    Use DataLoader's ``collate_fn`` for custom sample types. Planning fetches
    only the episode list (ids, time windows, and, for object storage, recording
    locations). Each new iteration rereads data. The default client reads
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

    ``shuffle=True`` permutes episode metadata using ``seed + epoch`` before
    partitioning ranks and workers. The same seed, epoch, and committed episode
    list produce the same permutation; all ranks must use the same seed and epoch.
    ``seed`` must be a nonnegative integer and requires ``shuffle=True`` when
    supplied. If omitted or ``None``, shuffling uses seed zero.
    Call :meth:`TorchDataset.set_epoch` between completed passes (initial epoch zero).
    Without shuffling, episodes retain sorted ID order. Samples within each episode
    keep callback order; this is not uniform sample shuffling or full training-run
    reproducibility. Shuffling uses O(number of episodes) metadata references,
    without buffering message payloads.

    Distributed rank and world size are captured here, before DataLoader workers
    start, or can be supplied together explicitly. Episodes can yield unequal
    sample counts: use an uneven-input training policy such as DDP's join context.
    DataLoader shuffling and DistributedSampler do not apply to this iterable.
    """
    if not isinstance(shuffle, bool):
        raise TypeError("shuffle must be a bool")
    if seed is not None:
        if isinstance(seed, bool) or not isinstance(seed, int):
            raise TypeError("seed must be an integer")
        if seed < 0:
            raise ValueError("seed must be nonnegative")
        if not shuffle:
            raise ValueError("seed requires shuffle=True")
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
    return TorchDataset(
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
        shuffle=shuffle,
        seed=0 if seed is None else seed,
    )
