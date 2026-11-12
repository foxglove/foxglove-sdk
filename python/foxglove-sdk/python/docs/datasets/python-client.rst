Python client reference
=======================

Package ``foxglove-client`` (0.23.0 or later).
``from foxglove.client import Client``.

Each method calls one endpoint in :doc:`rest`, except ``download_dataset``
and ``iter_messages``.
Returned dict keys are snake_case, except ``creation_context`` and recording
``location``, which keep API names. Times are ``datetime`` objects. Errors
raise ``requests.exceptions.HTTPError``.

``Client(token: str, host: str = "api.foxglove.dev")``.

Datasets
--------

create_dataset
^^^^^^^^^^^^^^

Create a dataset, optionally seeding the first version with episodes.

Parameters

.. list-table::
   :header-rows: 1
   :widths: 18 14 10 12 46

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``project_id``
     - str
     - yes
     -
     - Project that owns the dataset.
   * - ``name``
     - str
     - yes
     -
     - 1–100 characters. Unique in the Project, case-insensitive.
   * - ``description``
     - str
     - no
     -
     - 1–500 characters.
   * - ``episode_ids``
     - list of str
     - no
     -
     - At most 2,000 episode IDs.

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

List datasets. ``name`` is a case-insensitive substring match.

Parameters

.. list-table::
   :header-rows: 1
   :widths: 18 14 10 12 46

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``project_id``
     - str
     - no
     -
     - Restrict to one Project.
   * - ``name``
     - str
     - no
     -
     - Case-insensitive substring.
   * - ``sort_by``
     - str
     - no
     - created_at
     - ``name``, ``created_at``, ``updated_at``, ``episode_count``.
   * - ``sort_order``
     - str
     - no
     -
     - ``asc`` or ``desc``.
   * - ``limit``
     - int
     - no
     - 2000
     - Page size, 0–2000.
   * - ``cursor``
     - str
     - no
     -
     - Cursor from a previous page.

Returns a ``Page``. Iterate with ``page.auto_paging_iter()``.

.. code-block:: python

   for d in client.get_datasets(project_id="prj_123").auto_paging_iter():
       print(d["id"], d["name"], d["episode_count"])

get_dataset
^^^^^^^^^^^

Return one dataset, including ``episode_count``.

Parameters

.. list-table::
   :header-rows: 1
   :widths: 18 14 10 12 46

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``dataset_id``
     - str
     - yes
     -
     - Dataset ID.

.. code-block:: python

   dataset = client.get_dataset(dataset_id="dset_123")

update_dataset
^^^^^^^^^^^^^^

Patch ``name`` and/or ``description``. ``description=None`` clears it.

Parameters

.. list-table::
   :header-rows: 1
   :widths: 18 14 10 12 46

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``dataset_id``
     - str
     - yes
     -
     - Dataset ID.
   * - ``name``
     - str
     - no
     -
     - 1–100 characters.
   * - ``description``
     - str or None
     - no
     -
     - 1–500, or ``None`` to clear.

.. code-block:: python

   client.update_dataset(dataset_id="dset_123", description=None)

delete_dataset
^^^^^^^^^^^^^^

Delete the dataset. Does not delete episodes. Returns ``None``.

Parameters

.. list-table::
   :header-rows: 1
   :widths: 18 14 10 12 46

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``dataset_id``
     - str
     - yes
     -
     - Dataset ID.

get_dataset_episodes
^^^^^^^^^^^^^^^^^^^^

Page of membership on the latest committed version, or the initial draft if
none is committed. ``sort_by`` default ``added_at``. ``start`` and ``end``
(datetime) must be passed together.

Parameters: same as :ref:`get_dataset_version_episodes` except there is no
``version_number``.

update_dataset_episodes
^^^^^^^^^^^^^^^^^^^^^^^

Stage draft membership. Does not commit.

An ID in ``update`` can't also be in ``add`` or ``remove``. An object in
``add`` must not already be in the draft. At least one list must be
non-empty.

Parameters

.. list-table::
   :header-rows: 1
   :widths: 18 14 10 12 46

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``dataset_id``
     - str
     - yes
     -
     - Dataset ID.
   * - ``add``
     - list
     - no
     -
     - IDs or ``{"id", "version?"}``. Latest version if omitted. ≤ 2000.
   * - ``update``
     - list
     - no
     -
     - Same forms as ``add``. ≤ 2000.
   * - ``remove``
     - list of str
     - no
     -
     - Episode IDs to drop. ≤ 2000.

