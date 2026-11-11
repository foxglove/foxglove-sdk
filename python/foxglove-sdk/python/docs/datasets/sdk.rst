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

The default client is ``foxglove.client.Client(token=os.environ["FOXGLOVE_API_TOKEN"])``.

foxglove.datasets.torch.read_dataset
------------------------------------

.. autofunction:: foxglove.datasets.torch.read_dataset

.. autoclass:: foxglove.datasets.torch.TorchDataset
   :members: set_epoch
   :exclude-members: __init__, __new__

foxglove.datasets.ray.read_dataset
----------------------------------

Parameters match the PyTorch adapter except ``rank``, ``world_size``,
``shuffle``, and ``seed``, plus ``concurrency`` (int, optional): cap on
concurrent read tasks. Returns ``ray.data.Dataset``. ``read_episode`` must
yield mappings with consistent column types. Ray schedules and distributes
read tasks.

.. autofunction:: foxglove.datasets.ray.read_dataset

EpisodeReader
-------------

.. autoclass:: foxglove.datasets.EpisodeReader
   :members:

Splits
------

``DatasetSplit(train=0.8, validation=0.1, test=0.1, seed=0, groups=None)``.
Proportions must be non-negative and sum to 1. Episodes that share a
recording, or the same ``groups`` ID, go to the same split.
``inspect_splits(dataset_id, *, version, split_config, client_factory=None)``
returns ``dict[str, SplitSummary]`` (``group_count``, ``episode_count``).

.. autoclass:: foxglove.datasets.DatasetSplit

.. autofunction:: foxglove.datasets.inspect_splits

.. autoclass:: foxglove.datasets.SplitSummary

Object storage
--------------

``source="object_storage"`` reads recordings whose ``location.scheme`` is
``s3``, ``gs``, or ``az`` (Azure also needs ``azure_storage_account_name``),
using each worker's cloud credentials. Objects must be indexed MCAP and must
not change during a read. For other stores, pass ``object_store_factory``.

.. autoclass:: foxglove.datasets.ObjectLocation
   :members:

.. autoclass:: foxglove.datasets.ObjectStore
   :members:

Video
-----

Optional extra ``foxglove-sdk[video]``.
``decode_h264(episode, *, topic=None, lookback=timedelta(seconds=5), decoder_factories=None)``
yields ``VideoSample`` dicts from ``foxglove.CompressedVideo`` H.264 Annex B;
pass it directly as ``read_episode``. ``to_image_tensor(sample)`` takes a
mapping whose ``image`` is ``[..., H, W, 3]`` and returns ``[..., 3, H, W]``.

.. autofunction:: foxglove.datasets.video.decode_h264

.. autoclass:: foxglove.datasets.video.VideoSample
   :members:

.. autoexception:: foxglove.datasets.video.VideoDecodeError

.. autofunction:: foxglove.datasets.torch.to_image_tensor

Message encodings
-----------------

``iter_messages`` decodes JSON. Install ``mcap-protobuf-support``,
``mcap-ros1-support``, or ``mcap-ros2-support`` on every worker to decode
those encodings.
