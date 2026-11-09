Dataset loading for ML
======================

Load a Foxglove dataset version into PyTorch or Ray Data. Choose the topics to read and define how
to convert each episode into training samples.

.. warning::

   The ``foxglove.datasets`` APIs, including the PyTorch and Ray Data adapters
   and ``EpisodeReader``, are experimental and unstable. They may change in
   backward-incompatible ways as we continue development and incorporate user
   feedback.

Installation
------------

Install the extra for your framework, or both to use Ray with PyTorch:

.. code-block:: bash

   pip install 'foxglove-sdk[torch]'      # PyTorch
   pip install 'foxglove-sdk[ray]'        # Ray Data
   pip install 'foxglove-sdk[torch,ray]'  # Both

PyTorch
-------

Set ``FOXGLOVE_API_TOKEN`` before running this example. It reads JSON messages
with ``speed`` and ``steering`` fields; adapt ``read_episode`` to your data.

.. code-block:: python

   from foxglove.datasets.torch import read_dataset
   from torch.utils.data import DataLoader

   def read_episode(episode):
       for schema, channel, message, decoded in episode.iter_messages():
           yield {"speed": decoded["speed"], "steering": decoded["steering"]}

   if __name__ == "__main__":
       dataset = read_dataset(
           "ds_123", version=7, topics=["/training/measurements"],
           read_episode=read_episode,
       )
       loader = DataLoader(
           dataset, batch_size=32, num_workers=2,
           multiprocessing_context="spawn", prefetch_factor=1,
       )
       for batch in loader:
           print(batch)

The result is an ``IterableDataset``. Your callback can yield tensors, tuples,
dictionaries, or objects handled by a custom ``collate_fn``.

Ray Data
--------

Use the same callback with the Ray adapter:

.. code-block:: python

   from foxglove.datasets.ray import read_dataset

   dataset = read_dataset(
       "ds_123", version=7, topics=["/training/measurements"],
       read_episode=read_episode, concurrency=2,
   )
   for batch in dataset.iter_torch_batches(batch_size=32):
       print(batch)

The result is a ``ray.data.Dataset``. Callbacks must yield mappings with
consistent column types, such as scalars or NumPy arrays. Use ``map_batches`` for
further processing. ``concurrency`` limits the number of simultaneous read tasks.

Episode properties
------------------

``EpisodeReader.properties`` is a read-only mapping of the episode's custom
properties. Values come from the episode version pinned by the dataset version, so
they match the messages being read even if the episode is edited later. Episodes
without custom properties return an empty mapping. Both adapters supply the same
reader, so this callback works with PyTorch and Ray Data:

.. code-block:: python

   def read_episode(episode):
       # Skip episodes reviewers rejected; unreviewed episodes are kept.
       if episode.properties.get("quality") == "rejected":
           return
       weight = float(episode.properties.get("weight", 1.0))
       for schema, channel, message, decoded in episode.iter_messages():
           yield {"speed": decoded["speed"], "weight": weight}

Pass a default to ``get`` for properties that some episodes do not set, so every
sample has the same fields and types. Episodes can have different property keys, so
select the properties you need rather than yielding the whole mapping: DataLoader's
default collation fails on samples with differing keys or ``None`` values, and Ray
Data requires consistent column types.

Object storage
--------------

The default ``source="foxglove"`` downloads messages through Foxglove. For
customer-managed indexed storage, pass ``source="object_storage"`` to either adapter.
Foxglove supplies the episode list and recording locations; workers read MCAP
files directly with their cloud credentials. The same callback works for both
sources:

.. code-block:: python

   dataset = read_dataset(
       "ds_123", version=7, topics=["/training/measurements"],
       read_episode=read_episode, source="object_storage",
   )

.. important::

   Object storage reads require the API and client to provide recording
   locations with ``location.scheme`` (``s3``, ``gs``, or ``az``).

