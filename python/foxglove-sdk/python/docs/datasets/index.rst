Datasets
========

The ``foxglove.datasets`` modules load a version of a Foxglove dataset for model training.
:py:mod:`foxglove.datasets.torch` returns a PyTorch dataset, and
:py:mod:`foxglove.datasets.ray` returns a Ray Data dataset. Both stream the topics you select and
call a function you provide to turn each episode's messages into samples.
:py:mod:`foxglove.datasets.video` decodes H.264 video into RGB frames.
:py:mod:`foxglove.datasets` defines what the two readers share: the episode reader that your
function receives, dataset splits, and access to recordings in your own object storage.

.. _datasets-quickstart:

Quickstart
----------

Install the SDK with the ``torch`` and ``video`` extras, and set the ``FOXGLOVE_API_TOKEN``
environment variable to an API key:

.. code-block:: bash

   pip install 'foxglove-sdk[torch,video]>=0.31'

This script connects to Foxglove, lists the versions of a dataset, reads a committed version, and
takes one batch of video frames from the ``/camera`` topic:

.. code-block:: python

   import os

   from foxglove.client import Client
   from foxglove.datasets.torch import read_dataset
   from foxglove.datasets.video import decode_h264
   from torch.utils.data import DataLoader

   client = Client(token=os.environ["FOXGLOVE_API_TOKEN"])
   versions = client.get_dataset_versions(dataset_id="dset_123")
   committed = [v for v in versions.items if v["committed_at"]]

   ds = read_dataset("dset_123",
                     version=committed[0]["version_number"],
                     topics=["/camera"], read_episode=decode_h264)
   batch = next(iter(DataLoader(ds, batch_size=8)))

``batch["image"]`` is a ``uint8`` tensor of frames shaped ``[8, H, W, 3]``.

The `dataset-training example
<https://github.com/foxglove/foxglove-sdk/tree/main/python/foxglove-sdk-examples/dataset-training>`__
is a complete script for PyTorch and Ray. The product docs have a `training how-to
<https://docs.foxglove.dev/docs/data/datasets/training>`__.

In a notebook, DataLoader workers that start with the ``spawn`` or ``forkserver`` method cannot
load a ``read_episode`` function that is defined in a notebook cell, and iteration raises a
``RuntimeError`` that says a DataLoader worker exited unexpectedly. The default method is ``spawn``
on macOS and Windows, and ``forkserver`` on Linux in Python 3.14 and later. Define the function in
a module and import it, or keep ``num_workers=0``.

Install
-------

The modules require Python 3.10 or later. Install the extra for each module you use:

.. code-block:: bash

   # foxglove.datasets.torch
   pip install 'foxglove-sdk[torch]>=0.31'
   # foxglove.datasets.ray
   pip install 'foxglove-sdk[ray]>=0.31'
   # foxglove.datasets.video, with torch or ray
   pip install 'foxglove-sdk[torch,video]>=0.31'

Install the ``video`` extra together with ``torch`` or ``ray``. The video decoder reads the
episodes that those modules supply.

Reference
---------

For guides on building datasets and training with them, see the `Datasets documentation
<https://docs.foxglove.dev/docs/data/datasets>`__.

Every function and class is documented in the :doc:`API Reference <../api/index>`, which also
covers the dataset and episode methods of the :doc:`foxglove-client package <../api/client>`.
The REST API reference is at https://docs.foxglove.dev/api.
