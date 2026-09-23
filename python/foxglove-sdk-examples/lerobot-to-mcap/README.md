# LeRobot to MCAP

An example from the Foxglove SDK.

Convert a [LeRobot](https://github.com/huggingface/lerobot) dataset into MCAP files, one per
episode, to open in Foxglove. Datasets in the v2.0, v2.1 and v3.0 formats are supported, and
video is copied into the MCAP files without re-encoding.

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

| Option               | Description                                                                                                                                                                                       |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--episodes`         | Episodes to convert, e.g. `0,3,10-12`. Defaults to all of them.                                                                                                                                   |
| `--start-time`       | ISO 8601 time the first episode starts at, in UTC unless it has an offset. It has to be at or after `1970-01-01T00:00:00Z` and before `2100-01-01T00:00:00Z`. Defaults to `2020-01-01T00:00:00Z`. |
| `--episode-gap`      | Seconds between one episode's end and the next one's start, 0 or more. Defaults to 1.                                                                                                             |
| `--strict-keyframes` | Fail if an episode's video doesn't start on a keyframe, rather than starting it earlier.                                                                                                          |

Each episode is written to the output directory as `episode_<index>.mcap`, with the index
padded to six digits, e.g. `episode_000003.mcap`.

## Topics

Topic names and the `lerobot.Scalars` schema match LeRobot's own
[Foxglove integration](https://foxglove.dev/blog/native-foxglove-visualization-in-lerobot)
and the [SO-101 example](../so101-visualization), so layouts made for either work with
converted episodes too.

| LeRobot feature                                     | Topic                        | Schema                     |
| --------------------------------------------------- | ---------------------------- | -------------------------- |
| video feature, e.g. `observation.images.front`      | `/observation/images/front`  | `foxglove.CompressedVideo` |
| image feature, e.g. `observation.image`             | `/observation/images/image`  | `foxglove.CompressedImage` |
| `observation.state`                                 | `/observation/state`         | `lerobot.Scalars`          |
| `action`                                            | `/action/state`              | `lerobot.Scalars`          |
| `next.reward`, `next.done`, `next.success`, ...     | `/episode/state`             | `lerobot.Scalars`          |
| other numeric features, e.g. `observation.velocity` | `/observation/velocity`      | `lerobot.Scalars`          |
| string features                                     | named after the feature      | `lerobot.Text`             |
| the frame's task                                    | `/task`, whenever it changes | `lerobot.Task`             |

A `lerobot.Scalars` message holds a `scalars` list of `{label, value}` pairs. The labels are
the feature's `names` from `meta/info.json` when they give one name per element. Otherwise
they're the feature's name and an index, like `state_0`, or just the name, like `reward`,
for a feature with one element. Plotting `/observation/state.scalars[:]` draws one series
per joint, named after it.

Each file also has:

- a metadata record named `lerobot`, with the dataset's name, codebase version, robot type,
  frame rate and episode count, and the episode's index, length and tasks
- the dataset's `meta/info.json` as an attachment, so that each file carries the full
  feature definitions

## Time

LeRobot doesn't record when data was captured: every episode's timestamps start at zero. The
converter lays the episodes end to end, in index order, on a timeline starting at
`--start-time`. The timeline only depends on the dataset, so converting it again reproduces
the same time ranges, as long as `--start-time` and `--episode-gap` stay the same.

Timestamps within LeRobot's tolerance of the frame grid are snapped to it, so a frame's
video and data share a log time.

## Video

AV1 and VP9 frames are copied as they are, and H.264 and H.265 frames are rewritten from
the mp4 format into the Annex B format that `foxglove.CompressedVideo` expects. Every
keyframe carries the sequence header or parameter sets needed to decode it, so playback can
start from any keyframe.

In v3.0 datasets, many episodes share one mp4. If an episode doesn't start on a keyframe, it
gets the frames back to the previous keyframe, before its start time, so that its first
frame decodes. Episodes recorded with LeRobot start on a keyframe, so this is rare.

## Limitations

- Only the time within an episode is real. Episode start times are made up, see
  [Time](#time).
- Depth map videos (`video.is_depth_map`) are skipped. LeRobot stores them as quantized
  12-bit HEVC, which would need decoding and dequantizing into `foxglove.RawImage` frames.
- `language` features, LeRobot's language annotations, are skipped.
- Image features have to embed their images in the data files, as LeRobot does. Images
  stored only as file paths are rejected.
- Videos with B-frames are rejected, because Foxglove can't play them back. Re-encode them
  without B-frames first, e.g. with ffmpeg's `-bf 0`. Codecs other than AV1, H.264, H.265
  and VP9 are rejected too.
- Dataset and episode statistics (`meta/stats.json`, `meta/episodes_stats.jsonl`, and the
  `stats/*` columns in v3.0) aren't written.
- LeRobot datasets have no camera calibration or transform tree, so there's no
  `foxglove.CameraCalibration` or `/tf` to write. Joint values come without the URDF joint
  names or units that driving a robot model with `foxglove.JointStates` would need.
- The `timestamp`, `frame_index`, `episode_index`, `index` and `task_index` columns aren't
  written as topics, since log times, the metadata record and `/task` carry what they hold.
- v1.x datasets aren't supported. Convert them to v2.0 with LeRobot first.

## Tests

```bash
uv run pytest
```

The tests write small datasets in both the v2.1 and v3.0 formats, with video encoded as
H.264 and AV1.