The SDK selects S3, GCS, or Azure storage from the location's scheme. S3 bucket
regions are discovered automatically and cached; Azure uses the location's
``azure_storage_account_name``. The framework extras include ``pyarrow``.
Configure native cloud credentials on each worker, such as environment variables
or the machine's IAM role, and grant read access to the original objects. The
Foxglove API token does not grant bucket access. Run compute near your storage to
avoid cross-region transfer.

Object storage reads require immutable MCAP recordings with summaries, chunk
indexes, and message indexes. Missing locations, permissions, unsupported formats, and missing indexes
raise errors; there is no fallback to Foxglove downloads. Query-optimized sites
that do not retain original objects should use the default download path.

Custom factories
^^^^^^^^^^^^^^^^

Use ``client_factory`` for custom Foxglove authentication or configuration.
Without a factory, the SDK creates a client using ``FOXGLOVE_API_TOKEN``.
Define factories at module scope so workers can serialize them:

.. code-block:: python

   import os
   from foxglove.client import Client

   def make_client():
       return Client(token=os.environ["MY_FOXGLOVE_TOKEN"])

   dataset = read_dataset(
       "ds_123", version=7, topics=["/training/measurements"],
       read_episode=read_episode, client_factory=make_client,
   )

For custom storage or caching, pass ``object_store_factory``. A factory creates
one store in each consuming process; its ``open`` method receives an
``ObjectLocation`` and returns a seekable binary handle. For example:

.. code-block:: python

   import pyarrow.fs as fs
   from foxglove.datasets import ObjectLocation

   class S3Store:
       def __init__(self):
           self.filesystem = fs.S3FileSystem(region="us-east-1")

       def open(self, location: ObjectLocation):
           return self.filesystem.open_input_file(
               f"{location.bucket}/{location.path}"
           )

   dataset = read_dataset(
       "ds_123", version=7, topics=["/training/measurements"],
       read_episode=read_episode, source="object_storage", object_store_factory=S3Store,
   )

A custom ``object_store_factory`` requires ``source="object_storage"``. Use the
exact bucket and object key from the location, which may differ from the display
filename. Create live filesystem clients inside the factory; do not capture open files or short-lived
credentials. A custom store can return a seekable ``fsspec`` binary handle with
bounded caching. Use ``multiprocessing_context="spawn"`` for PyTorch workers.

The built-in S3 store discovers bucket regions through AWS. For S3-compatible
services such as MinIO, custom endpoints, or networks that cannot reach the
region-discovery endpoint, use a custom factory. Configure its
``fs.S3FileSystem`` with an explicit ``region`` and, when needed,
``endpoint_override="minio.example.com:9000"``. This bypasses built-in region
discovery. Configure TLS and credentials for that endpoint in the factory.

Performance and iteration behavior:

* Planning requests recording locations with the paginated episode list;
  it does not open objects. Storage clients are created once per worker iteration
  or Ray read task, after episode sharding. Object storage workers do not need
  to call the Foxglove API, and the planning client factory is not serialized.
* MCAP indexes select chunks by topic and the episode's inclusive time window,
  extended by ``lookback`` when requested. A selected chunk may contain other
  topics, so the reader still fetches and decompresses that whole chunk. The SDK
  uses a 64 KiB buffer to coalesce small header and summary reads.
* Recordings are merged in log-time order before decoding. Equivalent schemas
  and channels share episode-local IDs; conflicting file-local IDs are remapped.
  Original timestamps and payload bytes are preserved. Files close as each
  recording is exhausted, including errors and early termination. Recordings
  outside the requested time range close after their summaries are read.
* Before the first sample, each recording's summary is read and the first
  matching chunk from each overlapping recording is loaded to establish global
  timestamp order. Startup latency therefore grows with the number of recordings.
* Memory includes the indexes and active decompressed chunks of the episode's
  recordings, decoded samples, and framework prefetch buffers. It is not bounded
  by Ray's output block target alone. Tune Ray ``concurrency`` or PyTorch
  ``num_workers`` and ``prefetch_factor`` against your workload and storage limits.
* Every new iteration reads again. Sparse windows benefit from range reads;
  repeated dense epochs may benefit from a bounded worker-local disk cache in
  your store. Whole-file caches fetch the entire recording and can increase
  first-sample latency and transfer. Keep recordings immutable and manage cache
  size explicitly.