Returns ``added``, ``updated``, ``removed``, ``already_present`` (ints).

.. code-block:: python

   client.update_dataset_episodes(
       dataset_id="dset_123", add=["ep_new"], remove=["ep_old"]
   )

get_dataset_versions
^^^^^^^^^^^^^^^^^^^^

Page of versions (committed plus the current draft). The list includes the
draft; its ``committed_at`` is ``None``.

Parameters

.. list-table::
   :header-rows: 1
   :widths: 18 14 10 12 46

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``dataset_id``
     - str
     - yes
     -
     - Dataset ID.
   * - ``sort_order``
     - str
     - no
     -
     - ``asc`` or ``desc``.
   * - ``limit``
     - int
     - no
     - 2000
     - Page size, 0–2000.
   * - ``cursor``
     - str
     - no
     -
     - Cursor from a previous page.

get_dataset_version
^^^^^^^^^^^^^^^^^^^

One version, including ``has_missing_recordings`` and the count fields
``episode_count``, ``added_episode_count``, ``removed_episode_count``,
``changed_episode_count``. ``committed_at`` is ``None`` on the draft.

Parameters

.. list-table::
   :header-rows: 1
   :widths: 18 14 10 12 46

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``dataset_id``
     - str
     - yes
     -
     - Dataset ID.
   * - ``version_number``
     - int
     - yes
     -
     - Dataset version number.

update_dataset_version
^^^^^^^^^^^^^^^^^^^^^^

Set ``description`` (str or ``None``) on a committed or editable version.
Does not change episodes or commit attribution. String length 1–2048 with a
non-whitespace character.

Parameters

.. list-table::
   :header-rows: 1
   :widths: 18 14 10 12 46

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``dataset_id``
     - str
     - yes
     -
     - Dataset ID.
   * - ``version_number``
     - int
     - yes
     -
     - Dataset version number.
   * - ``description``
     - str or None
     - yes
     -
     - New description, or ``None`` to clear.

.. _get_dataset_version_episodes:

get_dataset_version_episodes
^^^^^^^^^^^^^^^^^^^^^^^^^^^^

List the episodes in one dataset version, at the episode versions it pins.

Parameters

.. list-table::
   :header-rows: 1
   :widths: 22 14 10 12 42

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``dataset_id``
     - str
     - yes
     -
     - Dataset ID.
   * - ``version_number``
     - int
     - yes
     -
     - Dataset version number.
   * - ``sort_by``
     - str
     - no
     - added_at
     - ``added_at``, ``created_at``, ``start_time``, ``end_time``.
   * - ``sort_order``
     - str
     - no
     -
     - ``asc`` or ``desc``.
   * - ``limit``
     - int
     - no
     - 2000
     - Page size, 0–2000.
   * - ``cursor``
     - str
     - no
     -
     - Cursor from a previous page.
   * - ``start`` / ``end``
     - datetime
     - no
     -
     - Overlap filter. Pass both.
   * - ``has_missing_recordings``
     - bool
     - no
     -
     - Filter by recording availability.
   * - ``recording_id``
     - str
     - no
     -
     - Episodes containing this recording.
   * - ``properties_query``
     - str
     - no
     -
     - ``key:value`` terms, ANDed.
   * - ``include_recordings``
     - bool
     - no
     - False
     - Add ``recordings``.
   * - ``include_properties``
     - bool
     - no
     - False
     - Add ``properties``.
   * - ``include_creation_context``
     - bool
     - no
     - False
     - Add ``creation_context``.

Returns ``Page`` of dicts: ``added_at`` (datetime), ``added_in_version`` (int),
``has_missing_recordings`` (bool), ``episode`` (dict). Iterate with
``page.auto_paging_iter()``.

.. code-block:: python

   for m in client.get_dataset_version_episodes(
       dataset_id="dset_123", version_number=2
   ).auto_paging_iter():
       print(m["episode"]["id"], m["episode"]["version"])

compare_dataset_versions
^^^^^^^^^^^^^^^^^^^^^^^^

Membership changes from ``base_version`` to ``version_number``. Optional
``include_*`` flags as on list calls. ``limit`` default 2000. Counts cover
the whole changeset. ``next_cursor`` is ``None`` on the last page. Changed
rows include ``previous_version`` and ``previous_episode``.

Parameters

