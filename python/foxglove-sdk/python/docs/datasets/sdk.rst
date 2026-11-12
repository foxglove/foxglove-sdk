SDK reference
=============

``foxglove.datasets`` loads a committed dataset version into PyTorch or Ray
Data. Requires ``foxglove-sdk>=0.31`` (``[torch]``, ``[ray]``, ``[video]``
extras) and ``foxglove-client>=0.23``.

.. warning::

   These APIs, including the adapters and ``EpisodeReader``, are experimental
   and unstable. They may change in backward-incompatible ways.

How-tos: :doc:`pytorch`, :doc:`ray`. Worked example:
``python/foxglove-sdk-examples/dataset-training``.

By default the adapters create ``Client(token=os.environ["FOXGLOVE_API_TOKEN"])``
(host ``api.foxglove.dev``) and raise ``ValueError`` if the variable is unset.
Pass ``client_factory`` for another host or token.

foxglove.datasets.torch.read_dataset
------------------------------------

.. autofunction:: foxglove.datasets.torch.read_dataset

Inputs

.. list-table::
   :header-rows: 1
   :widths: 22 16 10 12 40

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``dataset_id``
     - str
     - yes
     -
     - Dataset ID.
   * - ``version``
     - int
     - yes
     -
     - Committed version number.
   * - ``topics``
     - list of str
     - yes
     -
     - Topics to stream.
   * - ``read_episode``
     - callable
     - yes
     -
     - Convert an ``EpisodeReader`` into samples.
   * - ``source``
     - str
     - no
     - foxglove
     - ``foxglove`` or ``object_storage``.
   * - ``split_config``
     - DatasetSplit or None
     - no
     - None
     - Split configuration; pass with ``split``.
   * - ``split``
     - str or None
     - no
     - None
     - ``train``, ``validation``, or ``test``.
   * - ``include_properties``
     - bool
     - no
     - False
     - Load pinned custom properties.
   * - ``client_factory``
     - callable or None
     - no
     - None
     - Build a client; default uses ``FOXGLOVE_API_TOKEN``.
   * - ``object_store_factory``
     - callable or None
     - no
     - None
     - Custom object store; requires ``source="object_storage"``.
   * - ``rank``
     - int or None
     - no
     - None
     - Process rank; pass with ``world_size``.
   * - ``world_size``
     - int or None
     - no
     - None
     - Number of ranks; captured from DDP if omitted.
   * - ``shuffle``
     - bool
     - no
     - False
     - Permute episodes before rank and worker partitioning.
   * - ``seed``
     - int or None
     - no
     - None
     - Nonnegative shuffle seed. With ``shuffle=True``, omitting
       ``seed`` uses 0.

Outputs

:class:`~foxglove.datasets.torch.TorchDataset`.

.. autoclass:: foxglove.datasets.torch.TorchDataset
   :members: set_epoch
   :exclude-members: __init__, __new__

set_epoch inputs

.. list-table::
   :header-rows: 1
   :widths: 22 16 10 12 40

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``epoch``
     - int
     - yes
     -
     - Nonnegative signed 64-bit integer for the next DataLoader pass.

Outputs

``None``. Shuffling happens before rank and worker partitioning, so every
rank must use the same seed and epoch.

foxglove.datasets.ray.read_dataset
----------------------------------

``read_episode`` must yield mappings with consistent column types. Ray
schedules and distributes read tasks.

.. autofunction:: foxglove.datasets.ray.read_dataset

Inputs

