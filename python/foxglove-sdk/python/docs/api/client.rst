foxglove.client (foxglove-client package)
=========================================

``foxglove.client`` is the Python client for the Foxglove API. It is published separately from the
SDK, as the `foxglove-client <https://pypi.org/project/foxglove-client/>`__ package, and its source
is in the `foxglove-python <https://github.com/foxglove/foxglove-python>`__ repository. The
``torch`` and ``ray`` extras of ``foxglove-sdk`` install it.

This page describes ``foxglove-client`` 0.23. It covers the methods of
:py:class:`~foxglove.client.Client` that work with datasets, episodes, data coverage, and custom
properties.

Pass timezone-aware ``datetime`` values for time arguments. The client converts them with
``astimezone()``, so a naive ``datetime`` is read as local time. Times in results, such as
``created_at`` and ``start_time``, are ``datetime`` objects. Result dictionaries use snake_case
keys. Values that the client passes through unchanged, such as ``creation_context`` and a
recording's ``location``, keep the API's field names.

The REST API that these methods call is documented at https://docs.foxglove.dev/api.

The examples on this page use the ``client`` that the first example creates.

.. The signatures and parameter lists below are compared with the installed foxglove-client by
   python/foxglove/tests/test_client_reference_docs.py. The calls in the examples are checked by
   python/foxglove/tests/test_reference_examples.py.

.. currentmodule:: foxglove.client

.. py:class:: Client

   Client for the Foxglove API.

   .. py:method:: __init__(token, host="api.foxglove.dev")

      Create a client.

      :param str token: Required. API key sent with every request.
      :param str host: Optional, default ``"api.foxglove.dev"``. Host name of the API.

      .. code-block:: python

         import os

         from foxglove.client import Client

         client = Client(token=os.environ["FOXGLOVE_API_TOKEN"])

Datasets
--------

.. py:method:: Client.create_dataset(*, project_id, name, description=None, episode_ids=None)

   Create a dataset, optionally seeding the first version with episodes.

   :param str project_id: Required. Project that owns the dataset.
   :param str name: Required. 1–100 characters. Unique in the Project, case-insensitive.
   :param str description: Optional, default ``None``. 1–500 characters.
   :param list[str] episode_ids: Optional, default ``None``. At most 2,000 episode IDs.
   :returns: The dataset, with ``id``, ``project_id``, ``name``, ``description``,
      ``creator_org_member_id``, ``creator_api_key_id``, ``created_at``, ``updated_at``,
      ``episode_count``, and the counts ``added``, ``updated``, ``removed``, and
      ``already_present`` for ``episode_ids``.
   :rtype: dict
   :raises requests.exceptions.HTTPError: If the API returns an error, for example 409 when the
      name is taken.

   .. code-block:: python

      dataset = client.create_dataset(
          project_id="prj_123",
          name="Training candidates",
          episode_ids=["ep_1"],
      )

.. py:method:: Client.get_datasets(*, project_id=None, name=None, sort_by=None, sort_order=None, limit=None, cursor=None)

   List datasets.

   :param str project_id: Optional, default ``None``. Restrict to one Project.
   :param str name: Optional, default ``None``. Case-insensitive substring of the dataset name.
   :param str sort_by: Optional, default ``None``, which sorts by ``created_at``. One of ``name``,
      ``created_at``, ``updated_at``, or ``episode_count``.
   :param str sort_order: Optional, default ``None``. ``asc`` or ``desc``.
   :param int limit: Optional, default ``None``, which returns up to 2,000 datasets. Page size,
      0–2000.
   :param str cursor: Optional, default ``None``. Cursor from a previous page.
   :returns: A page of datasets, each as :py:meth:`~Client.get_dataset` returns it.
   :rtype: Page
   :raises requests.exceptions.HTTPError: If the API returns an error.

   .. code-block:: python

      page = client.get_datasets(project_id="prj_123")
      for d in page.auto_paging_iter():
          print(d["id"], d["name"], d["episode_count"])

.. py:method:: Client.get_dataset(*, dataset_id)

   Return one dataset.

   :param str dataset_id: Required. Dataset ID.
   :returns: The dataset, with ``id``, ``project_id``, ``name``, ``description``,
      ``creator_org_member_id``, ``creator_api_key_id``, ``created_at``, ``updated_at``, and
      ``episode_count``.
   :rtype: dict
   :raises requests.exceptions.HTTPError: If the API returns an error, for example 404 when the
      dataset does not exist.

   .. code-block:: python

      dataset = client.get_dataset(dataset_id="dset_123")
      print(dataset["name"], dataset["episode_count"])
      print(dataset["updated_at"].isoformat())

.. py:method:: Client.update_dataset(*, dataset_id, name=None, description=...)

   Update a dataset's name, description, or both.

   :param str dataset_id: Required. Dataset ID.
   :param str name: Optional, default ``None``, which leaves the name unchanged. 1–100 characters.
   :param description: Optional. 1–500 characters. Omit it to leave the description unchanged, or
      pass ``None`` to clear it.
   :type description: str or None
   :returns: The updated dataset, as :py:meth:`~Client.get_dataset` returns it.
   :rtype: dict
   :raises requests.exceptions.HTTPError: If the API returns an error, for example 409 when the
      name is taken.

   .. code-block:: python

      dataset = client.update_dataset(
          dataset_id="dset_123", description=None
      )

