#pragma once

#include <functional>
#include <memory>
#include <string_view>
#include <vector>

struct foxglove_fetch_asset_responder;

namespace foxglove {

/// @brief A fetch asset responder.
///
/// This is the means by which a fetch asset implementation responds to a
/// request from a client. Each request is paired with a unique responder
/// instance, and must be used exactly once.
class FetchAssetResponder final {
public:
  /// @brief Sends asset data to the client.
  ///
  /// @param data Asset data pointer.
  /// @param size Asset data length.
  void respondOk(const std::byte* data, size_t size) && noexcept;

  /// @brief Sends asset data to the client.
  ///
  /// @param data Asset data.
  void respondOk(const std::vector<std::byte>& data) && noexcept {
    std::move(*this).respondOk(data.data(), data.size());
  };

  /// @brief Sends an error message to the client.
  ///
  /// @param message Error message.
  void respondError(std::string_view message) && noexcept;

  /// @brief Default destructor.
  ~FetchAssetResponder() = default;
  /// @brief Default move constructor.
  FetchAssetResponder(FetchAssetResponder&&) noexcept = default;
  /// @brief Default move assignment.
  FetchAssetResponder& operator=(FetchAssetResponder&&) noexcept = default;
  FetchAssetResponder(const FetchAssetResponder&) = delete;
  FetchAssetResponder& operator=(const FetchAssetResponder&) = delete;

private:
  friend class WebSocketServer;

  struct Deleter {
    void operator()(foxglove_fetch_asset_responder* ptr) const noexcept;
  };

  std::unique_ptr<foxglove_fetch_asset_responder, Deleter> impl_;

  /// @brief Constructor from a raw pointer.
  explicit FetchAssetResponder(foxglove_fetch_asset_responder* ptr)
      : impl_(ptr) {}
};

/// @brief A fetch asset handler callback.
///
/// This callback is invoked from the client's main poll loop and must not block.
/// If blocking or long-running behavior is required, the implementation should
/// return immediately and handle the request asynchronously.
///
/// The `responder` represents an unfulfilled response. The implementation must
/// eventually call either `respondOk` or `respondError`, exactly once, in order
/// to complete the request. It is safe to invoke these completion methods
/// synchronously from the context of the callback.
///
/// @param uri The requested asset URI.
/// @param responder The responder used to send a response to the client.
using FetchAssetHandler =
  std::function<void(std::string_view uri, FetchAssetResponder&& responder)>;

}  // namespace foxglove