.. list-table::
   :header-rows: 1
   :widths: 22 16 10 12 40

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``dataset_id``
     - str
     - yes
     -
     - Dataset ID.
   * - ``version``
     - int
     - yes
     -
     - Committed version number.
   * - ``topics``
     - list of str
     - yes
     -
     - Topics to stream.
   * - ``read_episode``
     - callable
     - yes
     -
     - Convert an ``EpisodeReader`` into sample mappings.
   * - ``source``
     - str
     - no
     - foxglove
     - ``foxglove`` or ``object_storage``.
   * - ``split_config``
     - DatasetSplit or None
     - no
     - None
     - Split configuration; pass with ``split``.
   * - ``split``
     - str or None
     - no
     - None
     - ``train``, ``validation``, or ``test``.
   * - ``include_properties``
     - bool
     - no
     - False
     - Load pinned custom properties.
   * - ``client_factory``
     - callable or None
     - no
     - None
     - Build a client; default uses ``FOXGLOVE_API_TOKEN``.
   * - ``object_store_factory``
     - callable or None
     - no
     - None
     - Custom object store; requires ``source="object_storage"``.
   * - ``concurrency``
     - int or None
     - no
     - None
     - Cap on concurrent read tasks.

Outputs

``ray.data.Dataset``.

EpisodeReader
-------------

.. autoclass:: foxglove.datasets.EpisodeReader
   :members:

Splits
------

Proportions must be finite, non-negative, and sum to 1. Episodes that share a
recording, or the same ``groups`` ID, go to the same split.
``split_config`` and ``split`` must be passed together.

.. autoclass:: foxglove.datasets.DatasetSplit

Inputs

.. list-table::
   :header-rows: 1
   :widths: 22 16 10 12 40

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``train``
     - float
     - no
     - 0.8
     - Train proportion.
   * - ``validation``
     - float
     - no
     - 0.1
     - Validation proportion.
   * - ``test``
     - float
     - no
     - 0.1
     - Test proportion.
   * - ``seed``
     - int
     - no
     - 0
     - Split assignment seed, independent of epoch shuffling.
   * - ``groups``
     - mapping or sequence or None
     - no
     - None
     - Optional episode ID to group ID mapping.

Outputs

A :class:`~foxglove.datasets.DatasetSplit`.

.. autofunction:: foxglove.datasets.inspect_splits

Inputs

.. list-table::
   :header-rows: 1
   :widths: 22 16 10 12 40

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``dataset_id``
     - str
     - yes
     -
     - Dataset ID.
   * - ``version``
     - int
     - yes
     -
     - Committed version number.
   * - ``split_config``
     - DatasetSplit
     - yes
     -
     - Split configuration.
   * - ``client_factory``
     - callable or None
     - no
     - None
     - Build a client; default uses ``FOXGLOVE_API_TOKEN``.

Outputs

``dict`` of split name to :class:`~foxglove.datasets.SplitSummary`
(``group_count``, ``episode_count``).

.. autoclass:: foxglove.datasets.SplitSummary

Object storage
--------------

``source="object_storage"`` reads recordings whose ``location.scheme`` is
``s3``, ``gs``, or ``az`` (Azure also needs ``azure_storage_account_name``),
using each worker's cloud credentials. Objects must be indexed MCAP and must
not change during a read. For other stores, pass ``object_store_factory``
(requires ``source="object_storage"``).

.. autoclass:: foxglove.datasets.ObjectLocation
   :members:

.. autoclass:: foxglove.datasets.ObjectStore
   :members:

Video
-----

Optional extra ``foxglove-sdk[video]``. ``decode_h264`` yields
``VideoSample`` dicts from ``foxglove.CompressedVideo`` H.264 Annex B;
pass it directly as ``read_episode``.

.. autofunction:: foxglove.datasets.video.decode_h264

.. autoclass:: foxglove.datasets.video.VideoSample
   :members:

.. autoexception:: foxglove.datasets.video.VideoDecodeError

.. autofunction:: foxglove.datasets.torch.to_image_tensor

Inputs

.. list-table::
   :header-rows: 1
   :widths: 22 16 10 12 40

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``frame``
     - mapping
     - yes
     -
     - Sample or batch whose ``image`` is ``[..., H, W, 3]``.

Outputs

A contiguous tensor ``[..., 3, H, W]``.

Message encodings
-----------------

JSON is decoded by default. Protobuf and ROS decode when
``mcap-protobuf-support``, ``mcap-ros1-support``, or ``mcap-ros2-support``
are installed on every worker.
