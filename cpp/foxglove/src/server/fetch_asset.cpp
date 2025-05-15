#include <foxglove-c/foxglove-c.h>
#include <foxglove/server/fetch_asset.hpp>

using namespace std::string_view_literals;

namespace foxglove {

/**
 * FetchAssetResponder implementation.
 */
void FetchAssetResponder::Deleter::operator()(foxglove_fetch_asset_responder* ptr) const noexcept {
  auto message = "Internal server error: Server failed to send a response"sv;
  foxglove_fetch_asset_respond_error(ptr, {message.data(), message.length()});
}

void FetchAssetResponder::respondOk(const std::byte* data, size_t size) && noexcept {
  auto* ptr = impl_.release();
  foxglove_fetch_asset_respond_ok(ptr, {reinterpret_cast<const uint8_t*>(data), size});
}

void FetchAssetResponder::respondError(std::string_view message) && noexcept {
  auto* ptr = impl_.release();
  foxglove_fetch_asset_respond_error(ptr, {message.data(), message.length()});
}

}  // namespace foxglove