.. py:method:: Client.delete_dataset(*, dataset_id)

   Delete a dataset. Its episodes remain.

   :param str dataset_id: Required. Dataset ID.
   :returns: ``None``.
   :raises requests.exceptions.HTTPError: If the API returns an error, for example 404 when the
      dataset does not exist.

   .. code-block:: python

      scratch = client.create_dataset(
          project_id="prj_123", name="Scratch"
      )
      client.delete_dataset(dataset_id=scratch["id"])

.. py:method:: Client.get_dataset_episodes(*, dataset_id, sort_by=None, sort_order=None, limit=None, cursor=None, start=None, end=None, has_missing_recordings=None, recording_id=None, properties_query=None, include_recordings=False, include_properties=False, include_creation_context=False)

   List the episodes in a dataset's latest committed version, or in its draft if no version is
   committed.

   :param str dataset_id: Required. Dataset ID.
   :param str sort_by: Optional, default ``None``, which sorts by ``added_at``. One of
      ``added_at``, ``created_at``, ``start_time``, or ``end_time``.
   :param str sort_order: Optional, default ``None``. ``asc`` or ``desc``.
   :param int limit: Optional, default ``None``, which returns up to 2,000 episodes. Page size,
      0–2000.
   :param str cursor: Optional, default ``None``. Cursor from a previous page.
   :param datetime.datetime start: Optional, default ``None``. Start of a time range. Episodes
      whose time window overlaps the range are returned. Pass it together with ``end``.
   :param datetime.datetime end: Optional, default ``None``. End of the time range.
   :param bool has_missing_recordings: Optional, default ``None``. ``True`` returns episodes with
      a missing recording. ``False`` returns episodes whose recordings are all available.
   :param str recording_id: Optional, default ``None``. Return episodes that contain this
      recording.
   :param str properties_query: Optional, default ``None``. Space-separated ``key:value`` terms.
      Episodes that match every term are returned.
   :param bool include_recordings: Optional, default ``False``. Add ``recordings`` to each
      episode.
   :param bool include_properties: Optional, default ``False``. Add ``properties`` to each
      episode.
   :param bool include_creation_context: Optional, default ``False``. Add ``creation_context`` to
      each episode.
   :returns: A page of dicts with ``added_at``, ``added_in_version``, ``has_missing_recordings``,
      and ``episode``, which has the keys that :py:meth:`~Client.get_episodes` lists.
   :rtype: Page
   :raises ValueError: If only one of ``start`` and ``end`` is given.
   :raises requests.exceptions.HTTPError: If the API returns an error, for example 404 when the
      dataset does not exist.

   .. code-block:: python

      page = client.get_dataset_episodes(dataset_id="dset_123")
      for m in page.auto_paging_iter():
          print(m["episode"]["id"], m["added_in_version"])

.. py:method:: Client.update_dataset_episodes(*, dataset_id, add=None, remove=None, update=None)

   Stage episode additions, removals, and episode version changes in a dataset's draft.

   :param str dataset_id: Required. Dataset ID.
   :param list add: Optional, default ``None``. At most 2,000 episodes to add. Each item is an
      episode ID, which selects the latest episode version, or a dict
      ``{"id": ..., "version": ...}`` in which ``version`` is optional.
   :param list[str] remove: Optional, default ``None``. At most 2,000 episode IDs to remove.
   :param list update: Optional, default ``None``. At most 2,000 episodes in the draft whose
      episode version changes, in the same forms as ``add``.
   :returns: The counts ``added``, ``updated``, ``removed``, and ``already_present``.
   :rtype: dict
   :raises requests.exceptions.HTTPError: If the API returns an error, for example when an
      episode is in ``update`` and also in ``add`` or ``remove``.

   .. code-block:: python

      client.update_dataset_episodes(
          dataset_id="dset_123", add=["ep_new"], remove=["ep_old"]
      )

.. py:method:: Client.get_dataset_versions(*, dataset_id, sort_order=None, limit=None, cursor=None)

   List a dataset's versions: its committed versions and its draft.

   :param str dataset_id: Required. Dataset ID.
   :param str sort_order: Optional, default ``None``. ``asc`` or ``desc``.
   :param int limit: Optional, default ``None``, which returns up to 2,000 versions. Page size,
      0–2000.
   :param str cursor: Optional, default ``None``. Cursor from a previous page.
   :returns: A page of versions, each with ``version_number``, ``description``, ``created_at``,
      ``committed_at``, ``committed_by_org_member_id``, ``committed_by_api_key_id``,
      ``episode_count``, ``added_episode_count``, ``removed_episode_count``, and
      ``changed_episode_count``. The draft's ``committed_at`` is ``None``.
   :rtype: Page
   :raises requests.exceptions.HTTPError: If the API returns an error, for example 404 when the
      dataset does not exist.

   .. code-block:: python

      page = client.get_dataset_versions(dataset_id="dset_123")
      for v in page.auto_paging_iter():
          print(v["version_number"], v["committed_at"])

.. py:method:: Client.get_dataset_version(*, dataset_id, version_number)

   Return one dataset version.

   :param str dataset_id: Required. Dataset ID.
   :param int version_number: Required. Dataset version number.
   :returns: The version, with ``version_number``, ``description``, ``created_at``,
      ``committed_at``, ``committed_by_org_member_id``, ``committed_by_api_key_id``,
      ``episode_count``, ``added_episode_count``, ``removed_episode_count``,
      ``changed_episode_count``, and ``has_missing_recordings``.
   :rtype: dict
   :raises requests.exceptions.HTTPError: If the API returns an error, for example 404 when the
      version does not exist.

   .. code-block:: python

      v = client.get_dataset_version(
          dataset_id="dset_123", version_number=2
      )

