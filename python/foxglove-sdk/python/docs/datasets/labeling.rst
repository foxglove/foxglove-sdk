Integrate with a labeling provider
==================================

Export a committed dataset version, send the files to your labeling tool, then
write labels back as episode custom properties and commit a new dataset
version. Foxglove does not copy recordings when you add episodes to a dataset,
so the export is a snapshot of the pinned episode versions.

Define property keys first
--------------------------

An admin creates keys in **Settings → Episodes** before anyone can set values.
Each key has a type: Single-line text, Multi-line text, Number, True/False,
Single-select, or Multi-select. The REST ``resourceType`` for these keys is
``episode``. See :doc:`rest`.

Export
------

Use the UI **Use for training** dialog, the CLI, or the Python client. Each
path writes one MCAP per episode plus ``manifest.json``. The manifest
identifies the dataset version and the episodes in it.

CLI (Foxglove CLI v2.0.0 or later, signed in to the organization)::

   foxglove datasets download ds_123 --version 2 --output ./training-data --include-properties

Python:

.. code-block:: python

   from pathlib import Path
   from foxglove.client import Client

   client = Client(token="...")
   client.download_dataset(
       dataset_id="ds_123",
       version_number=2,
       output_directory=Path("training-data"),
       include_properties=True,
   )

``include_properties=True`` adds each episode's custom properties to the
manifest. ``topics`` selects topics; omit it to include every topic. The
version must be committed and must not have missing recordings.

Send the MCAP files (and the manifest, so you can map files back to episode
IDs) to the labeling tool. Keep the episode ID and episode version from the
manifest as the join key. Do not invent a parallel ID.

Round-trip
----------

When labels come back, merge them onto the latest episode version. Omitted
property keys keep their values. Pass ``None`` to unset a key. This creates a
new episode version when anything changes:

.. code-block:: python

   client.update_episodes(
       episodes=[
           {
               "id": "ep_123",
               "applies_to_version": 3,
               "properties": {
                   "outcome": "success",
                   "reviewer_notes": "grasp slipped on frame 40",
               },
           }
       ]
   )

``applies_to_version`` rejects the update if the episode moved on. Bulk
``update_episodes`` is atomic: if any item fails, none apply. Maximum 2,000
episodes per call.

Then point the dataset draft at the labeled episode versions and commit:

.. code-block:: python

   client.update_dataset_episodes(
       dataset_id="ds_123",
       update=[{"id": "ep_123", "version": 4}],
   )
   client.commit_dataset(dataset_id="ds_123", description="Labels from review batch 12")

Recommendations
---------------

Keep the labeling tool's file name or asset ID next to the Foxglove episode
ID in your own table. The Foxglove episode ID is what ``update_episodes``
accepts.

Use typed properties (Single-select, Number, True/False) for anything you
will filter on. Use Multi-line text for free-form notes.

Filter after import with ``propertiesQuery`` (same ``key:value`` syntax as
**Filter → Properties**) or with ``@episode.properties.<key>`` in Search.

Commit a dataset version after each labeling batch so training runs can pin
that batch. Compare versions to see which episodes were added, removed, or
re-pinned. See :doc:`versioning`.

.. note::

   A blog post on collecting and labeling robot data will link from here once
   it is published.
