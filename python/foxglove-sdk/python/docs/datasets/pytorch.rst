Train with PyTorch
==================

Stream a committed dataset version into a PyTorch ``IterableDataset``. You
choose the topics and convert each episode into samples.

This example matches
``python/foxglove-sdk-examples/dataset-training/main.py`` in the foxglove-sdk
repository.

API reference: :doc:`sdk`.

**Prerequisites:** Python 3.10+; a committed dataset version whose
``/training/measurements`` topic is JSON with numeric ``speed``,
``acceleration``, and ``steering``; an API key with ``datasets.list`` and
``data.stream``.

.. code-block:: bash

   pip install 'foxglove-sdk[torch]>=0.31'
   export FOXGLOVE_API_TOKEN=your-token

.. code-block:: python

   from collections.abc import Iterator

   import numpy as np
   from foxglove.datasets import EpisodeReader
   from foxglove.datasets.torch import read_dataset
   from torch.utils.data import DataLoader


   def read_episode(episode: EpisodeReader) -> Iterator[dict[str, np.ndarray]]:
       if episode.properties.get("quality") == "rejected":
           return
       for _schema, _channel, _message, payload in episode.iter_messages():
           yield {
               "inputs": np.asarray(
                   [payload["speed"], payload["acceleration"]], dtype=np.float32
               ),
               "target": np.asarray([payload["steering"]], dtype=np.float32),
           }


   if __name__ == "__main__":
       dataset = read_dataset(
           "dset_123",
           version=7,
           topics=["/training/measurements"],
           read_episode=read_episode,
           include_properties=True,
       )
       loader = DataLoader(
           dataset,
           batch_size=32,
           num_workers=2,
           prefetch_factor=1,
           multiprocessing_context="spawn",
       )
       for batch in loader:
           print(batch["inputs"].shape, batch["target"].shape)

**Result:** one line per batch, e.g. ``torch.Size([32, 2]) torch.Size([32, 1])``.
To adapt it, change ``topics`` and ``read_episode``. Reading
``episode.properties`` without ``include_properties=True`` fails with
``RuntimeError`` ("Episode properties were not requested; pass
include_properties=True to read_dataset"), raised from the DataLoader worker
as "Failed to read episode …".

In Foxglove, **Use for training** on a dataset page generates a similar script
for the committed version and topics you select. **Copy prompt** includes the
setup for a coding agent.

Shuffling and splits
--------------------

.. code-block:: python

   dataset = read_dataset(
       "dset_123",
       version=7,
       topics=["/training/measurements"],
       read_episode=read_episode,
       include_properties=True,
       shuffle=True,
       seed=42,
   )
   loader = DataLoader(
       dataset,
       batch_size=32,
       num_workers=2,
       prefetch_factor=1,
       multiprocessing_context="spawn",
   )
   for epoch in range(3):
       dataset.set_epoch(epoch)
       for batch in loader:
           ...

Episode order depends on ``seed + epoch``. Omitting ``seed`` uses 0.
Samples within an episode keep callback order. Ranks can yield different
sample counts; use an uneven-input policy such as DDP's ``join`` context.

.. code-block:: python

   from foxglove.datasets import DatasetSplit, inspect_splits
   from foxglove.datasets.torch import read_dataset

   splits = DatasetSplit(train=0.8, validation=0.1, test=0.1, seed=0)
   print(inspect_splits("dset_123", version=7, split_config=splits))
   train = read_dataset(
       "dset_123",
       version=7,
       topics=["/training/measurements"],
       read_episode=read_episode,
       include_properties=True,
       split_config=splits,
       split="train",
   )

``split_config`` and ``split`` must be passed together. Episodes that share a
recording land in the same split. Save ``splits`` with your checkpoint.

From a clone of ``foxglove/foxglove-sdk`` (requires ``uv``)::

   export FOXGLOVE_API_TOKEN=your-token
   cd python/foxglove-sdk-examples/dataset-training
   uv run main.py --dataset-id dset_123 --version 7 --framework torch
