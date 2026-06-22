Ray Data integration
====================

Classes for loading Foxglove recordings as a `Ray Data
<https://docs.ray.io/en/latest/data/data.html>`__ dataset, downloading them in parallel across a
Ray cluster.

.. note::
   The Ray Data integration is only available when the ``ray`` extra package is installed.
   Install it with ``pip install foxglove-sdk[ray]``.

:class:`~foxglove.ray.datasource.FoxgloveDataset` is used with ``ray.data.read_datasource``:

.. code-block:: python

   import ray
   from foxglove.ray import FoxgloveDataset

   ds = ray.data.read_datasource(
       FoxgloveDataset(token="fox_sk_...", dataset_id="ds_..."),
       parallelism=8,
   )

Each row is a dict ``{"recording": {"id": str, "mcap": bytes}}``. Decode the MCAP bytes downstream,
for example with ``ds.flat_map(...)``.

.. autoclass:: foxglove.ray.datasource.FoxgloveDataset
   :members:
   :exclude-members: get_read_tasks, estimate_inmemory_data_size
