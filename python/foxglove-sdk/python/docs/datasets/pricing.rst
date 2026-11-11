Episode pricing
===============

Each episode is charged once, when it is created. Datasets, dataset versions,
and episode versions are free. There is no monthly charge to keep episodes.
Deleting an episode does not refund its charge. Storage, query, indexing, and
bandwidth follow their own meters.

The first 1,000 episodes each month are free. On Pro, more episodes are
billed on a marginal tier. The count resets each month:

.. list-table::
   :header-rows: 1

   * - Episode creations in the month
     - Price per additional episode
   * - First 1,000
     - Free
   * - 1,001–10,000
     - $0.01
   * - 10,001–100,000
     - $0.005
   * - 100,001–1,000,000
     - $0.0025
   * - Above 1,000,000
     - $0.000625

Creating an episode that matches an existing latest version (same time
range, recordings, and custom properties) reuses that episode. The create
call still returns it; reuse is not a new creation.

Plan pages: https://foxglove.dev/pricing

Episode billing starts on 16 November 2026.
