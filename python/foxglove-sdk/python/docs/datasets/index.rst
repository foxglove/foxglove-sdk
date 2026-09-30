Dataset loading for ML
======================

Load a Foxglove dataset version into PyTorch or Ray Data. Choose the topics to read and define how
to convert each episode into training samples.

.. warning::

   The PyTorch and Ray Data APIs are experimental and unstable. They may change
   in backward-incompatible ways as we continue development and incorporate user
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
