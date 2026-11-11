REST reference
==============

Base URL: ``https://api.foxglove.dev/v1``. Authenticate with
``Authorization: Bearer <api-key>``. Official endpoint pages:
https://docs.foxglove.dev/api/datasets and
https://docs.foxglove.dev/api/episodes.

Python names for the same calls: :doc:`python-client`. This page uses API
camelCase.

Pagination uses ``limit``, ``cursor``, and response headers
``fg-pagination-next-cursor`` / ``fg-pagination-previous-cursor``.

Count fields on dataset and version objects are listed with those responses.

Objects
-------

Dataset
^^^^^^^

``id``, ``projectId``, ``name`` (required). ``description``.
``creatorOrgMemberId``, ``creatorApiKeyId``. ``createdAt``, ``updatedAt``.
``episodeCount`` (integer ≥ 0). Create-with-episodes also returns ``added``,
``updated``, ``removed``, ``alreadyPresent``.

DatasetVersion
^^^^^^^^^^^^^^

``versionNumber`` (integer ≥ 1; highest is the editable draft).
``description`` (omitted when empty). ``committedAt`` (null/omitted when
editable). ``committedByOrgMemberId``, ``committedByApiKeyId``.
``createdAt``. ``episodeCount``, ``addedEpisodeCount``,
``removedEpisodeCount``, ``changedEpisodeCount``. ``hasMissingRecordings``
on get/update version.

Episode
^^^^^^^

``id``, ``projectId``. ``startTime``, ``endTime`` (RFC 3339, nanosecond
capable). ``version``, ``latestVersion``. ``properties`` only with
``include=properties``. ``recordings`` only with ``include=recordings``.
``creationContext`` always on GET-by-id; lists need
``include=creationContext``. ``hasMissingRecordings`` when recordings are
included or on dataset membership. ``creatorOrgMemberId``,
``creatorApiKeyId``, ``createdAt``.

Recording members, when included: ``id``, ``path``, ``location`` (optional
``bucket``, ``path``, ``scheme``, ``azureStorageAccountName``), ``start``,
``end``, ``deviceId``, ``available``.

Datasets
--------

``POST /datasets``
^^^^^^^^^^^^^^^^^^

Create a dataset.

Body required: ``projectId``, ``name`` (1–100). Optional: ``description``
(1–500 or null), ``episodeIds`` (≤ 2000). Scope: ``datasets.create``.
Response 200: Dataset object.

.. code-block:: bash

   curl -X POST https://api.foxglove.dev/v1/datasets \
     -H "Authorization: Bearer $FOXGLOVE_API_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"projectId":"prj_123","name":"Training candidates"}'

``GET /datasets``
^^^^^^^^^^^^^^^^^

List datasets. Query: ``projectId``, ``name``, ``sortBy``
(``name``|``createdAt``|``updatedAt``|``episodeCount``, default
``createdAt``), ``sortOrder``, ``limit`` (0–2000, default 2000), ``cursor``.
Scope: ``datasets.list``.

``GET /datasets/{id}``
^^^^^^^^^^^^^^^^^^^^^^

Get a dataset, including ``episodeCount``. Scope: ``datasets.list``.

``PATCH /datasets/{id}``
^^^^^^^^^^^^^^^^^^^^^^^^

Edit ``name`` and/or ``description`` (null clears). Scope:
``datasets.update``.

``DELETE /datasets/{id}``
^^^^^^^^^^^^^^^^^^^^^^^^^

Delete the dataset, not its episodes. Response ``{"success": true}``. Scope:
``datasets.delete``.

``GET /datasets/{id}/episodes``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

List membership on the latest committed version (or the initial draft).
Query: ``sortBy`` (``addedAt``|``createdAt``|``startTime``|``endTime``),
``sortOrder``, ``limit``, ``cursor``, ``start`` and ``end`` together,
``hasMissingRecordings``, ``recordingId``, ``propertiesQuery``, ``include``
(``recordings``|``creationContext``|``properties``). Response
``{"episodes":[...]}``. Each item: ``addedAt``, ``addedInVersion``,
``hasMissingRecordings``, nested ``episode``.

``PATCH /datasets/{id}/episodes``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

