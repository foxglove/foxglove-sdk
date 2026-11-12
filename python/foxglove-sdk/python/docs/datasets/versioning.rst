Version and reproduce a training set
====================================

A committed dataset version records which episodes, at which episode versions,
a training run used. You edit only the draft. Committing saves the draft as a
new version and opens a new draft. Requires ``foxglove-client>=0.23``. CLI
examples need v2.1.0 or later.

Commit
------

.. code-block:: python

   import os
   from foxglove.client import Client

   client = Client(token=os.environ["FOXGLOVE_API_TOKEN"])
   client.update_dataset_episodes(
       dataset_id="dset_123",
       add=["ep_new"],
       remove=["ep_old"],
       update=[{"id": "ep_keep", "version": 4}],
   )
   result = client.commit_dataset(
       dataset_id="dset_123",
       description="Demos after labeling batch 12",
   )
   print(result["committed"]["version_number"], result["editable_version_number"])

``commit_dataset`` raises ``requests.HTTPError`` (409) when the draft has no
changes. Set ``description`` here, or in **Review changes**. Omitting it keeps
the draft's description; ``None`` clears it. ``discard_dataset`` resets the
draft to the last committed version.

CLI (v2.1.0 or later)::

   foxglove datasets commit dset_123

List, get, compare
------------------

.. code-block:: python

   for version in client.get_dataset_versions(dataset_id="dset_123").auto_paging_iter():
       print(version["version_number"], version["episode_count"], version["committed_at"])

   v = client.get_dataset_version(dataset_id="dset_123", version_number=2)
   print(v["added_episode_count"], v["removed_episode_count"], v["changed_episode_count"])

   diff = client.compare_dataset_versions(
       dataset_id="dset_123",
       version_number=2,
       base_version=1,
   )
   print(diff["added_count"], diff["removed_count"], diff["changed_count"])

The list includes the draft; its ``committed_at`` is ``None``.
``compare_dataset_versions`` takes the target as ``version_number`` and the
base as ``base_version``. Change kinds are ``added``, ``removed``, and
``changed``. A changed row includes ``previous_version``.

CLI (from version 1 to 2)::

   foxglove datasets versions compare dset_123 1 2

Restore
-------

Restore stages a previous version's episodes and episode versions in the draft.
It does not commit. If the draft has pending changes, restore raises
``requests.HTTPError`` (409); pass ``force=True`` to discard them and restore.

.. code-block:: python

   client.restore_dataset_version(dataset_id="dset_123", version_number=1)
   client.commit_dataset(dataset_id="dset_123", description="Restore version 1")

``commit_dataset`` after restore raises ``HTTPError`` (409) if the restored
membership equals the last committed version (nothing to commit).

CLI::

   foxglove datasets versions restore dset_123 1
   foxglove datasets versions restore dset_123 1 --force
   foxglove datasets commit dset_123

In Foxglove, open the earlier version and choose **Restore this version**. This
replaces any pending draft changes.

Clone
-----

Clone creates a new dataset in the same Project. Without
``include_history``, its only committed version is version 1. With
``include_history=True``, versions 1 through the cloned version keep their
numbers, descriptions, and commit details. Either way an empty draft opens on
top. Needs ``datasets.create``; raises ``HTTPError`` (409) if the name is
taken in the project, and 400 ``DatasetSpansPrimarySites`` if recordings span
Primary Sites.

.. code-block:: python

   clone = client.clone_dataset_version(
       dataset_id="dset_123",
       version_number=2,
       name="eval-holdout",
       include_history=False,
   )
   print(clone["id"], clone["episode_count"])

Save ``dataset_id``, ``version_number``, and any ``DatasetSplit`` and shuffle
``seed`` with each checkpoint. To retrain on the same data, pass the same
values to ``read_dataset``. See :doc:`python-client` and :doc:`rest`.
