Python client
=============

``foxglove-client>=0.23``. ``from foxglove.client import Client``.
``Client(token, host="api.foxglove.dev")``. Keys are snake_case except
``creation_context`` and recording ``location``. Time inputs are
timezone-aware ``datetime``. A naive ``datetime`` is read as local time;
pass a timezone-aware ``datetime``. Returned times are ``datetime``, except
``creation_context``, which keeps the API strings.
API errors raise ``requests.exceptions.HTTPError``. Argument errors raise
``RuntimeError`` (``iter_messages``) and ``ValueError`` (``create_episodes``).

Datasets
--------

create_dataset
^^^^^^^^^^^^^^

Create a dataset, optionally seeding the first version with episodes.

Parameters:

* ``project_id`` (str, required) — Project that owns the dataset.
* ``name`` (str, required) — 1–100 characters. Unique in the Project, case-insensitive.
* ``description`` (str, optional, default None) — 1–500 characters.
* ``episode_ids`` (list of str, optional, default None) — At most 2,000 episode IDs.

Returns a dataset dict (``id``, ``project_id``, ``name``, ``description``, ``creator_org_member_id``, ``creator_api_key_id``, ``created_at``, ``updated_at``, ``episode_count``, and seed counts ``added``, ``updated``, ``removed``, ``already_present`` when episodes were supplied).

.. code-block:: python

   dataset = client.create_dataset(
       project_id="prj_123", name="Training candidates", episode_ids=["ep_1"]
   )

get_datasets
^^^^^^^^^^^^

List datasets.

Parameters:

* ``project_id`` (str, optional, default None) — Restrict to one Project.
* ``name`` (str, optional, default None) — Case-insensitive substring.
* ``sort_by`` (str, optional, default ``created_at``) — ``name``, ``created_at``, ``updated_at``, ``episode_count``.
* ``sort_order`` (str, optional, default None) — ``asc`` or ``desc``.
* ``limit`` (int, optional, default 2000) — Page size, 0–2000.
* ``cursor`` (str, optional, default None) — Cursor from a previous page.

Returns a ``Page``. Iterate with ``page.auto_paging_iter()``.

.. code-block:: python

   for d in client.get_datasets(project_id="prj_123").auto_paging_iter():
       print(d["id"], d["name"], d["episode_count"])

get_dataset
^^^^^^^^^^^

Return one dataset.

Parameters:

* ``dataset_id`` (str, required) — Dataset ID.

Returns a dataset dict including ``episode_count``.

.. code-block:: python

   dataset = client.get_dataset(dataset_id="dset_123")

update_dataset
^^^^^^^^^^^^^^

Patch ``name`` and/or ``description``.

Parameters:

* ``dataset_id`` (str, required) — Dataset ID.
* ``name`` (str, optional, default unchanged) — 1–100 characters.
* ``description`` (str or None, optional, default unchanged) — 1–500, or ``None`` to clear.

Returns the updated dataset dict.

.. code-block:: python

   client.update_dataset(dataset_id="dset_123", description=None)

delete_dataset
^^^^^^^^^^^^^^

Delete the dataset. Does not delete episodes.

Parameters:

* ``dataset_id`` (str, required) — Dataset ID.

Returns ``None``.

.. code-block:: python

   client.delete_dataset(dataset_id="dset_123")

get_dataset_episodes
^^^^^^^^^^^^^^^^^^^^

Page of membership on the latest committed version, or the initial draft if none is committed.

Parameters:

* ``dataset_id`` (str, required) — Dataset ID.
* ``sort_by`` (str, optional, default ``added_at``) — ``added_at``, ``created_at``, ``start_time``, ``end_time``.
* ``sort_order`` (str, optional, default None) — ``asc`` or ``desc``.
* ``limit`` (int, optional, default 2000) — Page size, 0–2000.
* ``cursor`` (str, optional, default None) — Cursor from a previous page.
* ``start`` / ``end`` (datetime (timezone-aware), optional, default None) — Overlap filter. Pass both.
* ``has_missing_recordings`` (bool, optional, default None) — Filter by recording availability.
* ``recording_id`` (str, optional, default None) — Episodes containing this recording.
* ``properties_query`` (str, optional, default None) — ``key:value`` terms, ANDed.
* ``include_recordings`` (bool, optional, default False) — Add ``recordings``.
* ``include_properties`` (bool, optional, default False) — Add ``properties``.
* ``include_creation_context`` (bool, optional, default False) — Add ``creation_context``.

Returns ``Page`` of dicts: ``added_at`` (datetime), ``added_in_version`` (int), ``has_missing_recordings`` (bool), ``episode`` (dict). Iterate with ``page.auto_paging_iter()``.

