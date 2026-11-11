Integrate with a labeling provider
==================================

Keep annotating in your labeling tool. Export a committed dataset version, send
the files to the tool, write the results back as episode custom properties, and
commit a new dataset version. The export contains the episode versions that
dataset version pins.

**Prerequisites:** ``foxglove-client>=0.23`` or Foxglove CLI v2.1.0+; an API key
with ``datasets.list``, ``datasets.update``, ``episodes.update``, and
``data.stream``; an admin to define property keys.

Define property keys first
--------------------------

An admin creates keys in **Settings → Episodes** before anyone can set values.
Each key has a type: Single-line text, Multi-line text, Number, True/False,
Single-select, or Multi-select. Via the API: ``POST /custom-properties`` with
``resourceType: "episode"``, or
``client.create_custom_property(resource_type="episode", ...)``. Keys are 1–32
characters, ``[A-Za-z0-9_-]``.

Use typed properties (Single-select, Number, True/False) for anything you will
filter on. Use Multi-line text for free-form notes.

Export
------

Use the UI **Use for training** dialog, the CLI, or the Python client. Each
path writes one MCAP per episode plus ``manifest.json``. The manifest
identifies the dataset version and the episodes in it.

CLI (v2.1.0 or later, signed in to the organization)::

   foxglove datasets download dset_123 --version 2 --output ./training-data --include-properties

Python:

.. code-block:: python

   import os
   from pathlib import Path
   from foxglove.client import Client

   client = Client(token=os.environ["FOXGLOVE_API_TOKEN"])
   client.download_dataset(
       dataset_id="dset_123",
       version_number=2,
       output_directory=Path("training-data"),
       include_properties=True,
   )

``include_properties=True`` adds each episode's custom properties to the
manifest. ``topics`` selects topics; omit it to include every topic.
``output_directory`` must be empty or not exist. The call raises
``RuntimeError`` for an editable version or one with unavailable recordings.

Send the MCAP files (and the manifest, so you can map files back to episode
IDs) to the labeling tool. Use the manifest's episode ``id`` and ``version`` as
the join key with the labeling tool.

Round-trip
----------

Write the labels back. Keys you omit keep their values; ``None`` unsets a key.
A change creates a new episode version:

.. code-block:: python

   result = client.update_episodes(
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
   new_version = result[0]["version"]

``applies_to_version`` makes the call fail with 409 if the episode has a newer
version; the response lists ``conflicts`` with each ``currentVersion``. The
call is atomic and accepts up to 2,000 episodes.

Then point the dataset draft at the labeled episode versions and commit:

.. code-block:: python

   client.update_dataset_episodes(
       dataset_id="dset_123",
       update=[{"id": r["id"], "version": r["version"]} for r in result],
   )
   client.commit_dataset(
       dataset_id="dset_123", description="Labels from review batch 12"
   )

**Result:** ``get_dataset_version_episodes(..., include_properties=True)`` on
the new version returns the labeled properties.