.. py:method:: Client.update_dataset_version(*, dataset_id, version_number, description)

   Set the description of a committed version or of the draft.

   :param str dataset_id: Required. Dataset ID.
   :param int version_number: Required. Dataset version number.
   :param description: Required. New description, 1–2048 characters, or ``None`` to clear it.
   :type description: str or None
   :returns: The version, as :py:meth:`~Client.get_dataset_version` returns it.
   :rtype: dict
   :raises requests.exceptions.HTTPError: If the API returns an error, for example 404 when the
      version does not exist.

   .. code-block:: python

      client.update_dataset_version(
          dataset_id="dset_123",
          version_number=2,
          description=None,
      )

.. py:method:: Client.get_dataset_version_episodes(*, dataset_id, version_number, sort_by=None, sort_order=None, limit=None, cursor=None, start=None, end=None, has_missing_recordings=None, recording_id=None, properties_query=None, include_recordings=False, include_properties=False, include_creation_context=False)

   List the episodes in one dataset version, at the episode versions it pins.

   :param str dataset_id: Required. Dataset ID.
   :param int version_number: Required. Dataset version number.
   :param str sort_by: Optional, default ``None``, which sorts by ``added_at``. One of
      ``added_at``, ``created_at``, ``start_time``, or ``end_time``.
   :param str sort_order: Optional, default ``None``. ``asc`` or ``desc``.
   :param int limit: Optional, default ``None``, which returns up to 2,000 episodes. Page size,
      0–2000.
   :param str cursor: Optional, default ``None``. Cursor from a previous page.
   :param datetime.datetime start: Optional, default ``None``. Start of a time range. Episodes
      whose time window overlaps the range are returned. Pass it together with ``end``.
   :param datetime.datetime end: Optional, default ``None``. End of the time range.
   :param bool has_missing_recordings: Optional, default ``None``. ``True`` returns episodes with
      a missing recording. ``False`` returns episodes whose recordings are all available.
   :param str recording_id: Optional, default ``None``. Return episodes that contain this
      recording.
   :param str properties_query: Optional, default ``None``. Space-separated ``key:value`` terms.
      Episodes that match every term are returned.
   :param bool include_recordings: Optional, default ``False``. Add ``recordings`` to each
      episode.
   :param bool include_properties: Optional, default ``False``. Add ``properties`` to each
      episode.
   :param bool include_creation_context: Optional, default ``False``. Add ``creation_context`` to
      each episode.
   :returns: A page of dicts with ``added_at``, ``added_in_version``, ``has_missing_recordings``,
      and ``episode``, which has the keys that :py:meth:`~Client.get_episodes` lists.
   :rtype: Page
   :raises ValueError: If only one of ``start`` and ``end`` is given.
   :raises requests.exceptions.HTTPError: If the API returns an error, for example 404 when the
      version does not exist.

   .. code-block:: python

      page = client.get_dataset_version_episodes(
          dataset_id="dset_123", version_number=2
      )
      for m in page.auto_paging_iter():
          print(m["episode"]["id"], m["episode"]["version"])

.. py:method:: Client.compare_dataset_versions(*, dataset_id, version_number, base_version, limit=None, cursor=None, include_recordings=False, include_properties=False, include_creation_context=False)

   Return the episodes added, removed, or changed from ``base_version`` to ``version_number``.

   :param str dataset_id: Required. Dataset ID.
   :param int version_number: Required. Target version.
   :param int base_version: Required. Base version.
   :param int limit: Optional, default ``None``, which returns up to 2,000 changes. Page size,
      0–2000.
   :param str cursor: Optional, default ``None``. ``next_cursor`` from a previous call.
   :param bool include_recordings: Optional, default ``False``. Add ``recordings`` to each
      episode.
   :param bool include_properties: Optional, default ``False``. Add ``properties`` to each
      episode.
   :param bool include_creation_context: Optional, default ``False``. Add ``creation_context`` to
      each episode.
   :returns: A dict with ``changes``, ``added_count``, ``removed_count``, ``changed_count``, and
      ``next_cursor``. Each change has ``added_at``, ``added_in_version``, ``episode``, and
      ``change``, which is ``added``, ``removed``, or ``changed``. A changed episode also has
      ``previous_version`` and ``previous_episode``. The counts are totals across all pages.
   :rtype: dict
   :raises requests.exceptions.HTTPError: If the API returns an error, for example 404 when a
      version does not exist.

   .. code-block:: python

      diff = client.compare_dataset_versions(
          dataset_id="dset_123", version_number=2, base_version=1
      )

.. py:method:: Client.commit_dataset(*, dataset_id, description=...)

   Commit the draft as a new version and open the next draft.

   :param str dataset_id: Required. Dataset ID.
   :param description: Optional. Description of the committed version, 1–2048 characters. Omit it
      to keep the draft's description, or pass ``None`` to clear it.
   :type description: str or None
   :returns: A dict with ``committed`` and ``editable_version_number``. ``committed`` is the
      committed version, with the keys that :py:meth:`~Client.get_dataset_versions` lists.
      ``editable_version_number`` is the number of the new draft.
   :rtype: dict
   :raises requests.exceptions.HTTPError: If the API returns an error, for example 409 when the
      draft has no changes.

   .. code-block:: python

      client.commit_dataset(
          dataset_id="dset_123", description="Labels batch 12"
      )