.. code-block:: python

   page = client.get_dataset_episodes(dataset_id="dset_123")

update_dataset_episodes
^^^^^^^^^^^^^^^^^^^^^^^

Stage draft membership. Does not commit. An ID in ``update`` cannot also
be in ``add`` or ``remove``. An object in ``add`` must not already be in
the draft. At least one of ``add``, ``update``, or ``remove`` is required.

Parameters:

* ``dataset_id`` (str, required) — Dataset ID.
* ``add`` (list, optional, default None) — IDs or ``{"id", "version?"}``. Latest version if omitted. ≤ 2000.
* ``update`` (list, optional, default None) — Same forms as ``add``. ≤ 2000.
* ``remove`` (list of str, optional, default None) — Episode IDs to drop. ≤ 2000.

Returns ``added``, ``updated``, ``removed``, ``already_present`` (ints).

.. code-block:: python

   client.update_dataset_episodes(
       dataset_id="dset_123", add=["ep_new"], remove=["ep_old"]
   )

get_dataset_versions
^^^^^^^^^^^^^^^^^^^^

Page of versions (committed plus the current draft).

Parameters:

* ``dataset_id`` (str, required) — Dataset ID.
* ``sort_order`` (str, optional, default None) — ``asc`` or ``desc``.
* ``limit`` (int, optional, default 2000) — Page size, 0–2000.
* ``cursor`` (str, optional, default None) — Cursor from a previous page.

Returns a ``Page`` of version dicts. Draft ``committed_at`` is ``None``.

.. code-block:: python

   versions = client.get_dataset_versions(dataset_id="dset_123")

get_dataset_version
^^^^^^^^^^^^^^^^^^^

Return one version.

Parameters:

* ``dataset_id`` (str, required) — Dataset ID.
* ``version_number`` (int, required) — Dataset version number.

Returns a version dict including ``has_missing_recordings`` and the count fields.

.. code-block:: python

   v = client.get_dataset_version(dataset_id="dset_123", version_number=2)

update_dataset_version
^^^^^^^^^^^^^^^^^^^^^^

Set ``description`` on a committed or editable version. Does not change episodes or commit attribution.

Parameters:

* ``dataset_id`` (str, required) — Dataset ID.
* ``version_number`` (int, required) — Dataset version number.
* ``description`` (str or None, required) — New description, or ``None`` to clear.

Returns the version dict.

.. code-block:: python

   client.update_dataset_version(dataset_id="dset_123", version_number=2, description=None)

get_dataset_version_episodes
^^^^^^^^^^^^^^^^^^^^^^^^^^^^

List the episodes in one dataset version, at the episode versions it pins.

Parameters:

* ``dataset_id`` (str, required) — Dataset ID.
* ``version_number`` (int, required) — Dataset version number.
* ``sort_by`` (str, optional, default ``added_at``) — ``added_at``, ``created_at``, ``start_time``, ``end_time``.
* ``sort_order`` (str, optional, default None) — ``asc`` or ``desc``.
* ``limit`` (int, optional, default 2000) — Page size, 0–2000.
* ``cursor`` (str, optional, default None) — Cursor from a previous page.
* ``start`` / ``end`` (datetime (timezone-aware), optional, default None) — Overlap filter. Pass both.
* ``has_missing_recordings`` (bool, optional, default None) — Filter by recording availability.
* ``recording_id`` (str, optional, default None) — Episodes containing this recording.
* ``properties_query`` (str, optional, default None) — ``key:value`` terms, ANDed.
* ``include_recordings`` (bool, optional, default False) — Add ``recordings``.
* ``include_properties`` (bool, optional, default False) — Add ``properties``.
* ``include_creation_context`` (bool, optional, default False) — Add ``creation_context``.

Returns ``Page`` of dicts: ``added_at`` (datetime), ``added_in_version`` (int), ``has_missing_recordings`` (bool), ``episode`` (dict). Iterate with ``page.auto_paging_iter()``.

.. code-block:: python

   for m in client.get_dataset_version_episodes(
       dataset_id="dset_123", version_number=2
   ).auto_paging_iter():
       print(m["episode"]["id"], m["episode"]["version"])

compare_dataset_versions
^^^^^^^^^^^^^^^^^^^^^^^^

Membership changes from ``base_version`` to ``version_number``.

Parameters:

