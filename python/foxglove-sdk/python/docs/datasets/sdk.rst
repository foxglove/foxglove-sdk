SDK reference
=============

``foxglove.datasets`` streams a committed dataset version into PyTorch or Ray
Data. ``foxglove-sdk>=0.31``, extras ``[torch]``, ``[ray]``, ``[video]``.
``foxglove-client>=0.23``.

Default client reads only ``FOXGLOVE_API_TOKEN`` and builds
``Client(token=...)``. Raises ``ValueError``
(``Set FOXGLOVE_API_TOKEN or provide client_factory``) if it is unset or
empty. Pass ``client_factory`` for another host or token.

JSON is decoded by default. Protobuf and ROS decode when
``mcap-protobuf-support``, ``mcap-ros1-support``, or ``mcap-ros2-support``
are installed on every worker.

During ``read_dataset`` iteration, callback and stream errors are raised as
``RuntimeError("Failed to read episode…")`` with the original as
``__cause__``. Ray wraps that as ``RayTaskError``.

foxglove.datasets.torch.read_dataset
------------------------------------

Return a ``TorchDataset`` over a committed version. Planning fetches the
episode list; messages stream during iteration. Keyword-only after
``dataset_id``.

Episodes are sorted by ID. When ``shuffle=True``, they are permuted with
``random.Random(seed + epoch)``, then sliced ``[rank::world_size]`` and
``[worker::num_workers]``. ``rank``/``world_size`` default to
``torch.distributed`` when it is initialized at call time, else 0 and 1.
Call ``read_dataset`` after ``init_process_group``.
Messages are returned in log-time order across the selected topics.

Parameters:

* ``dataset_id`` (str, required) — Dataset ID.
* ``version`` (int, keyword-only, required) — Committed version number.
* ``topics`` (list of str, keyword-only, required) — Topics to stream.
* ``read_episode`` (callable, keyword-only, required) — Convert an ``EpisodeReader`` into samples. Must be pickleable.
* ``source`` (str, keyword-only, optional, default ``foxglove``) — ``foxglove`` or ``object_storage``. With ``object_storage``, set DataLoader ``multiprocessing_context="spawn"``.
* ``split_config`` (DatasetSplit or None, keyword-only, optional, default None) — Pass with ``split``.
* ``split`` (str or None, keyword-only, optional, default None) — ``train``, ``validation``, or ``test``.
* ``include_properties`` (bool, keyword-only, optional, default False) — Load pinned custom properties.
* ``client_factory`` (callable or None, keyword-only, optional, default None) — Build a client. Must be pickleable.
* ``object_store_factory`` (callable or None, keyword-only, optional, default None) — Custom store; requires ``source="object_storage"``. Must be pickleable.
* ``rank`` (int or None, keyword-only, optional, default None) — Process rank; pass with ``world_size``.
* ``world_size`` (int or None, keyword-only, optional, default None) — Number of ranks.
* ``shuffle`` (bool, keyword-only, optional, default False) — Permute episodes before rank and worker partitioning.
* ``seed`` (int or None, keyword-only, optional, default None) — Nonnegative shuffle seed. With ``shuffle=True``, omitting ``seed`` uses 0. Every rank must pass the same ``seed``.

Returns: ``TorchDataset``.

Raises: ``TypeError`` if ``shuffle`` is not a bool or ``seed`` is not an
int. ``ValueError`` if the version is uncommitted, recordings are missing,
``split_config``/``split`` are not passed together, ``rank``/``world_size``
are not passed together, ``seed`` is negative, ``seed`` is set without
``shuffle=True``, ``rank`` is out of range, or ``groups`` does not cover
exactly the committed version's episodes. During iteration, read errors
are ``RuntimeError("Failed to read episode…")`` with the original as
``__cause__``.

.. code-block:: python

   from foxglove.datasets.torch import read_dataset

   def read_episode(episode):
       for schema, channel, message, payload in episode.iter_messages():
           yield {"log_time": message.log_time}

   ds = read_dataset(
       "dset_123", version=2, topics=["/camera"], read_episode=read_episode
   )

