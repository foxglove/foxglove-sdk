Train with Ray
==============

Stream a committed dataset version into a Ray Data dataset. The episode
callback is the same shape as the PyTorch adapter: it receives an
``EpisodeReader`` and must yield mappings with consistent column types
(scalars or NumPy arrays).

This example matches the Ray path in
``python/foxglove-sdk-examples/dataset-training/main.py``.

.. warning::

   ``foxglove.datasets`` is experimental and unstable. It may change in
   backward-incompatible ways.

Install ``foxglove-sdk[ray]`` (or ``foxglove-sdk[torch,ray]`` to consume
Torch batches). Set ``FOXGLOVE_API_TOKEN``.

.. code-block:: python

   import ray
   from foxglove.datasets.ray import read_dataset

   ray.init()
   try:
       dataset = read_dataset(
           "ds_123",
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

Define ``read_episode`` as in :doc:`pytorch`. ``concurrency`` limits how
many read tasks run at once. Use ``dataset.map_batches`` for further
processing.

``split_config`` and ``split`` work the same as in the PyTorch adapter.
``source="object_storage"`` reads indexed MCAP objects with the workers'
cloud credentials; the Foxglove token still loads the episode list.

Run the checked-in example::

   export FOXGLOVE_API_TOKEN=your-token
   cd python/foxglove-sdk-examples/dataset-training
   uv run main.py --dataset-id ds_123 --version 7 --framework ray

Parameter list: :doc:`sdk`.

.. note::

   A blog post on training with Datasets will link from here once it is
   published.
