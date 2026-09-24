"""A custom Ray Data datasource for the Foxglove recording cloud API.

This is deliberately *domain-agnostic*: it knows how to fetch episodes from the API in
parallel across the cluster, and nothing about ACT, states, actions, images, or chunking.
The only real contract a datasource implements is ``get_read_tasks(parallelism)`` ->
``list[ReadTask]``. Each ReadTask is shipped to a (possibly remote) worker, run there, and
yields one or more blocks. The driver does only the *cheap* planning call (list shards);
the per-episode download happens inside the read functions, on workers.

We slice the source on "episode id" -- the natural shard the API exposes -- and emit **one
row per raw episode**, the whole payload carried opaquely in an ``"episode"`` column. The
domain-specific expansion (episode -> per-frame training samples) is a Ray Data transform
that lives in the training script, applied with ``.flat_map(...)`` after ``read_datasource``.
So this file is reusable for any Foxglove-backed project, not just ACT/LeRobot.
"""

from __future__ import annotations

import inspect
import logging
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from typing import Any
from urllib.parse import urlparse

import pandas as pd
import requests
from ray.data.block import BlockMetadata
from ray.data.datasource import Datasource, ReadTask

_EPISODES_PAGE = 100  # page size for the paginated dataset-episodes listing
_LOGGER = logging.getLogger(__name__)


@dataclass
class FoxgloveClient:
    """HTTP client for the Foxglove dataset-episodes API.

    Stores only strings so it pickles cleanly onto Ray workers. Construct a fresh client
    on each worker rather than capturing a live session.
    """

    base_url: str
    api_key: str  # a Foxglove API token
    dataset_id: str

    def _v1_url(self, path: str) -> str:
        """Build a ``https://{host}/v1{path}`` URL.

        Accepts a ``base_url`` that is a bare host or a full URL, with or without a
        scheme.
        """
        parsed = urlparse(
            self.base_url if "://" in self.base_url else f"https://{self.base_url}"
        )
        return f"{parsed.scheme}://{parsed.netloc}/v1{path}"

    def _headers(self) -> dict[str, str]:
        return {
            "Authorization": f"Bearer {self.api_key}",
            "Content-Type": "application/json",
        }

    def list_episodes(self) -> list[dict[str, str]]:
        """Cheap index call: the dataset's episode ids, one small descriptor per shard.

        Small and fast so the driver can plan the read without downloading any recordings.
        Pages through ``GET /datasets/{id}/episodes`` so the full episode set is returned
        regardless of its size.
        """
        episodes: list[dict[str, str]] = []
        offset = 0
        while True:
            resp = requests.get(
                self._v1_url(f"/datasets/{self.dataset_id}/episodes"),
                headers=self._headers(),
                params={"limit": _EPISODES_PAGE, "offset": offset},
                timeout=60,
            )
            resp.raise_for_status()
            batch = resp.json()["episodes"]
            episodes.extend({"id": entry["episode"]["id"]} for entry in batch)
            if len(batch) < _EPISODES_PAGE:
                return episodes
            offset += _EPISODES_PAGE

    def fetch_episode(self, episode_id: str) -> dict[str, Any]:
        """Stream one episode's MCAP from the Foxglove cloud and return its raw bytes.

        Runs on a worker. Streams by ``episodeId`` (start/end default to the episode's
        time window); the ``/data/stream`` endpoint returns a short-lived signed link we
        then download. Parsing the MCAP into training features is the script's concern,
        so this stays domain-agnostic.
        """
        _LOGGER.info("streaming episode %s", episode_id)
        link_resp = requests.post(
            self._v1_url("/data/stream"),
            headers=self._headers(),
            json={"episodeId": episode_id},
            timeout=60,
        )
        link_resp.raise_for_status()
        link = link_resp.json()["link"]
        data_resp = requests.get(link, stream=True, timeout=600)
        data_resp.raise_for_status()
        mcap_bytes = data_resp.content
        _LOGGER.info(
            "streamed episode %s: %s bytes", episode_id, f"{len(mcap_bytes):,}"
        )
        return {"episode_id": episode_id, "mcap": mcap_bytes}