* ``dataset_id`` (str, required) — Dataset ID.
* ``version_number`` (int, required) — Target version.
* ``base_version`` (int, required) — Base version.
* ``limit`` (int, optional, default 2000) — Page size, 0–2000.
* ``cursor`` (str, optional, default None) — Cursor from a previous page.
* ``include_recordings`` (bool, optional, default False) — Add ``recordings``.
* ``include_properties`` (bool, optional, default False) — Add ``properties``.
* ``include_creation_context`` (bool, optional, default False) — Add ``creation_context``.

Returns ``changes``, ``added_count``, ``removed_count``, ``changed_count``,
``next_cursor``. Changed rows include ``previous_version`` and
``previous_episode``.

.. code-block:: python

   diff = client.compare_dataset_versions(
       dataset_id="dset_123", version_number=2, base_version=1
   )

commit_dataset
^^^^^^^^^^^^^^

Commit the draft.

Parameters:

* ``dataset_id`` (str, required) — Dataset ID.
* ``description`` (str or None, optional, default unchanged) — 1–2048, or ``None`` to clear.
  Omitting ``description`` keeps the draft's description.

Returns ``committed`` (version dict) and ``editable_version_number``.

Raises: ``HTTPError`` (409) when the draft has no changes.

.. code-block:: python

   client.commit_dataset(dataset_id="dset_123", description="Labels batch 12")

discard_dataset
^^^^^^^^^^^^^^^

Drop draft adds, removes, and updates.

Parameters:

* ``dataset_id`` (str, required) — Dataset ID.

Returns ``discarded_adds``, ``discarded_removes``, ``discarded_updates``.

.. code-block:: python

   client.discard_dataset(dataset_id="dset_123")

restore_dataset_version
^^^^^^^^^^^^^^^^^^^^^^^

Stage membership so the draft matches ``version_number``. Does not commit.

Parameters:

* ``dataset_id`` (str, required) — Dataset ID.
* ``version_number`` (int, required) — Committed version to restore.
* ``force`` (bool, optional, default False) — Discard pending draft changes and restore.

Returns ``added``, ``updated``, ``removed``, ``discarded_adds``, ``discarded_removes``, ``discarded_updates``.

Raises: ``HTTPError`` (409) if the draft has pending changes and ``force`` is false.

.. code-block:: python

   client.restore_dataset_version(dataset_id="dset_123", version_number=1)

clone_dataset_version
^^^^^^^^^^^^^^^^^^^^^

Create a new dataset from a committed version.

Parameters:

* ``dataset_id`` (str, required) — Source dataset ID.
* ``version_number`` (int, required) — Committed version to clone.
* ``name`` (str, required) — 1–100, unique in the Project.
* ``description`` (str, optional, default None) — 1–500.
* ``include_history`` (bool, optional, default False) — Copy versions 1 through the cloned version, keeping their numbers.

Returns the new dataset dict.

Raises: ``HTTPError`` (409) if the name is taken; 400 ``DatasetSpansPrimarySites`` if recordings span Primary Sites.

.. code-block:: python

   client.clone_dataset_version(dataset_id="dset_123", version_number=2, name="eval-holdout")

download_dataset
^^^^^^^^^^^^^^^^

Export a committed version to one MCAP per episode plus ``manifest.json``.

Parameters:

* ``dataset_id`` (str, required) — Dataset ID.
* ``version_number`` (int, required) — Committed version number.
* ``output_directory`` (str or Path, required) — Empty directory, or a path that does not exist.
* ``topics`` (list of str, optional, default None) — Topic filter. Omit for every topic.
* ``include_properties`` (bool, optional, default False) — Add custom properties to the manifest.

Returns a ``Path``. Failed episode transfers are recorded in the manifest; ``DatasetDownloadWarning`` is emitted if some episodes fail.

Raises: ``RuntimeError`` if the version is editable, recordings are
unavailable, ``output_directory`` exists and is not a directory, ``output_directory``
is not empty, or no episodes could be downloaded.

.. code-block:: python

   client.download_dataset(dataset_id="dset_123", version_number=2, output_directory="out")

Episodes
--------

create_episodes
^^^^^^^^^^^^^^^

Get-or-create up to 2,000 episodes. Matching uses time range, recordings, and custom properties of the latest episode version.

Parameters:

* ``project_id`` (str, required) — Project that owns the episodes.
* ``episodes`` (list of dict, required) — 1–2000 items. Exactly one of ``recordings`` or ``event_id``. Recording items may set ``start_time`` and ``end_time`` (timezone-aware ``datetime``). At most 100,000 recording memberships across ``episodes[].recordings``; event inputs resolve at most 10,000. Optional ``properties`` must use episode keys.

Returns a list of ``{"id": str, "created": bool}`` in request order. ``created`` is ``False`` when an existing episode was reused.