foxglove.datasets.torch.TorchDataset
------------------------------------

Iterable dataset created by ``torch.read_dataset``. Subclass of
``torch.utils.data.IterableDataset``.

.. code-block:: python

   from torch.utils.data import DataLoader

   loader = DataLoader(ds, batch_size=8, shuffle=False)

set_epoch
^^^^^^^^^

Select the episode order for the next DataLoader pass. No effect unless
``shuffle=True``. The epoch tensor is shared with spawned and persistent
loader workers. Call on every rank after the previous pass completes, with
the same epoch. Active iterators keep the order they started with.
Shuffling happens before rank and worker partitioning. Ranks can yield
different sample counts.

Parameters:

* ``epoch`` (int, required) — Nonnegative signed 64-bit integer. Every rank
  must use the same value.

Returns: ``None``.

Raises: ``TypeError`` if ``epoch`` is not an int; ``ValueError`` if it is
outside ``0 .. 2**63 - 1``.

.. code-block:: python

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

foxglove.datasets.torch.to_image_tensor
---------------------------------------

Convert channels-last RGB images to a contiguous channels-first tensor.
Preserves dtype and device.

Parameters:

* ``frame`` (mapping, required) — Sample or batch whose ``image`` is ``[..., H, W, 3]``.

Returns: tensor ``[..., 3, H, W]``.

Raises: ``ValueError`` if ``image`` is not channels-last RGB.

.. code-block:: python

   import torch
   from foxglove.datasets.torch import to_image_tensor

   batch = {"image": torch.zeros(8, 4, 8, 3)}
   images = to_image_tensor(batch)

foxglove.datasets.ray.read_dataset
----------------------------------

Return a Ray Data dataset of sample mappings. ``read_episode`` must yield
mappings with consistent column types. Factories and callbacks must be
serializable. Ray task retries rerun callbacks. Keyword-only after
``dataset_id``.

Parameters:

* ``dataset_id`` (str, required) — Dataset ID.
* ``version`` (int, keyword-only, required) — Committed version number.
* ``topics`` (list of str, keyword-only, required) — Topics to stream.
* ``read_episode`` (callable, keyword-only, required) — Convert an ``EpisodeReader`` into sample mappings. Must be pickleable.
* ``source`` (str, keyword-only, optional, default ``foxglove``) — ``foxglove`` or ``object_storage``.
* ``split_config`` (DatasetSplit or None, keyword-only, optional, default None) — Pass with ``split``.
* ``split`` (str or None, keyword-only, optional, default None) — ``train``, ``validation``, or ``test``.
* ``include_properties`` (bool, keyword-only, optional, default False) — Load pinned custom properties.
* ``client_factory`` (callable or None, keyword-only, optional, default None) — Build a client. Must be pickleable.
* ``object_store_factory`` (callable or None, keyword-only, optional, default None) — Custom store; requires ``source="object_storage"``. Must be pickleable.
* ``concurrency`` (int or None, keyword-only, optional, default None) — Cap on concurrent read tasks.

Returns: ``ray.data.Dataset``.

Raises: ``ValueError`` if the version is uncommitted, recordings are
missing, ``split_config``/``split`` are not passed together, or
``groups`` does not cover exactly the committed version's episodes.
During iteration, a non-mapping from ``read_episode`` raises
``TypeError``; that and other read errors become
``RuntimeError("Failed to read episode…")`` with the original as
``__cause__``, then ``RayTaskError``.

.. code-block:: python

   from foxglove.datasets.ray import read_dataset

   def read_episode(episode):
       for schema, channel, message, payload in episode.iter_messages():
           yield {"log_time": message.log_time}

   ds = read_dataset(
       "dset_123", version=2, topics=["/camera"], read_episode=read_episode
   )

EpisodeReader
-------------

An episode scoped to the topics selected by ``read_dataset``. Passed into
``read_episode``. Usable only until that callback's iterator finishes.

Attributes:

