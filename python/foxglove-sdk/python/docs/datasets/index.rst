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

Direct object storage (BYOS)
--------------------------------

The default ``source="foxglove"`` downloads messages through Foxglove. For
customer-managed indexed storage, pass ``source="object_storage"`` to either adapter.
Foxglove supplies episode metadata and recording locations; workers read MCAP
files directly with their cloud credentials. The same callback works for both
sources:

.. code-block:: python

   dataset = read_dataset(
       "ds_123", version=7, topics=["/training/measurements"],
       read_episode=read_episode, source="object_storage",
   )

.. important::

   Direct reads require the API and client to provide recording locations with
   ``location.scheme`` (``s3``, ``gs``, or ``az``).

The SDK selects S3, GCS, or Azure storage from the location's scheme. S3 bucket
regions are discovered automatically and cached; Azure uses the location's
``azure_storage_account_name``. The framework extras include ``pyarrow``.
Configure native cloud credentials on each worker, such as environment variables
or the machine's IAM role, and grant read access to the original objects. The
Foxglove API token does not grant bucket access. Run compute near your storage to
avoid cross-region transfer.

Direct reads require immutable, chunk-indexed MCAP recordings with summary
indexes. Missing locations, permissions, unsupported formats, and missing indexes
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

Supplying ``object_store_factory`` also selects direct reads when ``source`` is
omitted, preserving existing factory-based calls. Use the exact bucket and object
key from the location, which may differ from the display filename. Create live
filesystem clients inside the factory; do not capture open files or short-lived
credentials. A custom store can return a seekable ``fsspec`` binary handle with
bounded caching. Use ``multiprocessing_context="spawn"`` for PyTorch workers.

Performance and iteration behavior:

* Planning requests recording locations with the paginated episode metadata;
  it does not open objects. Storage clients are created once per worker iteration
  or Ray read task, after episode sharding. Direct-reading workers do not need
  to call the Foxglove API, and the planning client factory is not serialized.
* MCAP indexes select chunks by topic and the episode's inclusive time window,
  extended by ``lookback`` when requested. A selected chunk may contain other
  topics, so the reader still fetches and decompresses that whole chunk. The SDK
  uses a 64 KiB buffer to coalesce small header and summary reads.
* Recordings are merged in log-time order before decoding. Equivalent schemas
  and channels share episode-local IDs; conflicting file-local IDs are remapped.
  Original timestamps and payload bytes are preserved. Files close when the
  episode ends, including errors and early termination.
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

JSON decoding is included by default. For Protobuf or ROS, install ``mcap-protobuf-support``,
``mcap-ros1-support``, or ``mcap-ros2-support`` on every worker.

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