Message formats
---------------

JSON decoding (schema encoding ``jsonschema``) is included by default. Other encodings
need the matching MCAP decoder package on every worker. Install the package for your
data's schema encoding:

.. list-table::
   :header-rows: 1

   * - Schema encoding
     - Package
   * - ``jsonschema``
     - included by default
   * - ``protobuf``
     - ``mcap-protobuf-support``
   * - ``ros1msg``
     - ``mcap-ros1-support``
   * - ``ros2msg``
     - ``mcap-ros2-support``

.. code-block:: bash

   pip install mcap-protobuf-support   # example: protobuf recordings

If a decoder for an encoding in this table is missing, the error names the
package to install.

For custom decoding, create MCAP decoder factories inside your callback and pass
them to ``episode.iter_messages(decoder_factories=[...])``. Messages arrive in log-time order
as ``(schema, channel, message, decoded_message)`` tuples.

See ``python/foxglove-sdk-examples/dataset-training`` for a complete example.

H.264 video
-----------

Install ``foxglove-sdk[video,torch]`` or ``foxglove-sdk[video,ray]``, plus the
MCAP decoder for your recording (see `Message formats`_).

Pass ``decode_h264`` as the episode callback to decode every selected camera topic.
Using equally sized camera images:

.. code-block:: python

   from foxglove.datasets.torch import read_dataset, to_image_tensor
   from foxglove.datasets.video import decode_h264
   from torch.utils.data import DataLoader

   if __name__ == "__main__":
       dataset = read_dataset(
           "ds_123", version=7,
           topics=["/camera/front", "/camera/wrist"],
           read_episode=decode_h264,
       )
       loader = DataLoader(
           dataset, batch_size=16, num_workers=2,
           multiprocessing_context="spawn", prefetch_factor=1,
       )
       for batch in loader:
           images = to_image_tensor(batch)  # [B, 3, H, W]
           print(batch["topic"], images.shape)

Each sample is one camera frame: an RGB uint8 NumPy array in ``[H, W, 3]`` order,
its topic, and original timestamps in nanoseconds. Frames from different cameras
are not synchronized or paired. Match timestamps in your own callback to assemble
multi-camera samples. For Ray, change the adapter import and use ``dataset.iter_rows()``.

Within a callback, ``decode_h264(episode, topic="/camera/front")`` reads just that
camera in a separate download. The topic must be among those selected by
``read_dataset``. Without ``topic``, all selected cameras share one download.

Important limits:

* CPU decoding of ``CompressedVideo`` H.264 Annex B only: one image per message,
  no B-frames, and an initial IDR keyframe containing SPS/PPS.
* Missing video messages can cause undetected image corruption until the next
  keyframe, even when decoding raises no error.
* Decoding needs earlier frames. The default five-second ``lookback`` searches only
  recordings attached to the episode. Missing history raises ``VideoDecodeError``;
  increase the lookback or attach the missing recording. Compressed history is
  buffered and decoded only from the latest usable keyframe.
  Only frames within the episode's inclusive MCAP log-time window are emitted.
* Without ``topic``, non-video messages are discarded. An explicit non-video
  ``topic`` raises ``VideoDecodeError``. Read other sensors separately with
  ``episode.iter_messages(topics=["/joint_states"])``.
* Resize images as needed before batching. ``to_image_tensor(sample)`` from
  ``foxglove.datasets.torch`` accepts channels-last RGB NumPy images or tensors, including batches,
  and moves the color channels before height and width. Call it once, before or
  after DataLoader; it preserves dtype and device. Normalize and move to your
  training device as needed.

To request ten seconds of history, pass ``read_episode=read_video`` to
``read_dataset`` using this callback, which also works with spawned workers:

.. code-block:: python

   from datetime import timedelta
   from functools import partial

   read_video = partial(decode_h264, lookback=timedelta(seconds=10))

Consume the iterator fully to receive buffered frames. The episode closes streams
automatically, including on errors or early termination.