* ``id`` (str) — Episode ID.
* ``start_time`` (datetime) — Start of the episode window.
* ``end_time`` (datetime) — End of the episode window.
* ``properties`` (mapping, read-only) — Pinned custom properties. Requires
  ``include_properties=True``. Empty if the episode has none.

Raises: ``RuntimeError`` if properties were not requested. From
``read_episode``, that becomes ``RuntimeError("Failed to read episode…")``
with the original as ``__cause__``.

.. code-block:: python

   print(episode.id, episode.start_time, episode.end_time)

iter_messages
^^^^^^^^^^^^^

Stream the selected topics. All parameters are keyword-only. Messages
are returned in log-time order across the selected topics.

Parameters:

* ``decoder_factories`` (list of DecoderFactory or None, keyword-only, optional, default None) — MCAP decoders; ``None`` uses the client's defaults.
* ``topics`` (list of str or None, keyword-only, optional, default None) — Nonempty subset of the dataset topics.
* ``lookback`` (timedelta, keyword-only, optional, default 0) — Extra history before the episode start. When you yield samples, keep those whose ``message.log_time`` (Unix nanoseconds) is at or after the episode start. ``decode_h264`` already does this.

Returns: tuples of schema (possibly ``None``), channel, raw MCAP message, and
decoded payload. ``message.log_time`` is Unix nanoseconds.

Raises: ``RuntimeError`` if the reader is closed; ``ValueError`` if
``lookback`` is negative or ``topics`` is empty or outside the dataset
topics. From ``read_episode``, those become
``RuntimeError("Failed to read episode…")`` with the original as
``__cause__``.

.. code-block:: python

   def read_episode(episode):
       for schema, channel, message, payload in episode.iter_messages():
           yield {"topic": channel.topic, "log_time": message.log_time}

DatasetSplit
------------

Assign whole recording groups to train, validation, and test. Proportions
must be finite, nonnegative, and sum to 1. Episodes that share a recording,
or the same ``groups`` ID, go to the same split. ``split_config`` and
``split`` must be passed together.

Parameters:

* ``train`` (float, optional, default 0.8) — Train proportion.
* ``validation`` (float, optional, default 0.1) — Validation proportion.
* ``test`` (float, optional, default 0.1) — Test proportion.
* ``seed`` (int, optional, default 0) — Split assignment seed, independent of epoch shuffling.
* ``groups`` (mapping or sequence or None, optional, default None) — Mapping or ``(episode_id, group_id)`` pairs. Stored as a sorted tuple. Episode IDs must be unique. The mapping covers exactly the version's episodes. Group IDs may repeat.

Returns: a ``DatasetSplit``.

Raises: ``ValueError`` if proportions are invalid, episode IDs are
duplicated, or IDs are not nonempty strings. ``read_dataset`` and
``inspect_splits`` raise ``ValueError`` if ``groups`` does not cover
exactly the committed version's episodes.

.. code-block:: python

   from foxglove.datasets import DatasetSplit

   split = DatasetSplit(train=0.8, validation=0.1, test=0.1, seed=0)

inspect_splits
--------------

Return train, validation, and test sizes from episode metadata. Same
assignment as the PyTorch and Ray readers. Counts cover the whole committed
version, before rank or worker partitioning. Keyword-only after
``dataset_id``.

Parameters:

* ``dataset_id`` (str, required) — Dataset ID.
* ``version`` (int, keyword-only, required) — Committed version number.
* ``split_config`` (DatasetSplit, keyword-only, required) — Split configuration.
* ``client_factory`` (callable or None, keyword-only, optional, default None) — Build a client.

Returns: ``dict`` of split name to ``SplitSummary``.

Raises: ``ValueError`` if the version is uncommitted, recordings are missing,
``split_config`` is not a ``DatasetSplit``, or ``groups`` does not cover
exactly the committed version's episodes.

.. code-block:: python

   from foxglove.datasets import DatasetSplit, inspect_splits

   summaries = inspect_splits("dset_123", version=2, split_config=DatasetSplit())

SplitSummary
------------

