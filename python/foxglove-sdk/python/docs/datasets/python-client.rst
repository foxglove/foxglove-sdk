Python client reference
=======================

Package ``foxglove-client``. Import ``from foxglove.client import Client``.
Methods map 1:1 to the REST resources in :doc:`rest`. Keys in returned dicts
are snake_case. Times that the API stores with nanoseconds also appear as
``raw_start_time`` / ``raw_end_time`` RFC 3339 strings.

Authenticate with ``Client(token=...)``. The client sends
``Authorization: Bearer <token>``.

Count fields on dataset and version objects are listed with those methods.

Datasets
--------

create_dataset
^^^^^^^^^^^^^^

Create a dataset, optionally seeding the first version with episodes.

Inputs:

``project_id`` (str, required)
    Project that owns the dataset.

``name`` (str, required)
    1–100 characters. Unique within the project, case-insensitive.

``description`` (str, optional)
    1–500 characters.

``episode_ids`` (list of str, optional)
    At most 2,000 episode IDs.

Returns a dataset dict (``id``, ``project_id``, ``name``, ``description``,
``creator_org_member_id``, ``creator_api_key_id``, ``created_at``,
``updated_at``, ``episode_count``, and seed counts ``added``, ``updated``,
``removed``, ``already_present`` when episodes were supplied).

.. code-block:: python

   dataset = client.create_dataset(
       project_id="prj_123", name="Training candidates", episode_ids=["ep_1"]
   )

get_datasets
^^^^^^^^^^^^

List datasets. Optional ``project_id``, ``name`` (case-insensitive substring),
``sort_by`` (``name``, ``created_at``, ``updated_at``, ``episode_count``;
default ``created_at``), ``sort_order``, ``limit`` (0–2000, default 2000),
``cursor``. Returns a ``Page``. Use ``page.auto_paging_iter()``.

get_dataset
^^^^^^^^^^^

Return one dataset, including ``episode_count``. Input: ``dataset_id`` (str,
required).

update_dataset
^^^^^^^^^^^^^^

Patch ``name`` and/or ``description``. ``description=None`` clears it.

delete_dataset
^^^^^^^^^^^^^^

Delete the dataset. Does not delete episodes. Input: ``dataset_id``.

get_dataset_episodes
^^^^^^^^^^^^^^^^^^^^

Page of membership on the latest committed version, or the initial draft if
none is committed.

Optional filters: ``sort_by`` (``added_at``, ``created_at``, ``start_time``,
``end_time``), ``sort_order``, ``limit``, ``cursor``, ``start`` and ``end``
together, ``has_missing_recordings``, ``recording_id``, ``properties_query``,
``include_recordings``, ``include_properties``, ``include_creation_context``.

Each item has ``added_at``, ``added_in_version``, ``has_missing_recordings``,
and nested ``episode``.

update_dataset_episodes
^^^^^^^^^^^^^^^^^^^^^^^

Stage draft membership. Does not commit.

``add`` / ``update``
    List of episode IDs, or dicts ``{"id": ..., "version": ...}``. Omitting
    ``version`` uses the latest episode version. At most 2,000 each.

``remove``
    Episode IDs to drop. At most 2,000.

Returns ``added``, ``updated``, ``removed``, ``already_present`` (ints).

get_dataset_versions
^^^^^^^^^^^^^^^^^^^^

Page of versions (committed plus the current draft). Optional ``sort_order``,
``limit``, ``cursor``.

get_dataset_version
^^^^^^^^^^^^^^^^^^^

One version, including ``has_missing_recordings`` and the count fields
``episode_count``, ``added_episode_count``, ``removed_episode_count``,
``changed_episode_count``. Inputs: ``dataset_id``, ``version_number`` (int).
``committed_at`` is omitted or null on the editable version.

update_dataset_version
^^^^^^^^^^^^^^^^^^^^^^

Set ``description`` (str or ``None``) on a committed or editable version.
Does not change episodes or commit attribution.

get_dataset_version_episodes
^^^^^^^^^^^^^^^^^^^^^^^^^^^^

Same filters as ``get_dataset_episodes``, for a specific ``version_number``.
Episodes are returned at the pinned episode version.

compare_dataset_versions
^^^^^^^^^^^^^^^^^^^^^^^^

Membership changes from ``base_version`` to ``version_number``.