def _block_metadata(num_rows: int) -> BlockMetadata:
    """Build ``BlockMetadata`` across Ray versions that reshuffled this constructor."""
    params = inspect.signature(BlockMetadata).parameters
    kwargs: dict[str, Any] = {"num_rows": num_rows, "size_bytes": None}
    for name in ("schema", "input_files", "exec_stats"):
        if name in params:
            kwargs[name] = None
    return BlockMetadata(**kwargs)


class FoxgloveDataset(Datasource):
    """A Ray Data datasource that downloads Foxglove dataset episodes as raw MCAP bytes.

    Use it with ``ray.data.read_datasource``:

    .. code-block:: python

        import ray
        from foxglove.ray import FoxgloveDataset

        ds = ray.data.read_datasource(
            FoxgloveDataset(api_key="fox_sk_...", dataset_id="ds_..."),
            parallelism=8,
        )

    Each row is a dict ``{"episode": {"episode_id": str, "mcap": bytes}}``. Decode the
    MCAP bytes downstream, for example with ``ds.flat_map(...)``.

    This class is only available when the ``ray`` extra is installed. Install it with
    ``pip install "foxglove-sdk[ray]"``.
    """

    def __init__(
        self,
        *,
        api_key: str,
        dataset_id: str,
        base_url: str = "https://api.foxglove.dev",
    ) -> None:
        """
        :param api_key: A Foxglove API token used to authenticate with the cloud API.
        :param dataset_id: The id of the Foxglove dataset whose episodes should be
            downloaded.
        :param base_url: The Foxglove API host or URL. Defaults to
            ``https://api.foxglove.dev``. A bare host is accepted.
        """
        # Keep config (strings), NOT a live client, so this datasource and the ReadTasks
        # it produces pickle cleanly to remote workers -- an open socket would not.
        self._api_key = api_key
        self._base_url = base_url
        self._dataset_id = dataset_id
        # Plan on the driver: one cheap index call. We keep config (strings), NOT the
        # client object, so ReadTasks pickle cleanly to remote workers.
        self._episodes = FoxgloveClient(base_url, api_key, dataset_id).list_episodes()

    def estimate_inmemory_data_size(self) -> int | None:
        return None

    def get_read_tasks(
        self,
        parallelism: int,
        per_task_row_limit: int | None = None,
        data_context: object | None = None,
    ) -> list[ReadTask]:
        # Spread episodes across ~parallelism buckets; each bucket becomes one ReadTask
        # that Ray schedules on its own worker. (Ray derives `parallelism` from cluster
        # size + target block size; we cap it at the number of shards we actually have.)
        if not self._episodes:
            return []

        n_buckets = max(1, min(parallelism, len(self._episodes)))
        buckets: list[list[dict[str, str]]] = [[] for _ in range(n_buckets)]
        for i, ep in enumerate(self._episodes):
            buckets[i % n_buckets].append(ep)

        # capture plain config for the closure
        base_url, api_key, dataset_id = self._base_url, self._api_key, self._dataset_id

        read_tasks: list[ReadTask] = []
        for bucket in buckets:
            if not bucket:
                continue
            ids = [ep["id"] for ep in bucket]
            if per_task_row_limit is not None:
                ids = ids[:per_task_row_limit]
            if not ids:
                continue
            # One emitted row per episode, so num_rows here is the episode count.
            metadata = _block_metadata(len(ids))

            def make_read_fn(
                episode_ids: list[str],
            ) -> Callable[[], Iterator[pd.DataFrame]]:
                def read_fn() -> Iterator[pd.DataFrame]:
                    # Fresh client ON the worker, so no live socket is captured in the
                    # pickled closure.
                    client = FoxgloveClient(base_url, api_key, dataset_id)
                    for ep_id in episode_ids:
                        record = client.fetch_episode(ep_id)
                        # One block per episode -> Ray can stream episode k+1's download
                        # while episode k is already feeding downstream operators.
                        yield pd.DataFrame({"episode": [record]})

                return read_fn

            read_tasks.append(ReadTask(make_read_fn(ids), metadata))

        return read_tasks
