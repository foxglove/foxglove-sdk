Datasets and episodes
=====================

A dataset is a named, versioned collection of episodes in a Project. An episode
is a time range over one or more recordings, with optional custom properties
such as ``task`` or ``outcome``. Episodes exist independently of datasets, so
the same episode can belong to more than one training or evaluation set.

A dataset references recording data; it does not copy it. Each committed version
pins every episode to one episode version. Inspect a version in Foxglove, stream
it from Python, or download it as MCAP.

To build a dataset:

1. Create a dataset.
2. In Search, select results and choose **Add to dataset**. Each result becomes
   an episode.
3. Trim episodes and set their properties.
4. Commit a version.
5. Train on that version, or clone it.

The CLI, ``foxglove-client`` ``download_dataset``, and the browser each write
one MCAP per episode plus ``manifest.json``.

UI, Search, and commit: https://docs.foxglove.dev/docs/data/datasets

Episode pricing: https://foxglove.dev/pricing

.. toctree::
   :maxdepth: 1

   explore
   labeling
   pytorch
   ray
   versioning
   python-client
   rest
   sdk

References
----------

Python client (``foxglove-client`` 0.23.0 or later): :doc:`python-client`

Training adapters (``foxglove-sdk>=0.31``): :doc:`sdk`

REST map: :doc:`rest` and https://docs.foxglove.dev/api/datasets

CLI (v2.1.0 or later): ``foxglove datasets`` and ``foxglove episodes``
(https://docs.foxglove.dev/docs/cli)