.. py:method:: Client.discard_dataset(*, dataset_id)

   Discard the staged changes in a dataset's draft.

   :param str dataset_id: Required. Dataset ID.
   :returns: The counts ``discarded_adds``, ``discarded_removes``, and ``discarded_updates``.
   :rtype: dict
   :raises requests.exceptions.HTTPError: If the API returns an error, for example 404 when the
      dataset does not exist.

   .. code-block:: python

      counts = client.discard_dataset(dataset_id="dset_123")
      for name, count in counts.items():
          print(name, count)

.. py:method:: Client.restore_dataset_version(*, dataset_id, version_number, force=False)

   Stage the changes that make the draft match a committed version.

   :param str dataset_id: Required. Dataset ID.
   :param int version_number: Required. Committed version to restore.
   :param bool force: Optional, default ``False``. Replace the pending changes in the draft.
   :returns: The counts ``added``, ``updated``, ``removed``, ``discarded_adds``,
      ``discarded_removes``, and ``discarded_updates``.
   :rtype: dict
   :raises requests.exceptions.HTTPError: If the API returns an error, for example 409 when the
      draft has pending changes and ``force`` is ``False``.

   .. code-block:: python

      client.restore_dataset_version(
          dataset_id="dset_123", version_number=1
      )

.. py:method:: Client.clone_dataset_version(*, dataset_id, version_number, name, description=None, include_history=None)

   Create a new dataset from a committed version.

   :param str dataset_id: Required. Source dataset ID.
   :param int version_number: Required. Committed version to clone.
   :param str name: Required. Name of the new dataset, 1–100 characters. Unique in the Project,
      case-insensitive.
   :param str description: Optional, default ``None``. 1–500 characters.
   :param bool include_history: Optional, default ``None``, which the API treats as ``False``.
      ``True`` copies every version up to and including the cloned one.
   :returns: The new dataset, as :py:meth:`~Client.get_dataset` returns it.
   :rtype: dict
   :raises requests.exceptions.HTTPError: If the API returns an error, for example 409 when the
      name is taken.

   .. code-block:: python

      client.clone_dataset_version(
          dataset_id="dset_123",
          version_number=2,
          name="eval-holdout",
      )

.. py:method:: Client.download_dataset(*, dataset_id, version_number, output_directory, topics=None, include_properties=False)

   Export a committed version to a directory, as one MCAP file per episode and a
   ``manifest.json``.

   :param str dataset_id: Required. Dataset ID.
   :param int version_number: Required. Committed version number.
   :param output_directory: Required. An empty directory, or a path that does not exist yet.
   :type output_directory: str or os.PathLike
   :param list[str] topics: Optional, default ``None``, which exports every topic. Topics to
      export.
   :param bool include_properties: Optional, default ``False``. Add each episode's custom
      properties to the manifest.
   :returns: The output directory.
   :rtype: pathlib.Path
   :raises requests.exceptions.HTTPError: If the API returns an error.
   :raises RuntimeError: If the version is not committed, an episode in it has a missing
      recording, ``output_directory`` is a file or a directory with contents, or every episode
      fails to download.
   :raises OSError: If a file cannot be written or a transfer is interrupted.
   :Warns: :py:class:`DatasetDownloadWarning` -- If some episodes fail to download. The manifest
      records each one.

   .. code-block:: python

      client.download_dataset(
          dataset_id="dset_123",
          version_number=2,
          output_directory="out",
      )

Episodes
--------

.. py:method:: Client.create_episodes(*, project_id, episodes)

   Get or create up to 2,000 episodes. An item returns an existing episode when the time range,
   recordings, and custom properties of its latest version match.

   :param str project_id: Required. Project that owns the episodes.
   :param list[dict] episodes: Required. 1–2000 items. Each item has exactly one of
      ``recordings``, a list of recording IDs, or ``event_id``. An item with ``recordings`` can set
      ``start_time`` and ``end_time``, and takes its time range from the recordings when they are
      omitted. An item with ``event_id`` takes the event's time range and the completed recordings
      of its device that overlap it. Any item can set ``properties``, a dict of episode custom
      property values. All ``recordings`` lists together hold at most 100,000 recordings, and all
      ``event_id`` items together resolve to at most 10,000.
   :returns: One dict per item, in request order, with ``id`` and ``created``. ``created`` is
      ``False`` for an existing episode.
   :rtype: list[dict]
   :raises ValueError: If an item has both or neither of ``recordings`` and ``event_id``, or has
      ``event_id`` together with ``start_time`` or ``end_time``.
   :raises requests.exceptions.HTTPError: If the API returns an error.

   .. code-block:: python

      from datetime import datetime, timezone

      episodes = client.create_episodes(
          project_id="prj_123",
          episodes=[
              {
                  "recordings": ["rec_1"],
                  "start_time": datetime(
                      2026, 11, 2, 18, 0, tzinfo=timezone.utc
                  ),
                  "end_time": datetime(
                      2026, 11, 2, 18, 5, tzinfo=timezone.utc
                  ),
                  "properties": {"task": "fold_towel"},
              }
          ],
      )

