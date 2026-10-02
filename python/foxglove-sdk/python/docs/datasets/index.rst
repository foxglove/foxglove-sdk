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

   import os
   from foxglove.client import Client
   from foxglove.datasets.torch import read_dataset
   from torch.utils.data import DataLoader

   def make_client():
       return Client(token=os.environ["FOXGLOVE_API_TOKEN"])

   def read_episode(episode):
       for schema, channel, message, decoded in episode.iter_messages():
           yield {"speed": decoded["speed"], "steering": decoded["steering"]}

   if __name__ == "__main__":
       dataset = read_dataset(
           "ds_123", version=7, topics=["/training/measurements"],
           read_episode=read_episode, client_factory=make_client,
       )
       loader = DataLoader(dataset, batch_size=32, num_workers=2)
       for batch in loader:
           print(batch)

The result is an ``IterableDataset``. Your callback can yield tensors, tuples,
dictionaries, or objects handled by a custom ``collate_fn``.

Ray Data
--------

Use the same callback and client factory with the Ray adapter:

.. code-block:: python

   from foxglove.datasets.ray import read_dataset

   dataset = read_dataset(
       "ds_123", version=7, topics=["/training/measurements"],
       read_episode=read_episode, client_factory=make_client, concurrency=2,
   )
   for batch in dataset.iter_torch_batches(batch_size=32):
       print(batch)

The result is a ``ray.data.Dataset``. Callbacks must yield mappings with
consistent column types, such as scalars or NumPy arrays. Use ``map_batches`` for
further processing. ``concurrency`` limits the number of simultaneous read tasks.

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
Using ``make_client`` from above and equally sized camera images:

.. code-block:: python

   from foxglove.datasets.torch import read_dataset, to_image_tensor
   from foxglove.datasets.video import decode_h264
   from torch.utils.data import DataLoader

   if __name__ == "__main__":
       dataset = read_dataset(
           "ds_123", version=7,
           topics=["/camera/front", "/camera/wrist"],
           read_episode=decode_h264, client_factory=make_client,
       )
       for batch in DataLoader(dataset, batch_size=16, num_workers=2):
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

Video decoding
^^^^^^^^^^^^^^

.. autofunction:: foxglove.datasets.video.decode_h264

.. autoclass:: foxglove.datasets.video.VideoSample
   :members:

.. autoexception:: foxglove.datasets.video.VideoDecodeError

.. autofunction:: foxglove.datasets.torch.to_image_tensor