Dataset splits
--------------

Use the same ``DatasetSplit`` configuration with either reader to select stable
training, validation, and test sets from a committed dataset version:

.. code-block:: python

   from foxglove.datasets import DatasetSplit, inspect_splits
   from foxglove.datasets.torch import read_dataset  # or foxglove.datasets.ray

   splits = DatasetSplit(train=0.8, validation=0.1, test=0.1, seed=42)
   summary = inspect_splits("ds_...", version=3, split_config=splits)
   for name, counts in summary.items():
       print(name, counts.group_count, counts.episode_count)

   train = read_dataset(
       "ds_...", version=3, topics=["/camera/image"], read_episode=read_episode,
       split_config=splits, split="train",
   )

Repeat with ``split="validation"`` or ``"test"`` for the other pools. Assignment
happens before epoch shuffling and rank/worker partitioning. The split seed is
independent of any shuffle seed. Omitting both arguments preserves the full dataset.
No server-side datasets are created.

Episodes sharing any recording ID are kept together, transitively, including an
episode attached to multiple recordings. Split planning requests recording IDs in
both source modes and fails if those IDs are missing. Frames within an episode
remain together. Recording grouping cannot detect related sessions, sites, or
robots spread across different recordings: pass ``groups={episode_id: group_id, ...}``
to ``DatasetSplit`` to add those constraints. The mapping must cover exactly the
committed version's episode IDs; shared recordings remain together even if the
caller assigns them different group IDs.

Proportions are assignment probabilities per group, not exact fractions of groups,
episodes, or samples. Connected groups can be large, and even nonzero proportions
can produce empty sets. Use ``inspect_splits`` before training or evaluation to
check group and episode counts for all three sets. Inspection fetches metadata
only and uses the same assignment as both readers, before worker/rank partitioning.
Frame and sample counts depend on decoding and ``read_episode`` and are not known
from this metadata. Save the committed dataset version,
proportions, split seed, and optional group mapping with model artifacts.
Custom group mappings are snapshotted as sorted, immutable episode/group pairs.
Configurations support ``pickle``, ``copy.deepcopy``, and ``dataclasses.asdict``;
restore a dictionary snapshot with ``DatasetSplit(**snapshot)``. JSON snapshots
also round-trip with ``DatasetSplit(**json.loads(json.dumps(asdict(config))))``.
Membership is independent of API pagination, source, framework, epoch, and worker
count. Each connected group is hashed by its smallest custom group ID when
``groups`` is provided, otherwise by its smallest recording ID. Adding episodes
within an existing recording group preserves membership. With custom groups,
adding recordings to an existing group also preserves membership: use stable
session, site, or robot IDs, since renaming these IDs can change assignments.
Changing the dataset version can merge groups or change a group's smallest ID,
which can move existing episodes between splits. For comparable training and
evaluation, use the same committed version and split configuration; assignments
across different versions do not guarantee separation.

API reference
-------------

PyTorch
^^^^^^^

.. autofunction:: foxglove.datasets.torch.read_dataset

Ray Data
^^^^^^^^

.. autofunction:: foxglove.datasets.ray.read_dataset

Episode reader
^^^^^^^^^^^^^^

.. autoclass:: foxglove.datasets.EpisodeReader
   :members:

Object storage
^^^^^^^^^^^^^^

.. autoclass:: foxglove.datasets.ObjectLocation
   :members:

.. autoclass:: foxglove.datasets.ObjectStore
   :members:

Video decoding
^^^^^^^^^^^^^^

.. autofunction:: foxglove.datasets.video.decode_h264

.. autoclass:: foxglove.datasets.video.VideoSample
   :members:

.. autoexception:: foxglove.datasets.video.VideoDecodeError

.. autofunction:: foxglove.datasets.torch.to_image_tensor

Dataset splits
^^^^^^^^^^^^^^

.. autoclass:: foxglove.datasets.DatasetSplit

.. autofunction:: foxglove.datasets.inspect_splits

.. autoclass:: foxglove.datasets.SplitSummary