.. list-table::
   :header-rows: 1
   :widths: 22 14 10 12 42

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``dataset_id``
     - str
     - yes
     -
     - Dataset ID.
   * - ``version_number``
     - int
     - yes
     -
     - Target version.
   * - ``base_version``
     - int
     - yes
     -
     - Base version.
   * - ``limit``
     - int
     - no
     - 2000
     - Page size, 0–2000.
   * - ``cursor``
     - str
     - no
     -
     - Cursor from a previous page.
   * - ``include_recordings``
     - bool
     - no
     - False
     - Add ``recordings``.
   * - ``include_properties``
     - bool
     - no
     - False
     - Add ``properties``.
   * - ``include_creation_context``
     - bool
     - no
     - False
     - Add ``creation_context``.

Returns ``changes``, ``added_count``, ``removed_count``, ``changed_count``,
``next_cursor``.

.. code-block:: python

   diff = client.compare_dataset_versions(
       dataset_id="dset_123", version_number=2, base_version=1
   )

commit_dataset
^^^^^^^^^^^^^^

Commit the draft. Raises ``HTTPError`` (409) when the draft has no changes.
Omitting ``description`` keeps the draft's description; ``None`` clears it.

Parameters

.. list-table::
   :header-rows: 1
   :widths: 18 14 10 12 46

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``dataset_id``
     - str
     - yes
     -
     - Dataset ID.
   * - ``description``
     - str or None
     - no
     -
     - 1–2048, or ``None`` to clear.

Returns ``committed`` (version dict) and ``editable_version_number``.

discard_dataset
^^^^^^^^^^^^^^^

Drop draft adds, removes, and updates.

Parameters

.. list-table::
   :header-rows: 1
   :widths: 18 14 10 12 46

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``dataset_id``
     - str
     - yes
     -
     - Dataset ID.

Returns ``discarded_adds``, ``discarded_removes``, ``discarded_updates``.

restore_dataset_version
^^^^^^^^^^^^^^^^^^^^^^^

Stage membership so the draft matches ``version_number``. Does not commit.
``force`` (bool, default ``False``): replace pending draft changes. Without
it, a draft with pending changes raises ``HTTPError`` (409).

Parameters

.. list-table::
   :header-rows: 1
   :widths: 18 14 10 12 46

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``dataset_id``
     - str
     - yes
     -
     - Dataset ID.
   * - ``version_number``
     - int
     - yes
     -
     - Committed version to restore.
   * - ``force``
     - bool
     - no
     - False
     - Discard pending draft changes and restore.

Returns ``added``, ``updated``, ``removed``, ``discarded_adds``,
``discarded_removes``, ``discarded_updates``.

clone_dataset_version
^^^^^^^^^^^^^^^^^^^^^

Create a new dataset from a committed version. Without ``include_history``,
its only committed version is version 1. With ``include_history=True``,
versions 1 through the cloned version keep their numbers, descriptions, and
commit details. Either way an empty draft opens on top. Raises ``HTTPError``
(409) if the name is taken in the project, and 400 ``DatasetSpansPrimarySites``
if recordings span Primary Sites.

Parameters

.. list-table::
   :header-rows: 1
   :widths: 18 14 10 12 46

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``dataset_id``
     - str
     - yes
     -
     - Source dataset ID.
   * - ``version_number``
     - int
     - yes
     -
     - Committed version to clone.
   * - ``name``
     - str
     - yes
     -
     - 1–100, unique in the Project.
   * - ``description``
     - str
     - no
     -
     - 1–500.
   * - ``include_history``
     - bool
     - no
     - False
     - Copy versions 1 through the cloned version, keeping their numbers.

Returns the new dataset dict.

download_dataset
^^^^^^^^^^^^^^^^

Export a committed version to one MCAP per episode plus ``manifest.json``.
``output_directory`` must be empty or not exist. Raises ``RuntimeError`` for
an editable version or one with unavailable recordings.

Parameters

.. list-table::
   :header-rows: 1
   :widths: 22 16 10 12 40

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``dataset_id``
     - str
     - yes
     -
     - Dataset ID.
   * - ``version_number``
     - int
     - yes
     -
     - Committed version number.
   * - ``output_directory``
     - str or Path
     - yes
     -
     - Empty directory, or a path that does not exist.
   * - ``topics``
     - list of str
     - no
     -
     - Topic filter. Omit for every topic.
   * - ``include_properties``
     - bool
     - no
     - False
     - Add custom properties to the manifest.