.. py:method:: Client.get_episodes(*, project_id=None, start=None, end=None, has_missing_recordings=None, recording_id=None, exact_recording_ids=None, properties_query=None, sort_by=None, sort_order=None, limit=None, cursor=None, include_recordings=False, include_properties=False, include_creation_context=False)

   List episodes, each at its latest version.

   :param str project_id: Optional, default ``None``. Restrict to one Project.
   :param datetime.datetime start: Optional, default ``None``. Start of a time range. Episodes
      whose time window overlaps the range are returned. Pass it together with ``end``.
   :param datetime.datetime end: Optional, default ``None``. End of the time range.
   :param bool has_missing_recordings: Optional, default ``None``. ``True`` returns episodes with
      a missing recording. ``False`` returns episodes whose recordings are all available.
   :param str recording_id: Optional, default ``None``. Return episodes that contain this
      recording.
   :param list[str] exact_recording_ids: Optional, default ``None``. Return episodes whose
      recordings are exactly these 1–50 recording IDs.
   :param str properties_query: Optional, default ``None``. Space-separated ``key:value`` terms.
      Episodes that match every term are returned.
   :param str sort_by: Optional, default ``None``, which sorts by ``created_at``. One of
      ``created_at``, ``start_time``, or ``end_time``.
   :param str sort_order: Optional, default ``None``. ``asc`` or ``desc``.
   :param int limit: Optional, default ``None``, which returns up to 2,000 episodes. Page size,
      0–2000.
   :param str cursor: Optional, default ``None``. Cursor from a previous page.
   :param bool include_recordings: Optional, default ``False``. Add ``recordings`` and
      ``has_missing_recordings`` to each episode.
   :param bool include_properties: Optional, default ``False``. Add ``properties`` to each
      episode.
   :param bool include_creation_context: Optional, default ``False``. Add ``creation_context`` to
      each episode.
   :returns: A page of episodes, each with ``id``, ``project_id``, ``start_time``, ``end_time``,
      ``version``, ``latest_version``, ``creator_org_member_id``, ``creator_api_key_id``, and
      ``created_at``. Each item of ``recordings`` has ``id``, ``path``, ``location``, ``start``,
      ``end``, ``device_id``, and ``available``.
   :rtype: Page
   :raises ValueError: If only one of ``start`` and ``end`` is given, or ``exact_recording_ids``
      is empty.
   :raises requests.exceptions.HTTPError: If the API returns an error.

   .. code-block:: python

      page = client.get_episodes(project_id="prj_123")

.. py:method:: Client.get_episode(*, episode_id, include_recordings=False, include_properties=False, version=None)

   Return one episode.

   :param str episode_id: Required. Episode ID.
   :param bool include_recordings: Optional, default ``False``. Add ``recordings`` and
      ``has_missing_recordings``.
   :param bool include_properties: Optional, default ``False``. Add ``properties``.
   :param int version: Optional, default ``None``, which selects the latest version. Episode
      version to return.
   :returns: The episode, with ``id``, ``project_id``, ``start_time``, ``end_time``, ``version``,
      ``latest_version``, ``creator_org_member_id``, ``creator_api_key_id``, ``created_at``, and
      ``creation_context``. Each item of ``recordings`` has ``id``, ``path``, ``location``,
      ``start``, ``end``, ``device_id``, and ``available``.
   :rtype: dict
   :raises requests.exceptions.HTTPError: If the API returns an error, for example 404 when the
      episode does not exist.

   .. code-block:: python

      episode = client.get_episode(episode_id="ep_123")

.. py:method:: Client.update_episode(*, episode_id, applies_to_version=None, start_time=None, end_time=None, recordings=None, properties=None)

   Update one episode. An update that changes the episode creates a new episode version.

   :param str episode_id: Required. Episode ID.
   :param int applies_to_version: Optional, default ``None``. Episode version that the update
      applies to. The API rejects the update when it is not the latest version.
   :param datetime.datetime start_time: Optional, default ``None``. New inclusive start of the
      episode window.
   :param datetime.datetime end_time: Optional, default ``None``. New inclusive end of the episode
      window.
   :param dict recordings: Optional, default ``None``. Recording IDs to add and remove, as
      ``{"add": [...], "remove": [...]}``.
   :param dict properties: Optional, default ``None``. Custom property values to merge into the
      episode's properties. A value of ``None`` unsets its key.
   :returns: The episode at the resulting version, with the keys that
      :py:meth:`~Client.get_episodes` lists.
   :rtype: dict
   :raises requests.exceptions.HTTPError: If the API returns an error, for example when
      ``applies_to_version`` is not the latest version.

   .. code-block:: python

      client.update_episode(
          episode_id="ep_123", properties={"outcome": "success"}
      )

.. py:method:: Client.update_episodes(*, episodes)

   Update up to 2,000 episodes in one transaction. If one update fails, none is applied.

   :param list[dict] episodes: Required. At most 2,000 items. Each item has ``id`` and can set
      ``applies_to_version``, ``start_time``, ``end_time``, ``recordings``, and ``properties``,
      which have the same meaning as the parameters of :py:meth:`~Client.update_episode`.
   :returns: One dict per item, in request order, with ``id`` and ``version``, the episode's
      resulting version.
   :rtype: list[dict]
   :raises KeyError: If an item has no ``id``.
   :raises requests.exceptions.HTTPError: If the API returns an error.

   .. code-block:: python

      client.update_episodes(
          episodes=[
              {
                  "id": "ep_123",
                  "properties": {"outcome": "success"},
              }
          ]
      )

