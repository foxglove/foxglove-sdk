LeRobot integration
===================

Convert `LeRobot <https://github.com/huggingface/lerobot>`__ datasets into MCAP files, one per
episode, to open in Foxglove. Datasets in the v2.0, v2.1 and v3.0 formats are supported, and
video is copied into the MCAP files without re-encoding.

.. note::
   The LeRobot integration is only available when the ``lerobot`` extra package is installed.
   Install it with ``pip install foxglove-sdk[lerobot]``.

.. code-block:: python

   from pathlib import Path

   from foxglove.lerobot import EpisodeWriter, load_metadata

   metadata = load_metadata("svla_so101_pickplace")
   writer = EpisodeWriter(metadata)
   output_dir = Path("mcap")
   output_dir.mkdir(exist_ok=True)
   for episode in metadata.episodes:
       writer.write(episode, output_dir)

For a command-line converter, see the `lerobot-to-mcap example
<https://github.com/foxglove/foxglove-sdk/tree/main/python/foxglove-sdk-examples/lerobot-to-mcap>`__.

Topics
------

Topic names and the ``lerobot.Scalars`` schema match LeRobot's own `Foxglove integration
<https://foxglove.dev/blog/native-foxglove-visualization-in-lerobot>`__, so layouts made for it
work with converted episodes too.

.. list-table::
   :header-rows: 1

   * - LeRobot feature
     - Topic
     - Schema
   * - video feature, e.g. ``observation.images.front``
     - ``/observation/images/front``
     - ``foxglove.CompressedVideo``
   * - image feature, e.g. ``observation.image``
     - ``/observation/images/image``
     - ``foxglove.CompressedImage``
   * - ``observation.state``
     - ``/observation/state``
     - ``lerobot.Scalars``
   * - ``action``
     - ``/action/state``
     - ``lerobot.Scalars``
   * - ``next.reward``, ``next.done``, ``next.success``, ...
     - ``/episode/state``
     - ``lerobot.Scalars``
   * - other numeric features, e.g. ``observation.velocity``
     - ``/observation/velocity``
     - ``lerobot.Scalars``
   * - string features
     - named after the feature
     - ``lerobot.Text``
   * - the frame's task
     - ``/task``, whenever it changes
     - ``lerobot.Task``

A ``lerobot.Scalars`` message holds a ``scalars`` list of ``{label, value}`` pairs. The labels
are the feature's ``names`` from ``meta/info.json`` when they give one name per element.
Otherwise they're the feature's name and an index, like ``state_0``, or just the name, like
``reward``, for a feature with one element. Plotting ``/observation/state.scalars[:]`` draws
one series per joint, named after it.

Each file also has:

- a metadata record named ``lerobot``, with the dataset's name, codebase version, robot type,
  frame rate and episode count, and the episode's index, length and tasks
- the dataset's ``meta/info.json`` as an attachment, so that each file carries the full feature
  definitions

Time
----

LeRobot doesn't record when data was captured: every episode's timestamps start at zero.
:class:`~foxglove.lerobot.EpisodeWriter` lays the episodes end to end, in index order, on a
timeline starting at ``start_time``. The timeline only depends on the dataset, so converting it
again reproduces the same time ranges, as long as ``start_time`` and ``episode_gap_s`` stay the
same.

Timestamps within LeRobot's tolerance of the frame grid are snapped to it, so a frame's video
and data share a log time.

Video
-----

H.264 and H.265 frames are rewritten from the mp4 format into the Annex B format that
``foxglove.CompressedVideo`` expects. H.264 keyframes without parameter sets and AV1 keyframes
without a sequence header get the ones from the mp4. Other frames are copied as they are.
Every keyframe then carries the sequence header or parameter sets needed to decode it, so
playback can start from any keyframe.

In v3.0 datasets, many episodes share one mp4. If an episode doesn't start on a keyframe, it
gets the frames back to the previous keyframe, before its start time, so that its first frame
decodes. Episodes recorded with LeRobot start on a keyframe, so this is rare.

Videos with B-frames store their frames out of display order. They're written as they are,
in decode order, with each frame's ``timestamp`` set to the time it's shown, so reading the
file in log time order decodes them. An episode also gets any frames from just outside it that
its frames depend on. These videos are listed in the ``b_frame_videos`` of the
:class:`~foxglove.lerobot.WrittenEpisode` that :meth:`~foxglove.lerobot.EpisodeWriter.write`
returns. Foxglove can't play them back. To view them, re-encode them without B-frames first,
e.g. with ffmpeg's ``-bf 0``.

Limitations
-----------

- Only the time within an episode is real. Episode start times are made up, see `Time`_.
- Depth map videos (``video.is_depth_map``) are skipped. LeRobot stores them as quantized 12-bit
  HEVC, which would need decoding and dequantizing into ``foxglove.RawImage`` frames.
- ``language`` features, LeRobot's language annotations, are skipped.
- Numeric features with a variable-length dimension, ``null`` in their shape, are skipped.
- A feature named ``task`` is skipped, since its topic, ``/task``, holds the frame's task.
- Image features have to embed their images in the data files, as LeRobot does. Images stored
  only as file paths are rejected.
- Videos with B-frames are written, but Foxglove can't play them back, see `Video`_. Codecs
  other than AV1, H.264, H.265 and VP9 are rejected.
- Dataset and episode statistics (``meta/stats.json``, ``meta/episodes_stats.jsonl``, and the
  ``stats/*`` columns in v3.0) aren't written.
- LeRobot datasets have no camera calibration or transform tree, so there's no
  ``foxglove.CameraCalibration`` or ``/tf`` to write. Joint values come without the URDF joint
  names or units that driving a robot model with ``foxglove.JointStates`` would need.
- The ``timestamp``, ``frame_index``, ``episode_index`` and ``task_index`` columns aren't written
  as topics, since log times, the metadata record and ``/task`` carry what they hold. The
  ``index`` column, a frame's position in the whole dataset, isn't written either.
- v1.x datasets aren't supported. Convert them to v2.0 with LeRobot first.

API
---

.. autofunction:: foxglove.lerobot.load_metadata

.. autoclass:: foxglove.lerobot.EpisodeWriter
   :members: write

.. autoclass:: foxglove.lerobot.WrittenEpisode

.. autoclass:: foxglove.lerobot.DatasetMetadata
   :members: version, fps, robot_type

.. autoclass:: foxglove.lerobot.Episode

.. autoclass:: foxglove.lerobot.VideoSegment

.. autoclass:: foxglove.lerobot.Feature

.. autoexception:: foxglove.lerobot.UnsupportedDatasetError

.. autoexception:: foxglove.lerobot.UnsupportedVideoError

.. autoexception:: foxglove.lerobot.KeyframeError
