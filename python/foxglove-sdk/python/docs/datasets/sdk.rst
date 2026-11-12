SDK reference
=============

``foxglove.datasets`` streams a committed dataset version into PyTorch or Ray
Data. ``foxglove-sdk>=0.31``, extras ``[torch]``, ``[ray]``, ``[video]``.
``foxglove-client>=0.23``.

Default client: ``Client(token=os.environ["FOXGLOVE_API_TOKEN"])``.
Raises ``ValueError`` if the variable is unset. Pass ``client_factory`` for
another host or token.

JSON is decoded by default. Protobuf and ROS decode when
``mcap-protobuf-support``, ``mcap-ros1-support``, or ``mcap-ros2-support``
are installed on every worker.

foxglove.datasets.torch.read_dataset
------------------------------------

Return a ``TorchDataset`` over a committed version. Planning fetches the
episode list; messages stream during iteration.

Parameters:

* ``dataset_id`` (str, required) — Dataset ID.
* ``version`` (int, required) — Committed version number.
* ``topics`` (list of str, required) — Topics to stream.
* ``read_episode`` (callable, required) — Convert an ``EpisodeReader`` into samples.
* ``source`` (str, optional, default ``foxglove``) — ``foxglove`` or ``object_storage``.
* ``split_config`` (DatasetSplit or None, optional, default None) — Pass with ``split``.
* ``split`` (str or None, optional, default None) — ``train``, ``validation``, or ``test``.
* ``include_properties`` (bool, optional, default False) — Load pinned custom properties.
* ``client_factory`` (callable or None, optional, default None) — Build a client.
* ``object_store_factory`` (callable or None, optional, default None) — Custom store; requires ``source="object_storage"``.
* ``rank`` (int or None, optional, default None) — Process rank; pass with ``world_size``.
* ``world_size`` (int or None, optional, default None) — Number of ranks; captured from DDP if omitted.
* ``shuffle`` (bool, optional, default False) — Permute episodes before rank and worker partitioning.
* ``seed`` (int or None, optional, default None) — Nonnegative shuffle seed. With ``shuffle=True``, omitting ``seed`` uses 0.

Returns: ``TorchDataset``.

Raises: ``ValueError`` if the version is uncommitted, recordings are missing,
``split_config``/``split`` are not passed together, or ``rank``/``world_size``
are not passed together.

.. code-block:: python

   from foxglove.datasets.torch import read_dataset

   ds = read_dataset(
       "dset_123", version=2, topics=["/camera"], read_episode=read_episode
   )

foxglove.datasets.torch.TorchDataset
------------------------------------

Iterable dataset created by ``torch.read_dataset``. Subclass of
``torch.utils.data.IterableDataset``.

set_epoch
^^^^^^^^^

Select the episode order for the next DataLoader pass. Call on every rank
after the previous pass completes. Active iterators keep the order they
started with. Shuffling happens before rank and worker partitioning.

Parameters:

* ``epoch`` (int, required) — Nonnegative signed 64-bit integer.

Returns: ``None``.

Raises: ``TypeError`` if ``epoch`` is not an int; ``ValueError`` if it is
outside ``0 .. 2**63 - 1``.

.. code-block:: python

   dataset.set_epoch(1)

foxglove.datasets.torch.to_image_tensor
---------------------------------------

Convert channels-last RGB images to a contiguous channels-first tensor.
Preserves dtype and device.

Parameters:

* ``frame`` (mapping, required) — Sample or batch whose ``image`` is ``[..., H, W, 3]``.

Returns: tensor ``[..., 3, H, W]``.

Raises: ``ValueError`` if ``image`` is not channels-last RGB.

.. code-block:: python

   from foxglove.datasets.torch import to_image_tensor

   batch = to_image_tensor(batch)

foxglove.datasets.ray.read_dataset
----------------------------------

Return a Ray Data dataset of sample mappings. ``read_episode`` must yield
mappings with consistent column types.

Parameters:

* ``dataset_id`` (str, required) — Dataset ID.
* ``version`` (int, required) — Committed version number.
* ``topics`` (list of str, required) — Topics to stream.
* ``read_episode`` (callable, required) — Convert an ``EpisodeReader`` into sample mappings.
* ``source`` (str, optional, default ``foxglove``) — ``foxglove`` or ``object_storage``.
* ``split_config`` (DatasetSplit or None, optional, default None) — Pass with ``split``.
* ``split`` (str or None, optional, default None) — ``train``, ``validation``, or ``test``.
* ``include_properties`` (bool, optional, default False) — Load pinned custom properties.
* ``client_factory`` (callable or None, optional, default None) — Build a client.
* ``object_store_factory`` (callable or None, optional, default None) — Custom store; requires ``source="object_storage"``.
* ``concurrency`` (int or None, optional, default None) — Cap on concurrent read tasks.

