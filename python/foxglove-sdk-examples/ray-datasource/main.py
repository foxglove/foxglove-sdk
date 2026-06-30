"""Load Foxglove recordings as a Ray Data dataset and summarize them.

The :class:`~foxglove.ray.FoxgloveDataset` datasource downloads every recording in a Foxglove
dataset in parallel across the Ray cluster, emitting one row of raw MCAP bytes per recording.
Decoding the bytes is left to a downstream Ray Data transform -- here we count the messages in each
recording with a ``.flat_map(...)``.
"""

import argparse
import io
import os

import ray
from foxglove.ray import FoxgloveDataset
from mcap.reader import make_reader

parser = argparse.ArgumentParser()
parser.add_argument(
    "--dataset-id",
    required=True,
    help="The id of the Foxglove dataset to load.",
)
parser.add_argument(
    "--token",
    default=os.environ.get("FOXGLOVE_API_TOKEN"),
    help="A Foxglove API token. Defaults to the FOXGLOVE_API_TOKEN environment variable.",
)
parser.add_argument(
    "--host",
    default="api.foxglove.dev",
    help="The Foxglove API host.",
)
args = parser.parse_args()


def count_messages(row: dict) -> list[dict]:
    """Decode one recording's MCAP bytes and emit a single summary row."""
    recording = row["recording"]
    reader = make_reader(io.BytesIO(recording["mcap"]))
    num_messages = sum(1 for _ in reader.iter_messages())
    return [{"id": recording["id"], "num_messages": num_messages}]


def main() -> None:
    if not args.token:
        raise SystemExit(
            "A Foxglove API token is required. Pass --token or set FOXGLOVE_API_TOKEN."
        )

    # The datasource downloads recordings on Ray workers; the driver only plans the read.
    ds = ray.data.read_datasource(
        FoxgloveDataset(token=args.token, dataset_id=args.dataset_id, host=args.host),
    )

    # Decode downstream: one summary row per recording.
    summaries = ds.flat_map(count_messages)
    for row in summaries.take_all():
        print(f"{row['id']}: {row['num_messages']} messages")


if __name__ == "__main__":
    main()