Stage draft membership. Body: ``add``, ``update``, ``remove`` (each ≤ 2000).
``add``/``update`` items are IDs or ``{id, version?}``. Does not commit.
Response ``added``, ``updated``, ``removed``, ``alreadyPresent``.

``GET /datasets/{id}/versions``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

List versions. Query: ``sortOrder``, ``limit``, ``cursor``. Response
``{"versions":[...]}``.

``GET /datasets/{id}/versions/{versionNumber}``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

Get one version, including counts and ``hasMissingRecordings``.
``versionNumber`` is ``^[0-9]+$``.

``PATCH /datasets/{id}/versions/{versionNumber}``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

Set ``description`` (string or null, 1–2048 when string). Works on editable
and committed versions.

``GET /datasets/{id}/versions/{versionNumber}/episodes``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

List episodes pinned in that version. Same query as list dataset episodes.

``GET /datasets/{id}/versions/{versionNumber}/compare``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

Compare target ``versionNumber`` to query ``version`` (required, integer ≥ 1,
the base). Optional ``limit``, ``cursor``, ``include``. Response ``changes``,
``addedCount``, ``removedCount``, ``changedCount``, ``nextCursor``. Change
kinds: ``added``, ``removed``, ``changed``.

``POST /datasets/{id}/commit``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

Commit the draft. Optional body ``description``. 409 when no changes.
Response ``committed`` (DatasetVersion) and ``editableVersionNumber``.

``POST /datasets/{id}/discard``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

Discard draft changes. Response ``discardedAdds``, ``discardedRemoves``,
``discardedUpdates``.

``POST /datasets/{id}/versions/{versionNumber}/restore``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

Restore into the draft. Does not commit. Query ``force`` (bool, default
false). Response ``added``, ``updated``, ``removed``, ``discardedAdds``,
``discardedRemoves``, ``discardedUpdates``.

``POST /datasets/{id}/versions/{versionNumber}/clone``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

Clone to a new dataset. Body required ``name``. Optional ``description``,
``includeHistory``. Response: Dataset object.

Episodes
--------

``POST /episodes``
^^^^^^^^^^^^^^^^^^

Get-or-create. Body: ``projectId`` required; ``episodes`` array ≤ 2000. Each
item has exactly one of ``recordings`` or ``eventId``, optional ``startTime``
/ ``endTime``, optional ``properties``. Matching uses time range, recordings,
and custom properties of the latest version. Scope: ``episodes.create``.

``PATCH /episodes``
^^^^^^^^^^^^^^^^^^^

Atomic bulk update, ≤ 2000. Each item needs ``id``. Optional
``appliesToVersion``, times, ``recordings`` ``{add,remove}``, ``properties``
(null unsets). Scope: ``episodes.update``.

``GET /episodes``
^^^^^^^^^^^^^^^^^

List. Query: ``projectId``, ``start``+``end``, ``hasMissingRecordings``,
``recordingId``, ``exactRecordingIds`` (comma-separated), ``propertiesQuery``,
``sortBy`` (``createdAt``|``startTime``|``endTime``), ``sortOrder``,
``limit``, ``cursor``, ``include``. Scope: ``episodes.list``.

``GET /episodes/{id}``
^^^^^^^^^^^^^^^^^^^^^^

Get one. Query: ``version``, ``include`` (``recordings``|``properties``).
Always returns ``creationContext``.

``PATCH /episodes/{id}``
^^^^^^^^^^^^^^^^^^^^^^^^

Update one. Body: ``appliesToVersion``, ``startTime``, ``endTime``,
``recordings``, ``properties``.

``DELETE /episodes/{id}``
^^^^^^^^^^^^^^^^^^^^^^^^^

409 if the episode belongs to a dataset. Response ``{"success": true}``.
Scope: ``episodes.delete``.

``GET /episodes/{id}/versions``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

List episode versions, newest first. Fields include ``version``,
``createdAt``, creators, ``changedFields``.

Custom properties
-----------------

``POST /custom-properties``
    Body: ``key``, ``label``, ``resourceType`` (includes ``episode``),
    ``valueType`` (``string``|``multiline-string``|``number``|``enum``|
    ``boolean``|``multi-enum``), ``enumValues`` when needed.

``GET /custom-properties``, ``GET/PATCH/DELETE /custom-properties/{id}``
    https://docs.foxglove.dev/api/custom-properties