Returns ``changes``, ``added_count``, ``removed_count``, ``changed_count``,
``next_cursor``. Each change has a kind ``added``, ``removed``, or
``changed``. Changed rows include ``previous_version`` and
``previous_episode`` when present. Optional ``include_*`` flags as on list
calls.

.. code-block:: python

   diff = client.compare_dataset_versions(
       dataset_id="ds_123", version_number=2, base_version=1
   )

commit_dataset
^^^^^^^^^^^^^^

Commit the draft. Optional ``description``. Returns ``committed`` (version
dict) and ``editable_version_number``. HTTP 409 when there are no changes.

discard_dataset
^^^^^^^^^^^^^^^

Drop draft adds, removes, and updates. Returns ``discarded_adds``,
``discarded_removes``, ``discarded_updates``.

restore_dataset_version
^^^^^^^^^^^^^^^^^^^^^^^

Stage membership so the draft matches ``version_number``. Does not commit.
``force`` (bool, default False) discards conflicting draft changes. Returns
``added``, ``updated``, ``removed``, ``discarded_adds``,
``discarded_removes``, ``discarded_updates``.

clone_dataset_version
^^^^^^^^^^^^^^^^^^^^^

Create a new dataset from a committed version. Required ``name``. Optional
``description``, ``include_history`` (bool). Returns the new dataset dict.
The copy has an empty editable version on top.

download_dataset
^^^^^^^^^^^^^^^^

Export a committed version to one MCAP per episode plus ``manifest.json``.
Inputs: ``dataset_id``, ``version_number``, ``output_directory``, optional
``topics``, ``include_properties`` (default False). Returns a ``Path``.
Raises if the version is editable or any member has missing recordings.
Failed episode transfers are recorded in the manifest;
``DatasetDownloadWarning`` is emitted if some episodes fail.

Episodes
--------

create_episodes
^^^^^^^^^^^^^^^

Get-or-create up to 2,000 episodes. Matching uses time range, recordings, and
custom properties of the latest episode version.

Each item must have exactly one of ``recordings`` or ``event_id``. Recording
items may set ``start_time`` / ``end_time`` (datetime or RFC 3339 string).
``event_id`` snapshots the event's current range and overlapping completed
recordings; later event edits do not change the episode. Optional
``properties`` must use keys defined for ``resourceType=episode``.

Returns the ``episodes`` list from the response. New episodes count toward
the episode meter (:doc:`pricing`). Matched episodes do not.

.. code-block:: python

   episodes = client.create_episodes(
       project_id="prj_123",
       episodes=[{"recordings": ["rec_1"], "properties": {"task": "fold_towel"}}],
   )

update_episodes
^^^^^^^^^^^^^^^

Atomic bulk update. Each item requires ``id``. Optional
``applies_to_version``, ``start_time``, ``end_time``, ``recordings``
(``{"add": [...], "remove": [...]}``), ``properties`` (merge; ``None``
unsets a key). Unchanged episodes keep their version.

get_episodes
^^^^^^^^^^^^

Page of episodes. Optional ``project_id``, ``start`` and ``end`` together,
``has_missing_recordings``, ``recording_id``, ``exact_recording_ids``
(non-empty list), ``properties_query``, ``sort_by`` (``created_at``,
``start_time``, ``end_time``), ``sort_order``, ``limit``, ``cursor``,
``include_recordings``, ``include_properties``,
``include_creation_context``.

get_episode
^^^^^^^^^^^

One episode. Optional ``version``, ``include_recordings``,
``include_properties``. Always includes ``creation_context``.

update_episode
^^^^^^^^^^^^^^

Single-episode form of ``update_episodes``. Returns the episode at the
resulting version.

delete_episode
^^^^^^^^^^^^^^

Delete if no dataset holds the episode. HTTP 409 otherwise.

get_episode_versions
^^^^^^^^^^^^^^^^^^^^

Page of episode versions, newest first. Each row has ``version``,
``created_at``, creators, and ``changed_fields``.

Custom properties
-----------------

``create_custom_property``, ``get_custom_properties``, ``get_custom_property``,
``update_custom_property``, ``delete_custom_property``. Episode keys use
``resource_type="episode"``. ``value_type`` is ``string``,
``multiline-string``, ``number``, ``enum``, ``boolean``, or ``multi-enum``.
