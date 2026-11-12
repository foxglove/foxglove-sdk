Datasets
========

The ``foxglove.datasets`` modules load a committed version of a Foxglove dataset for model
training. :py:mod:`foxglove.datasets.torch` returns a PyTorch dataset, and
:py:mod:`foxglove.datasets.ray` returns a Ray Data dataset. Both stream the topics you select and
call a function you provide to turn each episode's messages into samples.
:py:mod:`foxglove.datasets.video` decodes H.264 video into RGB frames.
:py:mod:`foxglove.datasets` defines what the two readers share: the episode reader passed to your
function, train, validation, and test splits, and access to recordings in your own object storage.

The modules require Python 3.10 or later. Install the extra for each module you use:

.. code-block:: bash

   pip install 'foxglove-sdk[torch]'   # foxglove.datasets.torch
   pip install 'foxglove-sdk[ray]'     # foxglove.datasets.ray
   pip install 'foxglove-sdk[video]'   # foxglove.datasets.video

Extras can be combined, for example ``foxglove-sdk[torch,video]``.

For guides on building datasets and training with them, see the `Datasets documentation
<https://docs.foxglove.dev/docs/data/datasets>`__.

Every function and class is documented in the :doc:`API Reference <../api/index>`, which also
covers the dataset and episode methods of the :doc:`foxglove-client package <../api/client>`.
The REST API reference is at https://docs.foxglove.dev/api.