.. py:method:: Client.delete_episode(*, episode_id)

   Delete an episode that no dataset holds.

   :param str episode_id: Required. Episode ID.
   :returns: ``None``.
   :raises requests.exceptions.HTTPError: If the API returns an error, for example 409 when a
      dataset holds the episode.

   .. code-block:: python

      client.delete_episode(episode_id="ep_123")

.. py:method:: Client.get_episode_versions(*, episode_id, limit=None, cursor=None)

   List an episode's versions, newest first.

   :param str episode_id: Required. Episode ID.
   :param int limit: Optional, default ``None``, which returns up to 2,000 versions. Page size,
      0–2000.
   :param str cursor: Optional, default ``None``. Cursor from a previous page.
   :returns: A page of dicts with ``version``, ``created_at``, ``creator_org_member_id``,
      ``creator_api_key_id``, and ``changed_fields``.
   :rtype: Page
   :raises requests.exceptions.HTTPError: If the API returns an error, for example 404 when the
      episode does not exist.

   .. code-block:: python

      page = client.get_episode_versions(episode_id="ep_123")

.. py:method:: Client.iter_messages(*, device_id=None, device_name=None, session_id=None, session_key=None, start=None, end=None, topics=None, decoder_factories=None, project_id=None, episode_id=None, episode_version=None, recording_id=None, recording_key=None, replay_policy=None, replay_lookback_seconds=None)

   Stream decoded messages. Messages are returned in log-time order across the selected topics.

   ``start`` and ``end`` are required unless ``episode_id`` is given, or ``recording_id`` or
   ``recording_key`` is given without a device.

   :param str device_id: Optional, default ``None``. Device that originated the data. Requires
      ``start`` and ``end``.
   :param str device_name: Optional, default ``None``. Name of the device that originated the
      data. Requires ``start`` and ``end``.
   :param str session_id: Optional, default ``None``. Session to stream. Requires ``start`` and
      ``end``.
   :param str session_key: Optional, default ``None``. Key of the session to stream. Requires
      ``start`` and ``end``.
   :param datetime.datetime start: Optional, default ``None``. Inclusive start of the stream. Pass
      it together with ``end``.
   :param datetime.datetime end: Optional, default ``None``. Inclusive end of the stream.
   :param list[str] topics: Optional, default ``None``, which streams every topic. Topics to
      stream.
   :param decoder_factories: Optional, default ``None``, which uses the client's default
      decoders: JSON, plus Protobuf, ROS 1, and ROS 2 when the matching ``mcap`` support package is
      installed. MCAP decoder factories to use instead.
   :type decoder_factories: list[~mcap.decoder.DecoderFactory]
   :param str project_id: Optional, default ``None``. Project of the device or session. Required
      with ``session_key``, and with ``device_name`` when the name exists in more than one Project.
   :param str episode_id: Optional, default ``None``. Episode to stream. Its time range is used
      when ``start`` and ``end`` are omitted.
   :param int episode_version: Optional, default ``None``, which selects the latest version.
      Episode version to stream. Requires ``episode_id``.
   :param str recording_id: Optional, default ``None``. Recording to stream.
   :param str recording_key: Optional, default ``None``. Key of the recording to stream.
   :param str replay_policy: Optional, default ``None``. ``lastPerChannel`` adds the most recent
      message on each channel from before ``start``.
   :param float replay_lookback_seconds: Optional, default ``None``. How far before ``start`` to
      look for those messages. Used with ``replay_policy="lastPerChannel"``.
   :returns: A generator of ``(schema, channel, message, decoded_message)`` tuples.
   :raises RuntimeError: If no device, session, recording, or episode is given, ``episode_id`` is
      combined with another of them, a recording is combined with a session, ``episode_version``
      is given without ``episode_id``, or ``start`` and ``end`` are missing when required.
   :raises requests.exceptions.HTTPError: If the API returns an error.

   .. code-block:: python

      for schema, channel, message, decoded in client.iter_messages(
          recording_id="rec_123",
          topics=["/camera/front"],
      ):
          print(channel.topic, message.log_time)

