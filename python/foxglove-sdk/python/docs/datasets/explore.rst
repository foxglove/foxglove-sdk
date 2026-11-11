Explore a dataset
=================

Inspect a committed dataset version in Foxglove or from Python. Both show the
episode versions that version pins.

UI: https://docs.foxglove.dev/docs/data/datasets

In Python
---------

**Prerequisites:** ``pip install 'foxglove-client>=0.23'``; an API key with
``datasets.list``, ``episodes.list`` (for ``get_episode``), and
``data.stream`` (for ``iter_messages``), exported as ``FOXGLOVE_API_TOKEN``.

List membership for a committed version, including custom properties:

.. code-block:: python

   import os
   from foxglove.client import Client

   client = Client(token=os.environ["FOXGLOVE_API_TOKEN"])

   dataset = client.get_dataset(dataset_id="dset_123")
   print(dataset["name"], dataset["episode_count"])

   version = client.get_dataset_version(dataset_id="dset_123", version_number=2)
   print(version["version_number"], version["episode_count"], version["committed_at"])

   page = client.get_dataset_version_episodes(
       dataset_id="dset_123",
       version_number=2,
       include_properties=True,
       properties_query="outcome:success",
       limit=100,
   )
   for member in page.auto_paging_iter():
       episode = member["episode"]
       print(episode["id"], episode["version"], episode.get("properties"))

Pass ``include_properties=True`` to return ``properties``. ``get_episode``
always returns ``creation_context``; list calls return it with
``include_creation_context=True``. ``creation_context`` keeps the API's
camelCase keys (``recordingIds``).

**Result:** lines such as ``Training candidates 120`` and
``2 120 2026-11-02 18:04:11+00:00``.

Stream decoded messages for one pinned episode:

.. code-block:: python

   episode = client.get_episode(episode_id="ep_123", version=3)
   for schema, channel, message, decoded in client.iter_messages(
       episode_id=episode["id"],
       episode_version=episode["version"],
       topics=["/camera/front", "/joint_states"],
   ):
       print(channel.topic, message.log_time, type(decoded))

``iter_messages`` decodes JSON. Install ``mcap-protobuf-support``,
``mcap-ros1-support``, or ``mcap-ros2-support`` to decode those encodings; the
client registers installed decoders automatically.

Next: :doc:`pytorch`, :doc:`ray`.
