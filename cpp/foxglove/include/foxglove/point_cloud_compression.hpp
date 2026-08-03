#pragma once

#include <cstdint>

namespace foxglove {

/// @brief Transparent point-cloud compression mode for a channel.
enum class PointCloudCompressionMode : uint8_t {
  /// Use the SDK default: Draco compression with default settings (kd-tree encoding with
  /// positions quantized to 12 bits, which is lossy). This is the default (0).
  Default = 0,
  /// Disable transparent point-cloud compression: point clouds are delivered unmodified.
  Disabled = 1,
  /// Draco compression with the settings in @ref PointCloudCompression::draco.
  Draco = 2,
};

/// @brief Options for Draco point-cloud encoding.
struct DracoEncodeOptions {
  /// @brief The maximum supported value for @ref quantization_bits.
  static constexpr uint8_t kMaxQuantizationBits = 30;

  /// @brief Quantization bits for the position attribute; must be between 1 and
  /// @ref kMaxQuantizationBits (30) inclusive. Out-of-range values are repaired, with a
  /// logged warning naming the channel: values above the maximum (which the reference
  /// Draco decoder rejects) are clamped to it, and `0` (lossless) provides no size
  /// reduction over the raw point cloud, so the channel is delivered unmodified — use
  /// @ref PointCloudCompressionMode::Disabled to do that without the warning.
  uint8_t quantization_bits = 12;
};

/// @brief Transparent point-cloud compression for a single channel, returned by the
/// per-channel policy callback on the gateway options.
///
/// When compression is enabled, channels carrying a supported point-cloud schema —
/// currently protobuf-encoded `foxglove.PointCloud` or CDR-encoded
/// `sensor_msgs/msg/PointCloud2` — are advertised with the protobuf-encoded
/// `foxglove.CompressedPointCloud` schema, and each logged point cloud
/// is compressed in a background task (off the logging hot path) before delivery. If
/// compression falls behind the log rate, the oldest queued message is dropped.
/// Channels classified as Reliable skip compression and deliver the raw point cloud on
/// the control bytestream.
///
/// Compressed clouds are conditioned before encoding: per-point fields that carry no
/// value on a remote viewer (timestamps, ranges and angles derivable from the positions,
/// and per-point indices, matched by exact (name, type) tuple) are dropped; packed
/// rgb/rgba color fields declared float32 are reinterpreted as uint32; float64 fields
/// are narrowed to float32 (about seven significant digits); and points containing a
/// non-finite value in any float field are removed. To deliver clouds untouched, return
/// @ref disabled() for the channel.
struct PointCloudCompression {
  /// @brief The compression mode.
  PointCloudCompressionMode mode = PointCloudCompressionMode::Default;
  /// @brief Draco encoding settings.
  ///
  /// These are **silently ignored unless @ref mode is @ref PointCloudCompressionMode::Draco** —
  /// setting `draco` without also setting `mode` leaves the SDK default in effect. Prefer the
  /// @ref withDraco() factory, which sets both together.
  DracoEncodeOptions draco;

  /// @brief Disable transparent point-cloud compression: deliver point clouds unmodified.
  static PointCloudCompression disabled() {
    return {PointCloudCompressionMode::Disabled, {}};
  }

  /// @brief Compress with Draco using the given settings.
  ///
  /// Sets @ref mode and @ref draco together so they cannot get out of sync. For example:
  /// `return PointCloudCompression::withDraco({8});`
  static PointCloudCompression withDraco(DracoEncodeOptions options) {
    return {PointCloudCompressionMode::Draco, options};
  }
};

}  // namespace foxglove