.. py:method:: Client.download_data(*, device_id=None, device_name=None, session_id=None, session_key=None, start=None, end=None, topics=None, output_format=OutputFormat.mcap, compression_format=None, callback=None, project_id=None, episode_id=None, episode_version=None, include_attachments=None, recording_id=None, recording_key=None, replay_policy=None, replay_lookback_seconds=None)

   Download the data of a device, session, recording, or episode as bytes.

   ``start`` and ``end`` are required unless ``episode_id`` is given, or ``recording_id`` or
   ``recording_key`` is given without a device.

   :param str device_id: Optional, default ``None``. Device that originated the data. Requires
      ``start`` and ``end``.
   :param str device_name: Optional, default ``None``. Name of the device that originated the
      data. Requires ``start`` and ``end``.
   :param str session_id: Optional, default ``None``. Session to download. Requires ``start`` and
      ``end``.
   :param str session_key: Optional, default ``None``. Key of the session to download. Requires
      ``start`` and ``end``.
   :param datetime.datetime start: Optional, default ``None``. Inclusive start of the time range.
      Pass it together with ``end``.
   :param datetime.datetime end: Optional, default ``None``. Inclusive end of the time range.
   :param list[str] topics: Optional, default ``None``, which downloads every topic. Topics to
      download.
   :param OutputFormat output_format: Optional, default ``OutputFormat.mcap``. Format of the
      returned data.
   :param CompressionFormat compression_format: Optional, default ``None``, for which the API
      uses LZ4. Compression of the chunks in MCAP output.
   :param callback: Optional, default ``None``. A function that the client calls as
      ``callback(progress=n)`` during the download, where ``n`` is the number of bytes received
      so far.
   :param str project_id: Optional, default ``None``. Project of the device or session. Required
      with ``session_key``, and with ``device_name`` when the name exists in more than one Project.
   :param str episode_id: Optional, default ``None``. Episode to download. Its time range is used
      when ``start`` and ``end`` are omitted.
   :param int episode_version: Optional, default ``None``, which selects the latest version.
      Episode version to download. Requires ``episode_id``.
   :param bool include_attachments: Optional, default ``None``. Include MCAP attachments. Applies
      to MCAP output for a recording, session, or episode.
   :param str recording_id: Optional, default ``None``. Recording to download.
   :param str recording_key: Optional, default ``None``. Key of the recording to download.
   :param str replay_policy: Optional, default ``None``. ``lastPerChannel`` adds the most recent
      message on each channel from before ``start``.
   :param float replay_lookback_seconds: Optional, default ``None``. How far before ``start`` to
      look for those messages. Used with ``replay_policy="lastPerChannel"``.
   :returns: The data in ``output_format``.
   :rtype: bytes
   :raises RuntimeError: If no device, session, recording, or episode is given, ``episode_id`` is
      combined with another of them, a recording is combined with a session, ``episode_version``
      is given without ``episode_id``, or ``start`` and ``end`` are missing when required.
   :raises requests.exceptions.HTTPError: If the API returns an error.

   .. code-block:: python

      data = client.download_data(
          episode_id="ep_123", topics=["/camera/front"]
      )

.. py:method:: Client.get_topics(*, device_id=None, device_name=None, start=None, end=None, include_schemas=False, project_id=None, session_id=None, session_key=None, episode_id=None, limit=None, offset=None, recording_id=None, recording_key=None, episode_version=None, sort_by=None, sort_order=None)

   List the topics of a device, session, recording, or episode.

   ``start`` and ``end`` are required with ``device_id`` or ``device_name``.
   Session, recording, and episode queries can omit them.

   :param str device_id: Optional, default ``None``. Device to list topics for.
   :param str device_name: Optional, default ``None``. Name of the device to list topics for.
   :param datetime.datetime start: Optional, default ``None``. Start of the time range. Pass it
      together with ``end``.
   :param datetime.datetime end: Optional, default ``None``. End of the time range.
   :param bool include_schemas: Optional, default ``False``. Add ``schema`` to each topic.
   :param str project_id: Optional, default ``None``. Restrict to one Project.
   :param str session_id: Optional, default ``None``. Session to list topics for.
   :param str session_key: Optional, default ``None``. Key of the session to list topics for.
   :param str episode_id: Optional, default ``None``. Episode to list topics for. Its time range
      is used when ``start`` and ``end`` are omitted.
   :param int limit: Optional, default ``None``, which returns up to 2,000 topics. Page size,
      0–2000.
   :param int offset: Optional, default ``None``. Number of topics to skip.
   :param str recording_id: Optional, default ``None``. Recording to list topics for.
   :param str recording_key: Optional, default ``None``. Key of the recording to list topics for.
   :param int episode_version: Optional, default ``None``, which selects the latest version.
      Episode version to list topics for. Requires ``episode_id``.
   :param str sort_by: Optional, default ``None``. ``topic`` or ``version``.
   :param str sort_order: Optional, default ``None``. ``asc`` or ``desc``.
   :returns: One dict per topic, with ``topic``, ``version``, ``encoding``, ``schema_encoding``,
      and ``schema_name``. With ``include_schemas``, each also has ``schema``, as ``bytes``
      or ``None`` when no schema is available.
   :rtype: list[dict]
   :raises RuntimeError: If ``episode_version`` is given without ``episode_id``, ``episode_id`` is
      combined with a device, session, or recording, or ``start`` and ``end`` are missing when
      required.
   :raises requests.exceptions.HTTPError: If the API returns an error.

   .. code-block:: python

      topics = client.get_topics(episode_id="ep_123")
      print([t["topic"] for t in topics])

Data coverage
-------------

.. py:method:: Client.get_coverage(*, start=None, end=None, device_id=None, device_name=None, tolerance=None, project_id=None, session_id=None, session_key=None, recording_id=None, recording_key=None, include_edge_recordings=None)

   List the time ranges for which Foxglove has data.

   :param datetime.datetime start: Optional, default ``None``. Start of the time range. Required
      with ``end`` unless a recording or session is given.
   :param datetime.datetime end: Optional, default ``None``. End of the time range.
   :param str device_id: Optional, default ``None``. Restrict to one device.
   :param str device_name: Optional, default ``None``. Restrict to the device with this name.
   :param int tolerance: Optional, default ``None``. Minimum gap, in seconds, for two ranges to
      count as separate.
   :param str project_id: Optional, default ``None``. Restrict to one Project.
   :param str session_id: Optional, default ``None``. Restrict to one session.
   :param str session_key: Optional, default ``None``. Restrict to the session with this key.
   :param str recording_id: Optional, default ``None``. Restrict to one recording.
   :param str recording_key: Optional, default ``None``. Restrict to the recording with this key.
   :param bool include_edge_recordings: Optional, default ``None``, which the API treats as
      ``False``. Include recordings from an Edge Site or a device running Foxlet. Each range then
      has an ``import_status``.
   :returns: One dict per range, with ``device_id``, ``device``, ``start``, ``end``, and
      ``import_status``. ``device`` is the device summary as the API returns it.
   :rtype: list[dict]
   :raises requests.exceptions.HTTPError: If the API returns an error.

   .. code-block:: python

      from datetime import datetime, timezone

      ranges = client.get_coverage(
          device_id="dev_123",
          start=datetime(2026, 11, 2, tzinfo=timezone.utc),
          end=datetime(2026, 11, 3, tzinfo=timezone.utc),
      )

