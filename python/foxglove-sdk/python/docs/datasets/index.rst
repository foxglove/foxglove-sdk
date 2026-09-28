Dataset loading for ML
======================

Read committed Foxglove dataset versions with PyTorch or Ray Data. The SDK fetches
episode metadata while planning, then streams only explicitly selected topics when
workers consume samples. Your callback converts each episode's messages to
training samples; no feature mapping or synchronization policy is imposed.

Installation
------------

Install ``foxglove-sdk[torch]`` for standalone PyTorch or ``foxglove-sdk[ray]`` for
Ray Data. Install both extras to consume Ray batches as PyTorch tensors.

The extras install foxglove-client 0.20.0 or newer, which includes the required
dataset APIs:

.. code-block:: bash

   pip install 'foxglove-sdk[torch,ray]'

JSON messages are decoded by ``foxglove-client``. For Protobuf, ROS 1, or ROS 2
messages, also install the corresponding MCAP decoder package on every worker. The
client automatically includes installed support packages in its default decoders:

.. code-block:: bash

   pip install mcap-protobuf-support  # or mcap-ros1-support / mcap-ros2-support

Usage
-----

Define the callback and client factory at module scope for spawned workers:

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
       loader = DataLoader(dataset, batch_size=32, num_workers=2, prefetch_factor=1)
       for batch in loader:
           print(batch)

For Ray, import ``read_dataset`` from ``foxglove.datasets.ray`` with the same
arguments. It returns a native ``ray.data.Dataset``; use ``map_batches`` for further
processing or ``iter_torch_batches`` for training. ``concurrency=2`` limits read
task concurrency. The shared dictionary example works with both frameworks. A
PyTorch callback may instead yield any sample shape accepted by its ``DataLoader``
collation, such as a tensor, tuple, mapping, or custom object. Ray callbacks must
yield dictionaries with Arrow-compatible values and compatible column schemas.

``EpisodeReader`` exposes ``id``, ``start_time``, ``end_time``, and ``metadata``.
``iter_messages()`` streams typed MCAP ``(schema, channel, message,
decoded_message)`` tuples. Pass decoder factories directly to it to use a custom or
explicit decoder set:

.. code-block:: python

   from mcap_protobuf.decoder import DecoderFactory as ProtobufDecoderFactory

   def read_episode(episode):
       decoder_factories = [ProtobufDecoderFactory()]
       for schema, channel, message, decoded in episode.iter_messages(
           decoder_factories=decoder_factories,
       ):
           yield {"value": decoded.value}

An explicit ``decoder_factories`` list replaces the client defaults, rather than
adding to them. Create those factories inside ``read_episode`` so each worker and
episode creates its own instances. Each invocation of ``iter_messages()`` is a
separate stream in log-time order. Reader and stream resources are closed when the
callback ends, and repeated message iteration downloads the selected data again.
Readers are valid only during their callback. Factories, callbacks, dependencies,
and credentials must be available on every worker.

Message decoding and media decoding are separate. Decoder factories turn MCAP
message bytes into Protobuf, ROS, JSON, or other message objects. Decoding a media
payload such as H.264 into frames is not included. A media decoder can be created in
``read_episode`` and kept per episode and video stream so it consumes messages in
order. The callback owns that decoder and must release it with a context manager or
``try``/``finally``. This reader does not fetch pre-roll before the episode
boundary. Episodes may begin between keyframes, and decoding their video frames may
require earlier keyframes and codec initialization data outside the episode.

Behavior and limits
-------------------

* The version must be committed. Missing recordings and callback failures raise
  errors; they are not silently skipped. Message-read errors include episode context.
* Topics must be explicit and nonempty; they are filtered by the server.
* The SDK does not buffer entire episodes. Ray blocks target 256 samples or 8 MiB
  of estimated data, whichever comes first. A single large sample can exceed this.
  User callbacks can still allocate unbounded memory themselves.
* Network/MCAP buffering and framework prefetch read ahead. Cancellation closes
  active streams when generators are closed, but cannot undo already transferred data.
* New iterations may redownload data. There is no SDK cache or sample-level index.
  Committed membership does not guarantee source bytes remain available forever.
* Callbacks may run again after a Ray task retry; avoid external side effects.
* PyTorch partitions episodes across distributed ranks, then loader workers.
  Initialize the process group before calling ``read_dataset`` or supply both
  ``rank`` and ``world_size`` explicitly. Different episode lengths can produce
  unequal numbers of batches per rank; the training loop must handle uneven inputs
  (for example, with DDP's join context). ``drop_last`` does not equalize ranks.
* The PyTorch result is an ``IterableDataset``. Do not use ``shuffle=True`` or a
  ``DistributedSampler``. Batching and collation remain DataLoader responsibilities.

See ``python/foxglove-sdk-examples/dataset-training`` for a complete script choosing
either framework. Synchronization, feature mappings, and temporal windows can be
implemented in the callback and added as reusable helpers later.

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