Group and episode counts for one split, before rank or worker partitioning.

Fields:

* ``group_count`` (int) — Connected recording groups in this split.
* ``episode_count`` (int) — Episodes in those groups.

.. code-block:: python

   print(summaries["train"].episode_count, summaries["train"].group_count)

ObjectLocation
--------------

Location of an original recording in customer-managed storage. ``path`` is
the full object key. Built-in schemes: ``s3``, ``gs``, ``az``.

Parameters:

* ``bucket`` (str, required) — Bucket or Azure container.
* ``path`` (str, required) — Full object key.
* ``azure_storage_account_name`` (str or None, optional, default None) — Azure account; required for ``scheme="az"``.
* ``scheme`` (str or None, optional, default None) — ``s3``, ``gs``, or ``az`` for built-in storage. Custom stores can accept other values or ``None``.

Returns: an ``ObjectLocation``.

.. code-block:: python

   from foxglove.datasets import ObjectLocation

   loc = ObjectLocation(bucket="my-bucket", path="run/file.mcap", scheme="s3")

ObjectStore
-----------

Protocol for worker-local storage access. ``source="object_storage"`` uses
the built-in store for ``s3``, ``gs``, and ``az`` (Azure also needs
``azure_storage_account_name``) with each worker's cloud credentials.
The reader reads objects directly from your bucket. Keep objects unchanged
while a read runs. Pass ``object_store_factory`` for another store
(requires ``source="object_storage"``).

open
^^^^

Open a seekable binary file. The SDK owns and closes the handle.

Parameters:

* ``location`` (ObjectLocation, required) — Recording object to open.

Returns: a seekable binary file supporting ``read``, ``readinto``, and ``seek``.

.. code-block:: python

   class LocalStore:
       def open(self, location):
           return open(f"/mnt/{location.bucket}/{location.path}", "rb")

decode_h264
-----------

Yield RGB frames from ``foxglove.CompressedVideo`` H.264 Annex B. Requires
``foxglove-sdk[video]``. Usable as ``read_episode`` or inside a callback.
Keyword-only after ``episode``. Frames are yielded lazily. Each channel
has its own decoder.

Parameters:

* ``episode`` (EpisodeReader, required) — Episode whose window bounds output frames.
* ``topic`` (str or None, keyword-only, optional, default None) — One selected topic, or all selected topics.
* ``lookback`` (timedelta, keyword-only, optional, default 5 seconds) — History budget before the episode start.
* ``decoder_factories`` (list of DecoderFactory or None, keyword-only, optional, default None) — MCAP deserializers; ``None`` uses the client's defaults.

Returns: ``VideoSample`` dicts, ordered per channel.

Raises: ``VideoDecodeError``. From ``read_dataset``, that becomes
``RuntimeError("Failed to read episode…")`` with the original as
``__cause__``. Ray wraps that as ``RayTaskError``.

.. code-block:: python

   from foxglove.datasets.video import decode_h264
   from foxglove.datasets.torch import read_dataset

   ds = read_dataset(
       "dset_123", version=2, topics=["/camera"], read_episode=decode_h264
   )

VideoSample
-----------

RGB image and metadata of its original compressed message.

Fields:

* ``image`` (ndarray) — Writable contiguous uint8 ``[H, W, 3]``.
* ``topic`` (str) — Source topic.
* ``channel_id`` (int) — MCAP channel ID.
* ``timestamp_ns`` (int) — Capture timestamp, Unix nanoseconds.
* ``log_time_ns`` (int) — Log time, Unix nanoseconds; determines episode membership.
* ``publish_time_ns`` (int) — Publish time, Unix nanoseconds.
* ``frame_id`` (str) — Camera coordinate frame.

.. code-block:: python

   height, width, _ = sample["image"].shape

VideoDecodeError
----------------

Raised when video data cannot be decoded. Subclass of ``ValueError``.

.. code-block:: python

   from foxglove.datasets.video import VideoDecodeError, decode_h264

   try:
       next(decode_h264(episode))
   except VideoDecodeError:
       pass
