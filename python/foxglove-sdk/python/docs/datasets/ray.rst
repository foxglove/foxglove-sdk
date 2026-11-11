Train with Ray
==============

Stream a committed dataset version into a Ray Data dataset. The episode
callback is the same shape as the PyTorch adapter: it receives an
``EpisodeReader`` and must yield mappings with consistent column types
(scalars or NumPy arrays).

This example matches the Ray path in
``python/foxglove-sdk-examples/dataset-training/main.py``.

API reference: :doc:`sdk`.

**Prerequisites:** ``pip install 'foxglove-sdk[torch,ray]>=0.31'``;
``FOXGLOVE_API_TOKEN`` set on the driver **and every Ray worker** (for example
via ``ray.init(runtime_env={"env_vars": {"FOXGLOVE_API_TOKEN": os.environ["FOXGLOVE_API_TOKEN"]}})``).

.. code-block:: python

   import os

   import numpy as np
   import ray
   from foxglove.datasets import EpisodeReader
   from foxglove.datasets.ray import read_dataset


   def read_episode(episode: EpisodeReader):
       if episode.properties.get("quality") == "rejected":
           return
       for _schema, _channel, _message, payload in episode.iter_messages():
           yield {
               "inputs": np.asarray(
                   [payload["speed"], payload["acceleration"]], dtype=np.float32
               ),
               "target": np.asarray([payload["steering"]], dtype=np.float32),
           }


   ray.init(
       runtime_env={
           "env_vars": {"FOXGLOVE_API_TOKEN": os.environ["FOXGLOVE_API_TOKEN"]}
       }
   )
   try:
       dataset = read_dataset(
           "dset_123",
           version=7,
           topics=["/training/measurements"],
           read_episode=read_episode,
           include_properties=True,
           concurrency=2,
       )
       for batch in dataset.iter_torch_batches(batch_size=32, device="cpu"):
           print(batch["inputs"].shape, batch["target"].shape)
   finally:
       ray.shutdown()

``concurrency`` caps concurrent read tasks. **Result:** one line per batch,
e.g. ``torch.Size([32, 2]) torch.Size([32, 1])``.

``split_config`` and ``split`` work the same as in the PyTorch adapter.
``source="object_storage"`` reads indexed MCAP objects with the workers' cloud
credentials; the Foxglove token still loads the episode list.

From a clone of ``foxglove/foxglove-sdk`` (requires ``uv``)::

   export FOXGLOVE_API_TOKEN=your-token
   cd python/foxglove-sdk-examples/dataset-training
   uv run main.py --dataset-id dset_123 --version 7 --framework ray
