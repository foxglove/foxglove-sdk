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

/// @brief Options for Draco point-cloud encoding.
struct DracoEncodeOptions {
  /// @brief Quantization bits for the position attribute; must be between 1 and 30
  /// inclusive. Values outside that range cause @ref RemoteAccessGateway::create to fail
  /// with @ref FoxgloveError::ConfigurationError. Values above 30 are rejected by the
  /// reference Draco decoder, and `0` (lossless) provides no size reduction over the raw
  /// point cloud — use @ref PointCloudCompressionMode::Disabled instead.
  uint8_t quantization_bits = 12;
};

/// @brief Transparent point-cloud compression configuration for a sink.
///
/// When compression is enabled, channels carrying `foxglove.PointCloud` messages are
/// advertised with the `foxglove.CompressedPointCloud` schema, and each logged point cloud
/// is compressed in a background task (off the logging hot path) before delivery. If
/// compression falls behind the log rate, the oldest queued message is dropped.
/// Channels classified as Reliable skip compression and deliver the raw point cloud on
/// the control bytestream. Draco cannot quantize float64 fields, so non-empty clouds
/// containing one (other than the x/y/z position fields) fail to compress and are
/// dropped, with a throttled warning.
struct PointCloudCompression {
  /// @brief The compression mode.
  PointCloudCompressionMode mode = PointCloudCompressionMode::Default;
  /// @brief Draco encoding settings. Only used when `mode` is
  /// @ref PointCloudCompressionMode::Draco.
  DracoEncodeOptions draco;
};

}  // namespace foxglove
