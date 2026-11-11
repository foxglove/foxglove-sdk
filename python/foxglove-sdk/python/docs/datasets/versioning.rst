Version and reproduce a training set
====================================

A committed dataset version is a record of which episodes, and which episode
versions, a run used. The draft is the only editable membership. Commit
writes a new version and opens the next draft. Later episode edits do not
change pins on older versions.

Commit
------

.. code-block:: python

   from foxglove.client import Client

   client = Client(token="...")
   client.update_dataset_episodes(
       dataset_id="ds_123",
       add=["ep_new"],
       remove=["ep_old"],
       update=[{"id": "ep_keep", "version": 4}],
   )
   result = client.commit_dataset(
       dataset_id="ds_123",
       description="Demos after labeling batch 12",
   )
   print(result["committed"]["version_number"], result["editable_version_number"])

``commit_dataset`` returns HTTP 409 when the draft has no changes.
``discard_dataset`` resets the draft to the last committed version.

CLI::

   foxglove datasets commit ds_123 --description "Demos after labeling batch 12"

In the UI, open **Review changes**, optionally describe the version, and
commit.

List, get, compare
------------------

.. code-block:: python

   for version in client.get_dataset_versions(dataset_id="ds_123").auto_paging_iter():
       print(version["version_number"], version["episode_count"], version["committed_at"])

   v = client.get_dataset_version(dataset_id="ds_123", version_number=2)
   print(v["added_episode_count"], v["removed_episode_count"], v["changed_episode_count"])

   diff = client.compare_dataset_versions(
       dataset_id="ds_123",
       version_number=2,
       base_version=1,
   )
   print(diff["added_count"], diff["removed_count"], diff["changed_count"])

``compare_dataset_versions`` takes the target as ``version_number`` and the
base as ``base_version``. Change kinds are ``added``, ``removed``, and
``changed``. A changed row includes ``previous_version``.

CLI::

   foxglove datasets versions compare ds_123 1 2

Restore
-------

Restore copies a previous version's membership into the draft. It does not
commit. Pending draft changes are discarded unless you need ``force=True``.

.. code-block:: python

   client.restore_dataset_version(dataset_id="ds_123", version_number=1)
   client.commit_dataset(dataset_id="ds_123", description="Restore version 1")

CLI::

   foxglove datasets versions restore ds_123 1
   foxglove datasets commit ds_123 --description "Restore version 1"

In the UI, open the earlier version and choose **Restore this version**.

Clone
-----

Clone creates a new dataset from a committed version. Clone is available in
the UI after the first commit. Draft changes on the source are not included.
The copy starts at version 1. ``include_history=True`` copies every version
through the selected one, including descriptions and commit details.

.. code-block:: python

   copy = client.clone_dataset_version(
       dataset_id="ds_123",
       version_number=2,
       name="eval-holdout",
       include_history=False,
   )
   print(copy["id"], copy["episode_count"])

CLI::

   foxglove datasets versions clone ds_123 2 --name eval-holdout

Record ``dataset_id`` and ``version_number`` next to checkpoints. Retrain by
pointing ``read_dataset`` at that pair. See :doc:`python-client` and
:doc:`rest`.
