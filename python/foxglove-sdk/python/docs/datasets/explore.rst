Explore a dataset
=================

Inspect a committed dataset version in the Foxglove UI, then the same
selection from Python. Both views use the episode versions pinned by that
dataset version.

In the UI
---------

Open the dataset and pick a version from the breadcrumb menu. The gallery
lists member episodes. Click a card for properties, recordings, and version
or dataset history.

On the **Episodes** page, switch gallery and table views, choose preview
topics, and filter with **Filter → Properties**. Every ``key:value`` term
must match. Quote values that contain spaces::

   outcome:success task:"fold towel"

In Search, the same filters use FoxQL::

   @episode.properties.task == fold_towel and @episode.properties.outcome == success

The **Episodes** tab in the visualization right sidebar lists episodes on the
loaded recordings, sorted by start time. Click an episode to seek to its
start. **Edit episode** has **Time range**, **Recordings**, and **Properties**
tabs.

Open **Compare versions** on a dataset to see additions, removals, and
episode version updates. **Version updated** means the episode ID is the same
but its pinned version changed.

In Python
---------

Install ``foxglove-client``. Create an API key with ``datasets.list`` (and
``episodes.list`` if you query episodes outside a dataset). Set
``FOXGLOVE_API_TOKEN``.

List membership for a committed version, including custom properties:

.. code-block:: python

   import os
   from foxglove.client import Client

   client = Client(token=os.environ["FOXGLOVE_API_TOKEN"])

   dataset = client.get_dataset(dataset_id="ds_123")
   print(dataset["name"], dataset["episode_count"])

   version = client.get_dataset_version(dataset_id="ds_123", version_number=2)
   print(version["version_number"], version["episode_count"], version["committed_at"])

   page = client.get_dataset_version_episodes(
       dataset_id="ds_123",
       version_number=2,
       include_properties=True,
       properties_query='outcome:success',
       limit=100,
   )
   for member in page.auto_paging_iter():
       episode = member["episode"]
       print(episode["id"], episode["version"], episode.get("properties"))

``properties`` is omitted unless you pass ``include_properties=True``.
``get_episode`` always includes ``creation_context``. List calls need
``include_creation_context=True`` for that field.

Stream decoded messages for one pinned episode. Times on the episode object
are datetimes (microsecond precision). ``raw_start_time`` and
``raw_end_time`` are the API's RFC 3339 strings and keep nanoseconds:

.. code-block:: python

   episode = client.get_episode(
       episode_id="ep_123",
       version=3,
       include_properties=True,
       include_recordings=True,
   )
   for schema, channel, message, decoded in client.iter_messages(
       episode_id=episode["id"],
       episode_version=episode["version"],
       topics=["/camera/front", "/joint_states"],
   ):
       print(channel.topic, message.log_time, type(decoded))

JSON (``jsonschema``) decodes by default. For protobuf, ROS 1, or ROS 2,
install ``mcap-protobuf-support``, ``mcap-ros1-support``, or
``mcap-ros2-support`` and pass decoder factories if you are not using the
client defaults.

Dataset and version responses include ``episode_count``. Version responses
also include ``added_episode_count``, ``removed_episode_count``,
``changed_episode_count``, and ``has_missing_recordings``.

See :doc:`python-client` and :doc:`rest` for parameters. For PyTorch or Ray
iteration over the whole version, see :doc:`pytorch` and :doc:`ray`.
