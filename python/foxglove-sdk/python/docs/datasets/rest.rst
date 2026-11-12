REST map
========

Base URL: ``https://api.foxglove.dev/v1``. Authenticate with
``Authorization: Bearer <api-key>``. Each endpoint lists the API key
**capability** it needs.

Generated field-level reference:

- https://docs.foxglove.dev/api/datasets
- https://docs.foxglove.dev/api/episodes
- https://docs.foxglove.dev/api/custom-properties

Python names: :doc:`python-client` (``foxglove-client>=0.23``). This page
uses API camelCase.

List endpoints take ``limit`` (0–2000, default 2000) and ``cursor``, and
return ``fg-pagination-next-cursor`` (and, where supported,
``fg-pagination-previous-cursor``) headers, except ``GET /custom-properties``,
which takes neither. Compare also returns ``nextCursor`` in the body.

Datasets
--------

.. list-table::
   :header-rows: 1
   :widths: 10 42 18 30

   * - Method
     - Path
     - Capability
     - Purpose
   * - POST
     - ``/datasets``
     - ``datasets.create``
     - Create a dataset, optionally seeding episodes
   * - GET
     - ``/datasets``
     - ``datasets.list``
     - List datasets (JSON **array**, not wrapped)
   * - GET
     - ``/datasets/{id}``
     - ``datasets.list``
     - Get a dataset
   * - PATCH
     - ``/datasets/{id}``
     - ``datasets.update``
     - Rename or describe a dataset
   * - DELETE
     - ``/datasets/{id}``
     - ``datasets.delete``
     - Delete the dataset, not its episodes
   * - GET
     - ``/datasets/{id}/episodes``
     - ``datasets.list``
     - Membership on the latest committed version (or initial draft)
   * - PATCH
     - ``/datasets/{id}/episodes``
     - ``datasets.update``
     - Stage draft add/update/remove (does not commit)
   * - GET
     - ``/datasets/{id}/versions``
     - ``datasets.list``
     - List versions, including the draft
   * - GET
     - ``/datasets/{id}/versions/{versionNumber}``
     - ``datasets.list``
     - Get one version
   * - PATCH
     - ``/datasets/{id}/versions/{versionNumber}``
     - ``datasets.update``
     - Set that version's description
   * - GET
     - ``/datasets/{id}/versions/{versionNumber}/episodes``
     - ``datasets.list``
     - Membership pinned in that version
   * - GET
     - ``/datasets/{id}/versions/{versionNumber}/compare``
     - ``datasets.list``
     - Diff against query ``version`` (the base)
   * - POST
     - ``/datasets/{id}/commit``
     - ``datasets.update``
     - Commit the draft (409 if no changes)
   * - POST
     - ``/datasets/{id}/discard``
     - ``datasets.update``
     - Discard draft changes
   * - POST
     - ``/datasets/{id}/versions/{versionNumber}/restore``
     - ``datasets.update``
     - Restore into the draft (409 if pending changes and ``force`` is false)
   * - POST
     - ``/datasets/{id}/versions/{versionNumber}/clone``
     - ``datasets.create``
     - Clone a committed version (409 name taken; 400 ``DatasetSpansPrimarySites``)

.. code-block:: bash

   curl -X POST https://api.foxglove.dev/v1/datasets \
     -H "Authorization: Bearer $FOXGLOVE_API_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"projectId":"prj_123","name":"Training candidates"}'

Episodes
--------

.. list-table::
   :header-rows: 1
   :widths: 10 42 18 30

   * - Method
     - Path
     - Capability
     - Purpose
   * - POST
     - ``/episodes``
     - ``episodes.create``
     - Get-or-create; response ``{"episodes":[{"id","created"}]}``
   * - PATCH
     - ``/episodes``
     - ``episodes.update``
     - Atomic bulk update
   * - GET
     - ``/episodes``
     - ``episodes.list``
     - List latest versions
   * - GET
     - ``/episodes/{id}``
     - ``episodes.list``
     - Get one episode (``version`` defaults to latest)
   * - PATCH
     - ``/episodes/{id}``
     - ``episodes.update``
     - Update one episode
   * - DELETE
     - ``/episodes/{id}``
     - ``episodes.delete``
     - Delete if no dataset holds it
   * - GET
     - ``/episodes/{id}/versions``
     - ``episodes.list``
     - List episode versions, newest first

Items with ``created: false`` reuse an existing episode.

.. code-block:: bash

   curl -X POST https://api.foxglove.dev/v1/episodes \
     -H "Authorization: Bearer $FOXGLOVE_API_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"projectId":"prj_123","episodes":[{"recordings":["rec_1"]}]}'

Data lookup
-----------

``iter_messages`` calls ``POST /data/stream``, which returns a ``link``;
GET that URL to read bytes. ``GET /data/topics`` backs ``get_topics``.
``GET /data/coverage`` backs ``get_coverage``.

.. list-table::
   :header-rows: 1
   :widths: 10 42 18 30

   * - Method
     - Path
     - Capability
     - Purpose
   * - POST
     - ``/data/stream``
     - ``data.stream``
     - Request an MCAP stream link
   * - GET
     - ``/data/topics``
     - ``data.topics.list``
     - List topics
   * - GET
     - ``/data/coverage``
     - ``data.coverage.list``
     - List coverage ranges

Custom properties
-----------------

.. list-table::
   :header-rows: 1
   :widths: 10 42 18 30

   * - Method
     - Path
     - Capability
     - Purpose
   * - POST
     - ``/custom-properties``
     - ``properties.create``
     - Create a key (``resourceType`` includes ``episode``)
   * - GET
     - ``/custom-properties``
     - ``properties.list``
     - List; filter with ``resourceType=episode``
   * - GET
     - ``/custom-properties/{id}``
     - ``properties.list``
     - Get one
   * - PATCH
     - ``/custom-properties/{id}``
     - ``properties.update``
     - ``label`` and ``enumValues`` only
   * - DELETE
     - ``/custom-properties/{id}``
     - ``properties.delete``
     - Delete the key

.. code-block:: bash

   curl -X POST https://api.foxglove.dev/v1/custom-properties \
     -H "Authorization: Bearer $FOXGLOVE_API_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"key":"outcome","label":"Outcome","resourceType":"episode","valueType":"enum","enumValues":["success","failure"]}'
