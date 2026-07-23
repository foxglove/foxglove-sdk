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
///
/// kd-tree is currently the only offered method: sequential encoding is withheld because
/// the encoder emits sequential bitstreams that the reference Draco decoder rejects
/// whenever positions are quantized (upstream draco-core conformance bug). A `Sequential`
/// value will be added once the upstream encoder is fixed.
enum class DracoMethod : uint8_t {
  /// kd-tree encoding: reorders points, and float32 extra fields are quantized with the
  /// same number of bits as positions. This is the default (0).
  KdTree = 0,
};

/// @brief Options for Draco point-cloud encoding.
struct DracoEncodeOptions {
  /// @brief The Draco encoding method. Currently kd-tree is the only choice; see
  /// @ref DracoMethod.
  DracoMethod method = DracoMethod::KdTree;
  /// @brief Quantization bits for the position attribute; must be between 1 and 31
  /// inclusive. Values outside that range cause @ref RemoteAccessGateway::create to fail
  /// with @ref FoxgloveError::ConfigurationError: values above 31 exceed what Draco
  /// supports, and `0` (lossless) provides no size reduction over the raw point cloud —
  /// use @ref PointCloudCompressionMode::Disabled instead.
  uint8_t quantization_bits = 12;
};

/// @brief Transparent point-cloud compression configuration for a sink.
///
/// When compression is enabled, channels carrying `foxglove.PointCloud` messages are
/// advertised with the `foxglove.CompressedPointCloud` schema, and each logged point cloud
/// is compressed in a background task (off the logging hot path) before delivery. If
/// compression falls behind the log rate, the oldest queued message is dropped.
/// Channels classified as Reliable skip compression and deliver the raw point cloud on
/// the control bytestream. Clouds containing float64 fields cannot be quantized and are
/// delivered losslessly (no size reduction); a throttled warning is emitted when this
/// happens.
struct PointCloudCompression {
  /// @brief The compression mode.
  PointCloudCompressionMode mode = PointCloudCompressionMode::Default;
  /// @brief Draco encoding settings. Only used when `mode` is
  /// @ref PointCloudCompressionMode::Draco.
  DracoEncodeOptions draco;
};

}  // namespace foxglove