.. code-block:: python

   from datetime import datetime, timezone

   episodes = client.create_episodes(
       project_id="prj_123",
       episodes=[
           {
               "recordings": ["rec_1"],
               "start_time": datetime(2026, 11, 2, 18, 0, tzinfo=timezone.utc),
               "end_time": datetime(2026, 11, 2, 18, 5, tzinfo=timezone.utc),
               "properties": {"task": "fold_towel"},
           }
       ],
   )

update_episodes
^^^^^^^^^^^^^^^

Atomic bulk update. Unchanged episodes keep their version.

Parameters:

* ``episodes`` (list of dict, required) — 1–2000 items, each with ``id``. Optional ``applies_to_version``, ``start_time`` and ``end_time`` (timezone-aware ``datetime``), ``recordings`` (``{"add", "remove"}``), ``properties`` (merge; ``None`` unsets a key).

Returns a list of ``{"id": str, "version": int}``.

.. code-block:: python

   client.update_episodes(episodes=[{"id": "ep_123", "properties": {"outcome": "success"}}])

get_episodes
^^^^^^^^^^^^

Page of episodes. Returns each episode's latest version.

Parameters:

* ``project_id`` (str, optional, default None) — Restrict to one Project.
* ``start`` / ``end`` (datetime (timezone-aware), optional, default None) — Overlap filter. Pass both.
* ``has_missing_recordings`` (bool, optional, default None) — Filter by recording availability.
* ``recording_id`` (str, optional, default None) — Episodes containing this recording.
* ``exact_recording_ids`` (list of str, optional, default None) — Exact recording set, 1–50 IDs.
* ``properties_query`` (str, optional, default None) — ``key:value`` terms, ANDed.
* ``sort_by`` (str, optional, default ``created_at``) — ``created_at``, ``start_time``, ``end_time``.
* ``sort_order`` (str, optional, default None) — ``asc`` or ``desc``.
* ``limit`` (int, optional, default 2000) — Page size, 0–2000.
* ``cursor`` (str, optional, default None) — Cursor from a previous page.
* ``include_recordings`` (bool, optional, default False) — Add ``recordings``.
* ``include_properties`` (bool, optional, default False) — Add ``properties``.
* ``include_creation_context`` (bool, optional, default False) — Add ``creation_context``.

Returns a ``Page`` of latest-version episode dicts.

.. code-block:: python

   page = client.get_episodes(project_id="prj_123")

get_episode
^^^^^^^^^^^

Return one episode.

Parameters:

* ``episode_id`` (str, required) — Episode ID.
* ``version`` (int, optional, default None) — Episode version. Latest if omitted.
* ``include_recordings`` (bool, optional, default False) — Add ``recordings``.
* ``include_properties`` (bool, optional, default False) — Add ``properties``.

Returns one episode dict, including ``creation_context``.

.. code-block:: python

   episode = client.get_episode(episode_id="ep_123")

iter_messages
^^^^^^^^^^^^^

Stream decoded messages. Messages are returned in log-time order across
the selected topics.

Parameters:

* ``device_id`` (str, optional, default None) — Device that originated the data. Requires ``start`` and ``end``.
* ``device_name`` (str, optional, default None) — Device name. Multi-project orgs also need ``project_id``. Requires ``start`` and ``end``.
* ``session_id`` (str, optional, default None) — Session to stream. Requires ``start`` and ``end``.
* ``session_key`` (str, optional, default None) — Session key to stream. Requires ``start`` and ``end``.
* ``recording_id`` (str, optional, default None) — Recording to stream.
* ``recording_key`` (str, optional, default None) — Recording key to stream.
* ``episode_id`` (str, optional, default None) — Episode to stream. Uses its time range when ``start`` and ``end`` are omitted.
* ``episode_version`` (int, optional, default None) — Episode version. Requires ``episode_id``. Latest if omitted.
* ``project_id`` (str, optional, default None) — Project for ``device_name`` in multi-project organizations.
* ``start`` (datetime (timezone-aware), optional, default None) — Inclusive start of the stream. Required unless ``episode_id``, ``recording_id``, or ``recording_key`` is given. Pass with ``end``.
* ``end`` (datetime (timezone-aware), optional, default None) — Inclusive end of the stream. Required unless ``episode_id``, ``recording_id``, or ``recording_key`` is given. Pass with ``start``.
* ``topics`` (list of str, optional, default None) — Topic filter. Omit for every topic.
* ``decoder_factories`` (list of DecoderFactory, optional, default None) — Replace the client's default MCAP decoders.
* ``replay_policy`` (str, optional, default None) — ``lastPerChannel`` includes the most recent message on each channel from before ``start``.
* ``replay_lookback_seconds`` (float, optional, default None) — How far before ``start`` to look. Used with ``replay_policy="lastPerChannel"``.

