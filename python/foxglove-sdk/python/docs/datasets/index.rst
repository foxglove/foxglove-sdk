Dataset loading for ML
======================

Load a Foxglove dataset version into PyTorch or Ray Data. Choose the topics to read and define how
to convert each episode into training samples.

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

The result is a ``ray.data.Dataset``. Callbacks must yield dictionaries with
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

Install the optional CPU video decoder together with your framework:

.. code-block:: bash

   pip install 'foxglove-sdk[video,torch]'  # or [video,ray]
   pip install mcap-protobuf-support      # for Protobuf recordings

``decode_h264(episode)`` yields sample dictionaries containing RGB NumPy arrays
(uint8, ``[H, W, 3]``), topic/channel/frame identifiers, and the original capture, log,
and publish times in integer nanoseconds. Its state belongs to one callback invocation and is independent
for each MCAP channel. The same callback works with either adapter:

.. code-block:: python

   import os
   from datetime import timedelta
   from foxglove.client import Client
   from foxglove.datasets.video import decode_h264

   def make_client():
       return Client(token=os.environ["FOXGLOVE_API_TOKEN"])

   def read_video_episode(episode):
       return decode_h264(episode)

   if __name__ == "__main__":
       from foxglove.datasets.torch import read_dataset
       from torch.utils.data import DataLoader

       dataset = read_dataset(
           "ds_123", version=7, topics=["camera_h264"],
           read_episode=read_video_episode, client_factory=make_client,
       )
       for batch in DataLoader(dataset, batch_size=16, num_workers=2):
           print(batch["image"].shape)  # [N, H, W, 3]

You can also pass ``read_episode=decode_h264`` directly. The decoder infers the episode
window and requests five seconds of preceding history by default. Override the history
budget with ``decode_h264(episode, lookback=timedelta(seconds=10))``. For custom MCAP
message deserialization, pass ``decoder_factories=[...]`` constructed inside the callback.

For independent camera streams, select a topic on each call:

.. code-block:: python

   def read_video_episode(episode):
       front = decode_h264(episode, topic="/camera/front")
       wrist = decode_h264(episode, topic="/camera/wrist")
       # Consume each iterator independently. This example emits unpaired frames.
       yield from front
       yield from wrist

Include both topics in ``read_dataset(topics=[...])``. Each iterator opens a separate,
server-filtered download on first iteration; it does not download the other camera's
messages. Omitting ``topic`` opens one download covering all selected topics. Topic
overrides must belong to the dataset's selected topics.

Samples retain ``topic`` and ``timestamp_ns`` so users can later assemble camera features.
This API does not synchronize cameras or yield paired samples; frame indices across
cameras need not correspond. Do not use ``zip`` as a substitute for timestamp matching.

For Ray, import ``read_dataset`` from ``foxglove.datasets.ray`` with the same arguments
and consume ``dataset.iter_batches(batch_size=16)``. Install dependencies on every worker.
For a PyTorch-only callback, ``foxglove.datasets.torch.to_image_tensor(frame)`` returns a
contiguous uint8 CPU tensor in ``[C, H, W]`` order. Normalize, resize, and transfer to a
training device in your own pipeline. Images must have compatible shapes for stacking
in a batch; separate or resize cameras with different resolutions.

Episode boundaries and lookback
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

A delta frame depends on earlier frames. ``lookback`` expands the download start time
while retaining the episode's original boundaries. Only recordings attached to the episode
are searched. The default five seconds is a configurable history budget, not a guarantee
that a usable keyframe exists. If necessary, increase it or attach the recording containing
the required history to the episode and commit a new dataset version.

The decoder skips lookback frames before the first IDR keyframe with SPS/PPS,
decodes subsequent lookback to initialize state, and emits only frames whose MCAP log
times fall inside the inclusive episode window. It raises ``VideoDecodeError`` if an
in-window message lacks initialization history, rather than dropping training frames until
the next keyframe. Capture timestamps are preserved separately and do not determine
window membership. Duplicate timestamps remain distinct frames. Invalid base64,
malformed NAL framing or headers, and unsupported codecs raise errors even during
lookback. Slice data in frames skipped before initialization is not decoded or checked
for corruption. Once initialized, decoding errors also raise during lookback.

Always feed every video message in order; sample frames only after decoding. Decoder
buffering can delay output, so consume the iterator to exhaustion to receive flushed
frames and validate completeness. The episode closes all decoder iterators and input streams when the callback ends,
including on failure, cancellation, and partial consumption. To release an iterator
earlier within a callback, call its ``close()`` method or use ``contextlib.closing``. Frames remain ordered within each
channel; output from different channels is not guaranteed to be globally time-sorted.

Supported formats
^^^^^^^^^^^^^^^^^

* ``foxglove.CompressedVideo`` with ``format="h264"`` and Annex B payloads.
* One complete encoded image per message, with SPS and PPS accompanying the initial
  IDR keyframe. B-frames, fragmented images, and multiple images per message are unsupported.
* Deserialized Foxglove JSON (base64 data and ``sec``/``nsec`` timestamps), Protobuf,
  and ROS 1/2 ``foxglove_msgs/CompressedVideo`` messages. Install the corresponding MCAP
  decoder separately. Other schemas are ignored when reading all selected topics;
  an explicit non-video ``topic`` raises an error. Other video codecs raise an error.
* CPU decoding through PyAV 16 / FFmpeg and RGB conversion through NumPy. Hardware
  acceleration is not currently exposed. No framework is required by the shared helper.

When combining cameras with other sensors, read each camera with
``decode_h264(episode, topic="/camera")`` and sensor messages with
``episode.iter_messages(topics=["/joint_states"])``. These are separate downloads;
your callback must match timestamps and assemble samples. Reading all selected topics
with ``decode_h264(episode)`` also downloads and discards non-video messages, including
their lookback history. Sharing a single download between video decoding and raw sensor
processing is not supported by this helper.

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
