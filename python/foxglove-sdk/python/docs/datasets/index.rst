Dataset loading for ML
======================

Read committed Foxglove dataset versions with PyTorch or Ray Data. The SDK fetches
episode metadata while planning, then streams only explicitly selected topics when
workers consume samples. A customer callback converts each episode's messages to
training samples; no feature mapping or synchronization policy is imposed.

Installation
------------

Install ``foxglove-sdk[torch]`` for standalone PyTorch or ``foxglove-sdk[ray]`` for
Ray Data. Install both extras to consume Ray batches as PyTorch tensors.

The extras install foxglove-client 0.20.0 or newer, which includes the required
dataset APIs:

.. code-block:: bash

   pip install 'foxglove-sdk[torch,ray]'

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
task concurrency. Both frameworks accept dictionaries containing scalars and NumPy
arrays; Ray requires compatible column schemas across samples.

``EpisodeReader`` exposes ``id``, ``start_time``, ``end_time``, and ``metadata``.
``iter_messages()`` preserves the client's tuple format and decoding behavior.
Configure any custom decoder support inside the client factory on each worker.
Readers are valid only during their callback; repeated message iteration downloads
the selected data again. Factories, callbacks, dependencies, and credentials must
be available on every worker.

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
