# LeRobot to MCAP

An example from the Foxglove SDK.

Convert a [LeRobot](https://github.com/huggingface/lerobot) dataset into MCAP files, one per
episode, to open in Foxglove. This is a command-line interface for the SDK's LeRobot
integration, `foxglove.lerobot`, which you can also use from Python after installing
`foxglove-sdk[lerobot]`. See the
[LeRobot integration docs](https://foxglove-sdk-api-docs.pages.dev/python/lerobot/index.html)
for the topics and schemas the files use, how episodes are placed in time, and what isn't
converted.

## Usage

This example uses [uv](https://docs.astral.sh/uv/).

```bash
uv run main.py --input path/to/dataset --output path/to/output
```

`--input` is the dataset's root directory, the one holding `meta/`, `data/` and `videos/`.
LeRobot keeps the datasets it records or downloads under `~/.cache/huggingface/lerobot/`. To
download a dataset from the Hugging Face Hub:

```bash
uvx --from huggingface_hub hf download lerobot/svla_so101_pickplace --repo-type dataset --local-dir svla_so101_pickplace
```

| Option               | Description                                                                                                                                                                                                 |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--episodes`         | Episodes to convert, e.g. `0,3,10-12`. Defaults to all of them.                                                                                                                                             |
| `--start-time`       | ISO 8601 time the dataset's first episode starts at, in UTC unless it has an offset. It has to be at or after `1970-01-01T00:00:00Z` and before `2100-01-01T00:00:00Z`. Defaults to `2020-01-01T00:00:00Z`. |
| `--episode-gap`      | Seconds between one episode's end and the next one's start, 0 or more. Defaults to 1.                                                                                                                       |
| `--strict-keyframes` | Fail if an episode's video doesn't start on a keyframe, rather than starting it earlier.                                                                                                                    |

Each episode is written to the output directory as `episode_<index>.mcap`, with the index
padded to six digits, e.g. `episode_000003.mcap`.

Videos with B-frames are written too, with a warning, but Foxglove can't play them back. To view
them, re-encode the dataset's videos without B-frames first, e.g. with ffmpeg's `-bf 0`.

## Tests

```bash
uv run pytest
```
