Train with PyTorch
==================

Stream a committed dataset version into a PyTorch ``IterableDataset``. You
choose the topics and convert each episode into samples.

This example matches
``python/foxglove-sdk-examples/dataset-training/main.py`` in the foxglove-sdk
repository.

.. warning::

   ``foxglove.datasets`` is experimental and unstable. It may change in
   backward-incompatible ways.

Install ``foxglove-sdk[torch]``. Create an API key with ``datasets.list`` and
``data.stream``. Set ``FOXGLOVE_API_TOKEN``.

.. code-block:: bash

   pip install 'foxglove-sdk[torch]'
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
           "ds_123",
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

The example topic is JSON with numeric ``speed``, ``acceleration``, and
``steering`` fields. Change ``read_episode`` and ``topics`` for your schemas.
``include_properties=True`` is required to read ``episode.properties``;
without it, that access raises ``RuntimeError``.

In the Foxglove UI, **Use for training** on a dataset page generates a
similar script for the committed version and topics you select. **Copy
prompt** includes the setup for a coding agent.

Shuffling and splits
--------------------

Pass ``shuffle=True`` and a ``seed`` to permute episodes each epoch. Call
``dataset.set_epoch(n)`` before each DataLoader pass. Leave DataLoader
``shuffle`` off. Samples inside an episode stay in callback order.

For a stable train, validation, and test assignment, pass a
``DatasetSplit`` and ``split="train"`` (or ``"validation"`` / ``"test"``).
Use ``inspect_splits`` to print group and episode counts before training.
See :doc:`sdk`.

Run the checked-in example::

   export FOXGLOVE_API_TOKEN=your-token
   cd python/foxglove-sdk-examples/dataset-training
   uv run main.py --dataset-id ds_123 --version 7 --framework torch

Parameter list: :doc:`sdk`. REST and client calls for listing versions:
:doc:`python-client`.

.. note::

   A blog post on training with Datasets will link from here once it is
   published.