Custom properties
-----------------

The ``properties`` of an episode use keys that are defined as custom properties with
``resource_type="episode"``.

.. py:method:: Client.create_custom_property(*, key, label, resource_type, value_type, enum_values=None)

   Create a custom property.

   :param str key: Required. Immutable identifier used in queries. 1–32 characters from
      ``A-Z``, ``a-z``, ``0-9``, ``_``, and ``-``.
   :param str label: Required. Display label, 1–32 characters.
   :param str resource_type: Required. ``device``, ``event``, ``session``, or ``episode``.
   :param str value_type: Required. ``string``, ``multiline-string``, ``number``, ``enum``,
      ``boolean``, or ``multi-enum``.
   :param list[str] enum_values: Optional, default ``None``. Allowed values. Required when
      ``value_type`` is ``enum`` or ``multi-enum``.
   :returns: The custom property, with ``id``, ``key``, ``label``, ``resource_type``,
      ``value_type``, ``enum_values``, and ``has_associated_data``.
   :rtype: dict
   :raises requests.exceptions.HTTPError: If the API returns an error.

   .. code-block:: python

      client.create_custom_property(
          key="outcome",
          label="Outcome",
          resource_type="episode",
          value_type="enum",
          enum_values=["success", "failure"],
      )

.. py:method:: Client.get_custom_properties(*, resource_type=None)

   List custom properties.

   :param str resource_type: Optional, default ``None``, which returns every type. ``device``,
      ``event``, ``session``, or ``episode``.
   :returns: The custom properties, each as :py:meth:`~Client.get_custom_property` returns it.
   :rtype: list[dict]
   :raises requests.exceptions.HTTPError: If the API returns an error.

   .. code-block:: python

      props = client.get_custom_properties(resource_type="episode")

.. py:method:: Client.get_custom_property(*, custom_property_id)

   Return one custom property.

   :param str custom_property_id: Required. Custom property ID.
   :returns: The custom property, with ``id``, ``key``, ``label``, ``resource_type``,
      ``value_type``, ``enum_values``, and ``has_associated_data``.
   :rtype: dict
   :raises requests.exceptions.HTTPError: If the API returns an error.

   .. code-block:: python

      prop = client.get_custom_property(custom_property_id="cp_123")

.. py:method:: Client.update_custom_property(*, custom_property_id, label=None, enum_values=None)

   Update a custom property's label, allowed values, or both.

   :param str custom_property_id: Required. Custom property ID.
   :param str label: Optional, default ``None``, which leaves the label unchanged. New display
      label, 1–32 characters.
   :param list[str] enum_values: Optional, default ``None``, which leaves the values unchanged.
      New list of allowed values. It contains every existing value and can add or reorder values.
   :returns: The custom property, as :py:meth:`~Client.get_custom_property` returns it.
   :rtype: dict
   :raises requests.exceptions.HTTPError: If the API returns an error.

   .. code-block:: python

      client.update_custom_property(
          custom_property_id="cp_123", label="Outcome"
      )

.. py:method:: Client.delete_custom_property(*, custom_property_id)

   Delete a custom property.

   :param str custom_property_id: Required. Custom property ID.
   :returns: ``None``.
   :raises requests.exceptions.HTTPError: If the API returns an error.

   .. code-block:: python

      client.delete_custom_property(custom_property_id="cp_123")

Supporting types
----------------

.. py:class:: Page

   One page of results from a list method.

   .. py:attribute:: items

      The results on this page, as a list.

   .. py:attribute:: next_cursor

      Cursor of the next page, or ``None`` on the last page.

   .. py:attribute:: previous_cursor

      Cursor of the previous page, or ``None`` when the API returns none.

   .. py:method:: auto_paging_iter()

      Yield the results on this page, then fetch each following page and yield its results.

      :returns: An iterator over the results.
      :raises requests.exceptions.HTTPError: If the API returns an error for a following page.

      .. code-block:: python

         for dataset in client.get_datasets().auto_paging_iter():
             print(dataset["name"])

.. py:class:: DatasetDownloadWarning

   Warning that :py:meth:`Client.download_dataset` emits when some episodes fail to download. A
   subclass of ``UserWarning``.

.. py:class:: OutputFormat

   Output formats for :py:meth:`Client.download_data`. An ``enum.Enum``.

   .. py:attribute:: bag
      :value: "bag1"

      A ``.bag`` file.

   .. py:attribute:: mcap
      :value: "mcap"

      An ``.mcap`` file.

   .. py:attribute:: mcap0
      :value: "mcap0"

      Deprecated. Use ``mcap``.

.. py:class:: CompressionFormat

   Chunk compression formats for :py:meth:`Client.download_data`. An ``enum.Enum``.

   .. py:attribute:: none
      :value: ""

      No compression.

   .. py:attribute:: zstd
      :value: "zstd"

      Zstandard compression.

   .. py:attribute:: lz4
      :value: "lz4"

      LZ4 compression.