Returns a ``Path``. Failed episode transfers are recorded in the manifest;
``DatasetDownloadWarning`` is emitted if some episodes fail.

Episodes
--------

create_episodes
^^^^^^^^^^^^^^^

Get-or-create up to 2,000 episodes. Matching uses time range, recordings, and
custom properties of the latest episode version.

Each item must have exactly one of ``recordings`` or ``event_id``. Recording
items may set ``start_time`` and ``end_time`` (timezone-aware ``datetime``).
``event_id`` items can't set times. Across all ``episodes[].recordings``
lists, a request may contain at most 100,000 recording memberships.
Duplicate recording IDs within an episode count once; a recording listed
in multiple episodes counts once per episode. Event inputs in one request
can resolve at most 10,000 recording memberships. ``event_id`` snapshots
the event's current range and overlapping completed recordings; later
event edits do not change the episode. Optional ``properties`` must use
keys defined for ``resourceType=episode``.

Returns a list of ``{"id": str, "created": bool}`` in request order.
``created`` is ``False`` when an existing episode was reused. Only new
episodes (``created: True``) count toward episode usage
(https://docs.foxglove.dev/docs/pricing).

Parameters

.. list-table::
   :header-rows: 1
   :widths: 18 14 10 12 46

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``project_id``
     - str
     - yes
     -
     - Project that owns the episodes.
   * - ``episodes``
     - list of dict
     - yes
     -
     - 1–2000 items. See above.

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

Atomic bulk update. Each item requires ``id``. Optional
``applies_to_version``, ``start_time``, ``end_time`` (timezone-aware
``datetime``), ``recordings`` (``{"add": [...], "remove": [...]}``),
``properties`` (merge; ``None`` unsets a key). Unchanged episodes keep their
version. Up to 2,000 items; duplicate IDs are rejected.

Returns a list of ``{"id": str, "version": int}``.

Parameters

.. list-table::
   :header-rows: 1
   :widths: 18 14 10 12 46

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``episodes``
     - list of dict
     - yes
     -
     - 1–2000 updates.

get_episodes
^^^^^^^^^^^^

Page of episodes. Returns each episode's latest version.
``exact_recording_ids`` (list of str, 1–50). ``sort_by`` default
``created_at``. ``start`` and ``end`` (datetime) must be passed together.

Parameters

.. list-table::
   :header-rows: 1
   :widths: 22 14 10 12 42

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``project_id``
     - str
     - no
     -
     - Restrict to one Project.
   * - ``start`` / ``end``
     - datetime
     - no
     -
     - Overlap filter. Pass both.
   * - ``has_missing_recordings``
     - bool
     - no
     -
     - Filter by recording availability.
   * - ``recording_id``
     - str
     - no
     -
     - Episodes containing this recording.
   * - ``exact_recording_ids``
     - list of str
     - no
     -
     - Exact recording set, 1–50 IDs.
   * - ``properties_query``
     - str
     - no
     -
     - ``key:value`` terms, ANDed.
   * - ``sort_by``
     - str
     - no
     - created_at
     - ``created_at``, ``start_time``, ``end_time``.
   * - ``sort_order``
     - str
     - no
     -
     - ``asc`` or ``desc``.
   * - ``limit``
     - int
     - no
     - 2000
     - Page size, 0–2000.
   * - ``cursor``
     - str
     - no
     -
     - Cursor from a previous page.
   * - ``include_recordings``
     - bool
     - no
     - False
     - Add ``recordings``.
   * - ``include_properties``
     - bool
     - no
     - False
     - Add ``properties``.
   * - ``include_creation_context``
     - bool
     - no
     - False
     - Add ``creation_context``.

get_episode
^^^^^^^^^^^

One episode. Always includes ``creation_context``. Optional ``version``
integer ≥ 1, default latest.

Parameters

.. list-table::
   :header-rows: 1
   :widths: 22 14 10 12 42

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``episode_id``
     - str
     - yes
     -
     - Episode ID.
   * - ``version``
     - int
     - no
     -
     - Episode version. Latest if omitted.
   * - ``include_recordings``
     - bool
     - no
     - False
     - Add ``recordings``.
   * - ``include_properties``
     - bool
     - no
     - False
     - Add ``properties``.

iter_messages
^^^^^^^^^^^^^

Stream decoded messages. Yields ``(schema, channel, message, decoded)``.
JSON is decoded by default. Protobuf and ROS decode when their packages
are installed. Pass ``decoder_factories`` to replace the client's default
decoders. For a pinned episode, pass ``episode_id`` and optional
``episode_version`` (latest if omitted). Its time range is used when
``start`` and ``end`` are omitted.

Parameters

.. list-table::
   :header-rows: 1
   :widths: 22 14 10 12 42

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``device_id``
     - str
     - no
     -
     - Device that originated the data.
   * - ``device_name``
     - str
     - no
     -
     - Device name. Multi-project orgs also need ``project_id``.
   * - ``session_id``
     - str
     - no
     -
     - Session to stream.
   * - ``session_key``
     - str
     - no
     -
     - Session key to stream.
   * - ``recording_id``
     - str
     - no
     -
     - Recording to stream.
   * - ``recording_key``
     - str
     - no
     -
     - Recording key to stream.
   * - ``episode_id``
     - str
     - no
     -
     - Episode to stream. Uses its time range when ``start`` and ``end``
       are omitted.
   * - ``episode_version``
     - int
     - no
     -
     - Episode version. Requires ``episode_id``. Latest if omitted.
   * - ``project_id``
     - str
     - no
     -
     - Project for ``device_name`` in multi-project organizations.
   * - ``start``
     - datetime
     - no
     -
     - Inclusive start of the stream. Pass with ``end``.
   * - ``end``
     - datetime
     - no
     -
     - Inclusive end of the stream. Pass with ``start``.
   * - ``topics``
     - list of str
     - no
     -
     - Topic filter. Omit for every topic.
   * - ``decoder_factories``
     - list of DecoderFactory
     - no
     -
     - Replace the client's default MCAP decoders.
   * - ``replay_policy``
     - str
     - no
     -
     - ``lastPerChannel`` includes the most recent message on each
       channel from before ``start``.
   * - ``replay_lookback_seconds``
     - float
     - no
     -
     - How far before ``start`` to look. Used with
       ``replay_policy="lastPerChannel"``.

.. code-block:: python

   episode = client.get_episode(episode_id="ep_123", version=3)
   for schema, channel, message, decoded in client.iter_messages(
       episode_id=episode["id"],
       episode_version=episode["version"],
       topics=["/camera/front", "/joint_states"],
   ):
       print(channel.topic, message.log_time, type(decoded))

update_episode
^^^^^^^^^^^^^^

Single-episode form of ``update_episodes``. Returns the episode at the
resulting version (unchanged if nothing changed). ``start_time`` /
``end_time`` are timezone-aware ``datetime``.

delete_episode
^^^^^^^^^^^^^^

Returns ``None``. Raises ``HTTPError`` (409) if a dataset holds the episode.

Parameters

.. list-table::
   :header-rows: 1
   :widths: 18 14 10 12 46

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``episode_id``
     - str
     - yes
     -
     - Episode ID.

get_episode_versions
^^^^^^^^^^^^^^^^^^^^

Page of episode versions, newest first. Each item: ``version`` (int),
``created_at`` (datetime), ``creator_org_member_id``, ``creator_api_key_id``,
``changed_fields`` (list of ``start_time``, ``end_time``, ``recordings``,
``properties``).

Parameters

.. list-table::
   :header-rows: 1
   :widths: 18 14 10 12 46

   * - Name
     - Type
     - Required
     - Default
     - Description
   * - ``episode_id``
     - str
     - yes
     -
     - Episode ID.
   * - ``limit``
     - int
     - no
     - 2000
     - Page size, 0–2000.
   * - ``cursor``
     - str
     - no
     -
     - Cursor from a previous page.

Custom properties
-----------------

``create_custom_property(key, label, resource_type, value_type, enum_values=None)``;
``get_custom_properties(resource_type=None)``;
``get_custom_property(custom_property_id)``;
``update_custom_property(custom_property_id, label=None, enum_values=None)``;
``delete_custom_property(custom_property_id)``.

Key: 1–32 chars ``[A-Za-z0-9_-]``, immutable. Label: 1–32. ``enum_values`` is
required for ``enum``. ``update_custom_property`` can only add or reorder
``enum_values``. ``value_type`` is ``string``, ``multiline-string``,
``number``, ``enum``, ``boolean``, or ``multi-enum``. Episode keys use
``resource_type="episode"``.

.. code-block:: python

   client.create_custom_property(
       key="outcome",
       label="Outcome",
       resource_type="episode",
       value_type="enum",
       enum_values=["success", "failure"],
   )