Returns: ``ray.data.Dataset``.

Raises: ``ValueError`` if the version is uncommitted, recordings are missing,
or ``split_config``/``split`` are not passed together. ``TypeError`` if
``read_episode`` yields a non-mapping.

.. code-block:: python

   from foxglove.datasets.ray import read_dataset

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
* ``properties`` (mapping) — Pinned custom properties. Requires
  ``include_properties=True``. Empty if the episode has none.

Raises: ``RuntimeError`` on ``properties`` if they were not requested.

iter_messages
^^^^^^^^^^^^^

Stream the selected topics in log-time order. Each call opens a new stream.

Parameters:

* ``decoder_factories`` (list of DecoderFactory or None, optional, default None) — MCAP decoders; ``None`` uses the client's defaults.
* ``topics`` (list of str or None, optional, default None) — Nonempty subset of the dataset topics.
* ``lookback`` (timedelta, optional, default 0) — Extra history before the episode start. When you yield samples, keep those whose ``message.log_time`` (Unix nanoseconds) is at or after the episode start. ``decode_h264`` already does this.

Returns: tuples of schema (possibly ``None``), channel, raw MCAP message, and
decoded payload. ``message.log_time`` is Unix nanoseconds.

Raises: ``RuntimeError`` if the reader is closed; ``ValueError`` if
``lookback`` is negative or ``topics`` is empty or outside the dataset
topics.

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
* ``groups`` (mapping or sequence or None, optional, default None) — Episode ID to group ID mapping.

Returns: a ``DatasetSplit``.

Raises: ``ValueError`` if proportions are invalid or ``groups`` IDs are not
unique nonempty strings.

.. code-block:: python

   from foxglove.datasets import DatasetSplit

   split = DatasetSplit(train=0.8, validation=0.1, test=0.1, seed=0)

inspect_splits
--------------

Return train, validation, and test sizes from episode metadata. Same
assignment as the PyTorch and Ray readers. Counts cover the whole committed
version, before rank or worker partitioning.

Parameters:

* ``dataset_id`` (str, required) — Dataset ID.
* ``version`` (int, required) — Committed version number.
* ``split_config`` (DatasetSplit, required) — Split configuration.
* ``client_factory`` (callable or None, optional, default None) — Build a client.

Returns: ``dict`` of split name to ``SplitSummary``.

Raises: ``ValueError`` if the version is uncommitted, recordings are missing,
or ``split_config`` is not a ``DatasetSplit``.

.. code-block:: python

   from foxglove.datasets import DatasetSplit, inspect_splits

   summaries = inspect_splits("dset_123", version=2, split_config=DatasetSplit())

SplitSummary
------------

Group and episode counts for one split, before rank or worker partitioning.

Fields:

* ``group_count`` (int) — Connected recording groups in this split.
* ``episode_count`` (int) — Episodes in those groups.

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

ObjectStore
-----------

Protocol for worker-local storage access. ``source="object_storage"`` uses
the built-in store for ``s3``, ``gs``, and ``az`` (Azure also needs
``azure_storage_account_name``) with each worker's cloud credentials.
Objects must be indexed MCAP. Pass ``object_store_factory`` for another
store (requires ``source="object_storage"``).

open
^^^^

Open a seekable binary file. The SDK owns and closes the handle.

Parameters:

* ``location`` (ObjectLocation, required) — Recording object to open.

Returns: a seekable binary file supporting ``read``, ``readinto``, and ``seek``.

decode_h264
-----------

Yield RGB frames from ``foxglove.CompressedVideo`` H.264 Annex B. Requires
``foxglove-sdk[video]``. Usable as ``read_episode`` or inside a callback.

Parameters:

* ``episode`` (EpisodeReader, required) — Episode whose window bounds output frames.
* ``topic`` (str or None, optional, default None) — One selected topic, or all selected topics.
* ``lookback`` (timedelta, optional, default 5 seconds) — History budget before the episode start.
* ``decoder_factories`` (list of DecoderFactory or None, optional, default None) — MCAP deserializers; ``None`` uses the client's defaults.

Returns: ``VideoSample`` dicts, ordered per channel.

Raises: ``VideoDecodeError`` if the stream is not H.264 Annex B
CompressedVideo with one image per message, I/P frames, and an initial IDR
keyframe with SPS/PPS.

.. code-block:: python

   from foxglove.datasets.video import decode_h264
   from foxglove.datasets.torch import read_dataset

   ds = read_dataset("dset_123", version=2, topics=["/camera"], read_episode=decode_h264)

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

VideoDecodeError
----------------

Raised when video data cannot be decoded. Subclass of ``ValueError``.
