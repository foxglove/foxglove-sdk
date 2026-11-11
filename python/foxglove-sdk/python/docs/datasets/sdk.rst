SDK reference
=============

``foxglove.datasets`` loads a committed dataset version into PyTorch or Ray
Data. Install ``foxglove-sdk[torch]``, ``foxglove-sdk[ray]``, or both.

.. warning::

   These APIs, including the adapters and ``EpisodeReader``, are experimental
   and unstable. They may change in backward-incompatible ways.

How-tos: :doc:`pytorch`, :doc:`ray`. Worked example:
``python/foxglove-sdk-examples/dataset-training``.

The default client is ``foxglove.client.Client(token=os.environ["FOXGLOVE_API_TOKEN"])``.

foxglove.datasets.torch.read_dataset
------------------------------------

Plan a topic-filtered version; download messages during iteration. Returns
``TorchDataset``.

Inputs:

``dataset_id`` (str, required)
    Dataset id.

``version`` (int, required)
    Committed version number.

``topics`` (sequence of str, required)
    Topics to read.

``read_episode`` (callable, required)
    ``EpisodeReader`` → iterable of samples.

``source`` (``"foxglove"`` | ``"object_storage"``, default ``"foxglove"``)
    Foxglove download, or original indexed MCAP objects.

``split_config`` / ``split`` (optional)
    ``DatasetSplit`` and ``"train"`` | ``"validation"`` | ``"test"``.

``include_properties`` (bool, default False)
    Fetch pinned custom properties for ``EpisodeReader.properties``.

``client_factory`` / ``object_store_factory`` (optional)
    Override client or object-store construction. Factories used by workers
    must be pickleable. ``object_store_factory`` requires
    ``source="object_storage"``.

``rank`` / ``world_size`` (int, optional)
    Supply together, or the adapter reads ``torch.distributed``.

``shuffle`` (bool, default False), ``seed`` (int, optional)
    Permute the episode list with ``seed + epoch``. ``seed`` requires
    ``shuffle=True``.

.. autofunction:: foxglove.datasets.torch.read_dataset

.. autoclass:: foxglove.datasets.torch.TorchDataset
   :members: set_epoch
   :exclude-members: __init__, __new__

foxglove.datasets.ray.read_dataset
----------------------------------

Same planning arguments as the PyTorch adapter, plus ``concurrency`` (int,
optional) to limit simultaneous read tasks. ``read_episode`` must yield
mappings. Returns ``ray.data.Dataset``. No ``shuffle`` / ``rank`` arguments;
use Ray for distribution.

.. autofunction:: foxglove.datasets.ray.read_dataset

EpisodeReader
-------------

``id``, ``start_time``, ``end_time``. ``properties`` is a read-only mapping
and requires ``include_properties=True``. ``iter_messages`` yields
``(schema, channel, message, decoded)`` in log-time order. Optional
``decoder_factories``, ``topics``, ``lookback``.

.. autoclass:: foxglove.datasets.EpisodeReader
   :members:

Splits
------

``DatasetSplit(train=0.8, validation=0.1, test=0.1, seed=0, groups=None)``.
Episodes that share a recording stay together. ``inspect_splits`` returns
``SplitSummary`` (``group_count``, ``episode_count``) per split name. No
server-side datasets are created.

.. autoclass:: foxglove.datasets.DatasetSplit

.. autofunction:: foxglove.datasets.inspect_splits

.. autoclass:: foxglove.datasets.SplitSummary

Object storage
--------------

``source="object_storage"`` needs recording ``location.scheme`` of ``s3``,
``gs``, or ``az``. Workers use their own cloud credentials. The Foxglove
token does not grant bucket access. Recordings must be immutable MCAP with
summaries and indexes.

.. autoclass:: foxglove.datasets.ObjectLocation
   :members:

.. autoclass:: foxglove.datasets.ObjectStore
   :members:

Video
-----

Optional extra ``foxglove-sdk[video]``. ``decode_h264`` yields RGB frames
from ``CompressedVideo`` H.264 Annex B. ``to_image_tensor`` moves
channels-last RGB to channels-first.

.. autofunction:: foxglove.datasets.video.decode_h264

.. autoclass:: foxglove.datasets.video.VideoSample
   :members:

.. autoexception:: foxglove.datasets.video.VideoDecodeError

.. autofunction:: foxglove.datasets.torch.to_image_tensor

Message encodings
-----------------

``jsonschema`` is included. Install ``mcap-protobuf-support``,
``mcap-ros1-support``, or ``mcap-ros2-support`` on every worker for those
encodings. Missing decoders name the package in the error.
