"""A `Ray Data <https://docs.ray.io/en/latest/data/data.html>`__ datasource for Foxglove.

:class:`FoxgloveDataset` is deliberately *domain-agnostic*: it knows how to fetch recordings from
the Foxglove cloud in parallel across the cluster, and nothing about the contents of those
recordings. The only contract a datasource implements is ``get_read_tasks(parallelism)`` ->
``list[ReadTask]``. Each :class:`~ray.data.datasource.ReadTask` is shipped to a (possibly remote)
worker, run there, and yields one or more blocks. The driver does only the *cheap* planning call
(list the recordings in the dataset); the per-recording download happens inside the read functions,
on workers.

We emit **one row per recording**, the raw MCAP bytes carried in a ``"recording"`` column. Decoding
the MCAP into domain-specific samples is left to a downstream Ray Data transform (e.g.
``.flat_map(...)``) in your training/analysis script, so this datasource stays reusable across
projects.
"""

from __future__ import annotations

from typing import Callable, Iterator

import pandas as pd
from ray.data.block import BlockMetadata
from ray.data.datasource import Datasource, ReadTask


class FoxgloveDataset(Datasource):
    """A Ray Data datasource that downloads Foxglove recordings as raw MCAP bytes.

    Use it with ``ray.data.read_datasource``:

    .. code-block:: python

        import ray
        from foxglove.ray import FoxgloveDataset

        ds = ray.data.read_datasource(
            FoxgloveDataset(token="fox_sk_...", dataset_id="ds_..."),
            parallelism=8,
        )

    Each row is a dict ``{"recording": {"id": str, "mcap": bytes}}``. Decode the MCAP bytes
    downstream, for example with ``ds.flat_map(...)``.

    This class is only available when the ``ray`` extra is installed. Install it with
    ``pip install "foxglove-sdk[ray]"``.
    """

    def __init__(
        self,
        *,
        token: str,
        dataset_id: str,
        host: str = "api.foxglove.dev",
    ):
        """
        :param token: A Foxglove API token used to authenticate with the cloud API.
        :param dataset_id: The id of the Foxglove dataset whose recordings should be downloaded.
        :param host: The Foxglove API host. Defaults to ``api.foxglove.dev``.
        """
        # Keep config (strings), NOT a live Client, so this datasource and the ReadTasks it
        # produces pickle cleanly to remote workers -- an open socket would not.
        self._token = token
        self._host = host
        self._dataset_id = dataset_id
        # Plan on the driver: one cheap index call to list the recordings in the dataset.
        self._recording_ids = self._resolve_recording_ids()

    def _resolve_recording_ids(self) -> list[str]:
        """Resolve the dataset id to the list of recording ids it contains.

        This is a cheap, driver-side index call: it should return one small id per recording so the
        driver can plan the read without downloading any data.
        """
        # TODO: foxglove-client does not yet expose a dataset API. Once it does, create a fresh
        # Client(self._token, self._host) here and call it to list the recordings belonging to
        # self._dataset_id, returning their ids. Until then this is intentionally unimplemented.
        raise NotImplementedError(
            "dataset_id resolution requires foxglove-client dataset support (follow-up)"
        )

    def estimate_inmemory_data_size(self) -> int | None:
        return None

    def get_read_tasks(
        self,
        parallelism: int,
        per_task_row_limit: int | None = None,
        data_context: object | None = None,
    ) -> list[ReadTask]:
        # Spread recordings across ~parallelism buckets; each bucket becomes one ReadTask that Ray
        # schedules on its own worker. (Ray derives `parallelism` from cluster size + target block
        # size; we cap it at the number of recordings we actually have.)
        n_buckets = max(1, min(parallelism, len(self._recording_ids)))
        buckets: list[list[str]] = [[] for _ in range(n_buckets)]
        for i, rec_id in enumerate(self._recording_ids):
            buckets[i % n_buckets].append(rec_id)

        token, host = self._token, self._host  # capture plain config for the closure

        read_tasks: list[ReadTask] = []
        for bucket in buckets:
            if not bucket:
                continue
            # One emitted row per recording, so num_rows here is the recording count.
            metadata = BlockMetadata(
                num_rows=len(bucket),
                size_bytes=None,
                input_files=None,
                exec_stats=None,
            )

            def make_read_fn(
                recording_ids: list[str],
            ) -> Callable[[], Iterator["pd.DataFrame"]]:
                def read_fn() -> Iterator["pd.DataFrame"]:
                    # Imported lazily and constructed fresh ON the worker, so the module loads
                    # without creds and no live client is captured in the pickled closure.
                    from foxglove.client import Client

                    client = Client(token=token, host=host)
                    for rec_id in recording_ids:
                        mcap_bytes = client.download_recording_data(id=rec_id)
                        # One block per recording -> Ray can stream recording k+1's download while
                        # recording k is already feeding downstream operators.
                        record = {"id": rec_id, "mcap": mcap_bytes}
                        yield pd.DataFrame({"recording": [record]})

                return read_fn

            read_tasks.append(ReadTask(make_read_fn(bucket), metadata))

        return read_tasks
