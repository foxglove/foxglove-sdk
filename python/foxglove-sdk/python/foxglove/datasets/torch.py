"""Load committed datasets with PyTorch; requires ``foxglove-sdk[torch]``."""

from __future__ import annotations

from collections.abc import Iterator, Mapping, Sequence
from random import Random
from typing import TYPE_CHECKING, Any, Literal, TypeVar

import torch.distributed as distributed
from torch import Tensor, as_tensor, int64, tensor
from torch.utils.data import IterableDataset, get_worker_info

from .reader import ClientFactory, ReadEpisode, _Plan, _plan
from .splits import DatasetSplit, SplitName
from .storage import ObjectStoreFactory

if TYPE_CHECKING:
    from .reader import _Episode

_T = TypeVar("_T")


def to_image_tensor(frame: Mapping[str, Any]) -> Tensor:
    """Convert channels-last images to a contiguous channels-first tensor.

    :param frame: Required. A sample or batch whose ``image`` is a NumPy array or
        Tensor shaped ``[..., H, W, 3]``.
    :returns: The image shaped ``[..., 3, H, W]``. Preserves dtype and device.
    :raises KeyError: If ``frame`` has no ``"image"`` entry.
    :raises ValueError: If ``image`` is not shaped ``[..., H, W, 3]``.

    Example::

        import torch
        from foxglove.datasets.torch import to_image_tensor

        batch = {"image": torch.zeros(8, 4, 8, 3)}
        images = to_image_tensor(batch)
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

        Set it in every rank before starting the next DataLoader iteration, after
        completing the previous pass. Active iterators retain the order they
        started with. For a dataset created with ``shuffle=True``, the epoch
        selects the episode order.

        :param epoch: Required. A nonnegative signed 64-bit integer.
        :raises TypeError: If ``epoch`` is not an integer.
        :raises ValueError: If ``epoch`` is negative or greater than ``2**63 - 1``.

        Example::

            from foxglove.datasets.torch import read_dataset

            def read_episode(episode):
                yield {"id": episode.id}

            ds = read_dataset(
                "dset_123",
                version=2,
                topics=["/camera"],
                read_episode=read_episode,
                shuffle=True,
            )
            ds.set_epoch(1)
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
    split_config: DatasetSplit | None = None,
    split: SplitName | None = None,
    include_properties: bool = False,
    client_factory: ClientFactory | None = None,
    object_store_factory: ObjectStoreFactory | None = None,
    rank: int | None = None,
    world_size: int | None = None,
    shuffle: bool = False,
    seed: int | None = None,
) -> TorchDataset[_T]:
    """Return a :class:`TorchDataset` over a committed dataset version.

    Planning fetches only the episode list (ids, time windows, and, for object
    storage, recording locations). Messages are downloaded during iteration, and
    each new iteration rereads data. Factories and callbacks used by workers must
    be pickleable.

    :param dataset_id: Required. ID of the dataset.
    :param version: Required. Number of a committed dataset version.
    :param topics: Required. Topics to stream. Must be nonempty.
    :param read_episode: Required. Callback that receives each
        :class:`~foxglove.datasets.EpisodeReader` and yields samples, such as
        tensors, tuples, or dictionaries. Use DataLoader's ``collate_fn`` for
        custom sample types.
    :param source: Optional. ``"foxglove"`` downloads messages through Foxglove.
        ``"object_storage"`` reads indexed MCAPs from S3, GCS, or Azure using
        recording locations and the worker's cloud credentials. No filesystem
        configuration is needed. Each MCAP file must have a summary. A file needs
        chunk indexes unless its summary reports zero messages, and every chunk
        that contains messages needs message indexes. Missing locations,
        unsupported schemes, and access failures raise errors. Use DataLoader's
        ``spawn`` multiprocessing context with cloud storage.
    :param split_config: Optional. Together with ``split``, selects a
        recording-safe train, validation, or test pool before worker partitioning.
        See :class:`foxglove.datasets.DatasetSplit`.
    :param split: Optional. The pool to read: ``"train"``, ``"validation"``, or
        ``"test"``. Requires ``split_config``.
    :param include_properties: Optional. ``True`` fetches each episode's
        properties from its pinned version for ``EpisodeReader.properties``. They
        are not fetched by default.
    :param client_factory: Optional. Callable that returns the API client, for
        custom authentication or endpoints. The default client reads
        ``FOXGLOVE_API_TOKEN`` from the environment. With
        ``source="object_storage"``, the factory runs only during planning and is
        not serialized. With ``source="foxglove"``, it also runs in each consuming
        process, so it must be pickleable.
    :param object_store_factory: Optional. Callable that returns an
        :class:`~foxglove.datasets.ObjectStore`, overriding built-in storage.
        Requires ``source="object_storage"``.
    :param rank: Optional. Distributed rank of this process. Supply together with
        ``world_size``. If both are omitted, they are captured here from
        ``torch.distributed``, before DataLoader workers start. If
        ``torch.distributed`` is not initialized, rank 0 and world size 1 are used.
    :param world_size: Optional. Number of distributed ranks. Episodes are divided
        among ranks, then among each rank's DataLoader workers, so each episode is
        read once per pass. ``read_dataset`` already splits episodes by rank, so
        do not add a ``DistributedSampler``. Ranks can get different numbers of
        batches, because each rank reads whole episodes and episodes differ in
        length. Before each optimizer step, call ``torch.distributed.all_reduce``
        with ``ReduceOp.MIN`` on a tensor that is 1 if the rank has a batch and 0
        if it does not, and stop the epoch on every rank when the result is 0.
        Every rank then takes the same number of optimizer steps.
    :param shuffle: Optional. ``True`` permutes episode metadata using
        ``seed + epoch`` before partitioning ranks and workers. The same seed,
        epoch, and committed episode list produce the same permutation; all ranks
        must use the same seed and epoch. Call :meth:`TorchDataset.set_epoch`
        between completed passes (initial epoch zero). Without shuffling, episodes
        retain sorted ID order. Samples within each episode keep callback order.
        Shuffling uses O(number of episodes) metadata references, without
        buffering message payloads. The dataset orders and partitions episodes
        itself, so leave DataLoader's ``shuffle`` and ``sampler`` options unset.
    :param seed: Optional. Shuffle seed. Must be a nonnegative integer and requires
        ``shuffle=True`` when supplied. If omitted or ``None``, shuffling uses seed
        zero.
    :returns: An iterable dataset of the samples that ``read_episode`` yields.
    :raises TypeError: If ``shuffle`` is not a bool or ``seed`` is not an integer.
    :raises ValueError: If an argument is invalid, ``FOXGLOVE_API_TOKEN`` is not
        set and no ``client_factory`` is given, the version is not committed or
        contains missing recordings, ``split_config.groups`` does not cover exactly
        the version's episodes, an episode has no recording IDs when
        ``split_config`` is given, or, for object storage, a recording has no
        supported location.
    :raises RuntimeError: While iterating, if ``read_episode`` or a message stream
        raises. The message names the episode, dataset, and version, and the
        original exception is its ``__cause__``. DataLoader worker processes
        report the error to the main process as a new ``RuntimeError`` whose
        message contains the original traceback.

    Example::

        from foxglove.datasets.torch import read_dataset

        def read_episode(episode):
            for schema, channel, message, payload in episode.iter_messages():
                yield {"log_time": message.log_time}

        ds = read_dataset(
            "dset_123",
            version=2,
            topics=["/camera"],
            read_episode=read_episode,
        )
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
            split_config=split_config,
            split=split,
            include_properties=include_properties,
        ),
        read_episode,
        resolved_rank,
        resolved_world_size,
        shuffle=shuffle,
        seed=0 if seed is None else seed,
    )
