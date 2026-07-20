#pragma once

#include <cstdint>

namespace foxglove {

/// @brief Transparent point-cloud compression mode for a sink.
enum class PointCloudCompressionMode : uint8_t {
  /// Use the SDK default: Draco compression with default settings (kd-tree encoding with
  /// positions quantized to 12 bits, which is lossy). This is the default (0).
  Default = 0,
  /// Disable transparent point-cloud compression: point clouds are delivered unmodified.
  Disabled = 1,
  /// Draco compression with the settings in @ref PointCloudCompression::draco.
  Draco = 2,
};

/// @brief Draco encoding method for point-cloud compression.
enum class DracoMethod : uint8_t {
  /// Sequential encoding: preserves point order and copies all extra fields losslessly.
  Sequential = 0,
  /// kd-tree encoding: better compression ratios, but reorders points, and float32 extra
  /// fields are quantized with the same number of bits as positions.
  ///
  /// Encoding falls back to sequential when `quantization_bits` is 0 (lossless) or the
  /// cloud contains a float64 field.
  KdTree = 1,
};

/// @brief Options for Draco point-cloud encoding.
struct DracoEncodeOptions {
  /// @brief The Draco encoding method.
  DracoMethod method = DracoMethod::KdTree;
  /// @brief Quantization bits for the position attribute. `0` encodes positions as lossless
  /// float32 (much larger output, and falls back to sequential encoding).
  uint8_t quantization_bits = 12;
};

/// @brief Transparent point-cloud compression configuration for a sink.
///
/// When compression is enabled, channels carrying `foxglove.PointCloud` messages are
/// advertised with the `foxglove.CompressedPointCloud` schema, and each logged point cloud
/// is compressed in a background task (off the logging hot path) before delivery. If
/// compression falls behind the log rate, the oldest queued message is dropped.
/// Channels classified as Reliable skip compression and deliver the raw point cloud on
/// the control bytestream.
struct PointCloudCompression {
  /// @brief The compression mode.
  PointCloudCompressionMode mode = PointCloudCompressionMode::Default;
  /// @brief Draco encoding settings. Only used when `mode` is
  /// @ref PointCloudCompressionMode::Draco.
  DracoEncodeOptions draco;
};

}  // namespace foxglove
