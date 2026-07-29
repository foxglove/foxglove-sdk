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
  /// `options.point_cloud_compression = PointCloudCompression::withDraco({8});`
  static PointCloudCompression withDraco(DracoEncodeOptions options) {
    return {PointCloudCompressionMode::Draco, options};
  }
};

}  // namespace foxglove
