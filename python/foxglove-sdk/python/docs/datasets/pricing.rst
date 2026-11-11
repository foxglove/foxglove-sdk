Episode pricing
===============

Episodes are billed when they are created. Creating a dataset or a dataset
version is not billed. New versions of an existing episode are not billed.
There is no monthly charge to keep episodes. Deleting an episode does not
refund a creation charge. Storage, query, indexing, and bandwidth still
follow their own meters.

The first 1,000 episode creations each month are included. After that,
additional creations are billed on a marginal tier:

.. list-table::
   :header-rows: 1

   * - Episode creations in the month
     - Price per additional episode
   * - First 1,000
     - Included
   * - 1,001–10,000
     - $0.01
   * - 10,001–100,000
     - $0.005
   * - 100,001–1,000,000
     - $0.0025
   * - Above 1,000,000
     - $0.000625

Updating an episode creates a new episode version. That is not a new episode
and is not billed as a creation.

Creating an episode that matches an existing latest version (same time
range, recordings, and custom properties) reuses that episode. The create
call still returns it; reuse is not a new creation.

Plan pages: https://foxglove.dev/pricing

This rate takes effect on 16 November 2026.