Returns tuples of ``(schema, channel, message, decoded)``. JSON is decoded by default.

Raises: ``RuntimeError`` if ``start`` and ``end`` are omitted unless
``episode_id``, ``recording_id``, or ``recording_key`` is given.

.. code-block:: python

   for schema, channel, message, decoded in client.iter_messages(
       recording_id="rec_123",
       topics=["/camera/front"],
   ):
       print(channel.topic, message.log_time)

update_episode
^^^^^^^^^^^^^^

Update one episode.

Parameters:

* ``episode_id`` (str, required) — Episode ID.
* ``applies_to_version`` (int, optional, default None) — Reject if the latest version differs.
* ``start_time`` (datetime (timezone-aware), optional, default None) — Inclusive start of the episode window.
* ``end_time`` (datetime (timezone-aware), optional, default None) — Inclusive end of the episode window.
* ``recordings`` (dict, optional, default None) — ``{"add": [...], "remove": [...]}`` recording IDs.
* ``properties`` (dict, optional, default None) — Merge keys; ``None`` unsets a key.

Returns the episode dict at the resulting version.

.. code-block:: python

   client.update_episode(episode_id="ep_123", properties={"outcome": "success"})

delete_episode
^^^^^^^^^^^^^^

Delete an episode that no dataset holds.

Parameters:

* ``episode_id`` (str, required) — Episode ID.

Returns ``None``.

Raises: ``HTTPError`` (409) if a dataset holds the episode.

.. code-block:: python

   client.delete_episode(episode_id="ep_123")

get_episode_versions
^^^^^^^^^^^^^^^^^^^^

Page of episode versions, newest first.

Parameters:

* ``episode_id`` (str, required) — Episode ID.
* ``limit`` (int, optional, default 2000) — Page size, 0–2000.
* ``cursor`` (str, optional, default None) — Cursor from a previous page.

Returns a ``Page`` of version dicts (``version``, ``created_at``,
``creator_org_member_id``, ``creator_api_key_id``, ``changed_fields``),
newest first.

.. code-block:: python

   page = client.get_episode_versions(episode_id="ep_123")

Custom properties
-----------------

Key: 1–32 characters ``[A-Za-z0-9_-]``, immutable. Label: 1–32.
``value_type``: ``string``, ``multiline-string``, ``number``,
``enum``, ``boolean``, or ``multi-enum``. ``enum_values`` is
required for ``enum`` and ``multi-enum``. Episode keys use
``resource_type="episode"``.

create_custom_property
^^^^^^^^^^^^^^^^^^^^^^

Create a custom property key.

Parameters:

* ``key`` (str, required) — Immutable identifier for queries.
* ``label`` (str, required) — Display label.
* ``resource_type`` (str, required) — ``device``, ``event``, ``session``, or ``episode``.
* ``value_type`` (str, required) — See above.
* ``enum_values`` (list of str, optional, default None) — Required for ``enum`` and ``multi-enum``.

Returns the custom property dict.

.. code-block:: python

   client.create_custom_property(
       key="outcome",
       label="Outcome",
       resource_type="episode",
       value_type="enum",
       enum_values=["success", "failure"],
   )

get_custom_properties
^^^^^^^^^^^^^^^^^^^^^

List custom properties.

Parameters:

* ``resource_type`` (str, optional, default None) — Filter: ``device``, ``event``, ``session``, or ``episode``.

Returns a list of custom property dicts.

.. code-block:: python

   props = client.get_custom_properties(resource_type="episode")

get_custom_property
^^^^^^^^^^^^^^^^^^^

Return one custom property.

Parameters:

* ``custom_property_id`` (str, required) — Custom property ID.

Returns the custom property dict.

.. code-block:: python

   prop = client.get_custom_property(custom_property_id="cp_123")

update_custom_property
^^^^^^^^^^^^^^^^^^^^^^

Patch ``label`` and/or ``enum_values``.

Parameters:

* ``custom_property_id`` (str, required) — Custom property ID.
* ``label`` (str, optional, default None) — New display label.
* ``enum_values`` (list of str, optional, default None) — Superset of existing values; reorder or add.

Returns the custom property dict.

.. code-block:: python

   client.update_custom_property(custom_property_id="cp_123", label="Outcome")

delete_custom_property
^^^^^^^^^^^^^^^^^^^^^^

Delete a custom property key.

Parameters:

* ``custom_property_id`` (str, required) — Custom property ID.

Returns ``None``.

.. code-block:: python

   client.delete_custom_property(custom_property_id="cp_123")
