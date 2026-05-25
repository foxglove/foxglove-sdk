#include "callback_forwarders.hpp"

#include <foxglove-c/foxglove-c.h>
#include <foxglove/channel.hpp>
#include <foxglove/error.hpp>
#include <foxglove/fetch_asset.hpp>
#include <foxglove/parameter.hpp>
#include <foxglove/parameter_handler.hpp>

#include <optional>
#include <string_view>
#include <utility>
#include <vector>

namespace foxglove::internal {

void forwardFetchAsset(
  const void* context, const foxglove_string* c_uri, foxglove_fetch_asset_responder* c_responder
) {
  const auto* handler = static_cast<const FetchAssetHandler*>(context);
  std::string_view uri{c_uri->data, c_uri->len};
  auto responder = ForwarderAccess::makeFetchAssetResponder(c_responder);
  try {
    (*handler)(uri, std::move(responder));
  } catch (const std::exception& exc) {
    warn() << "Fetch asset callback failed: " << exc.what();
    constexpr std::string_view kErrMessage{"Fetch asset callback failed"};
    if (auto* ptr = ForwarderAccess::releaseFetchAsset(responder)) {
      foxglove_fetch_asset_respond_error(ptr, {kErrMessage.data(), kErrMessage.size()});
    }
  }
}

bool forwardSinkChannelFilter(const void* context, const foxglove_channel_descriptor* channel) {
  if (context == nullptr) {
    return true;
  }
  try {
    const auto* filter = static_cast<const SinkChannelFilterFn*>(context);
    auto cpp_channel = ChannelDescriptor(channel);
    return (*filter)(cpp_channel);
  } catch (const std::exception& exc) {
    warn() << "Sink channel filter failed: " << exc.what();
    return false;
  }
}

void forwardParameterHandlerGet(
  const void* context, uint32_t client_id, const foxglove_string* c_request_id,
  const foxglove_string* c_param_names, size_t param_names_len,
  foxglove_get_parameters_responder* c_responder
) {
  // Take ownership of the responder before any throwable work, so its destructor
  // sends the generic error to the client if anything below throws.
  auto responder = ForwarderAccess::makeGetResponder(c_responder);
  callbackGuard("ParameterHandler.onGet", [&] {
    std::optional<std::string_view> request_id;
    if (c_request_id != nullptr) {
      request_id.emplace(c_request_id->data, c_request_id->len);
    }
    std::vector<std::string_view> param_names;
    if (c_param_names != nullptr) {
      param_names.reserve(param_names_len);
      for (size_t i = 0; i < param_names_len; ++i) {
        param_names.emplace_back(c_param_names[i].data, c_param_names[i].len);
      }
    }
    (static_cast<const ParameterHandler*>(context))
      ->onGet(client_id, request_id, param_names, std::move(responder));
  });
}

void forwardParameterHandlerSet(
  const void* context, uint32_t client_id, const foxglove_string* c_request_id,
  const foxglove_parameter_array* c_params, foxglove_set_parameters_responder* c_responder
) {
  // Take ownership of the responder before any throwable work, so its destructor
  // sends the generic error to the client if anything below throws.
  auto responder = ForwarderAccess::makeSetResponder(c_responder);
  callbackGuard("ParameterHandler.onSet", [&] {
    std::optional<std::string_view> request_id;
    if (c_request_id != nullptr) {
      request_id.emplace(c_request_id->data, c_request_id->len);
    }
    // The C ABI guarantees that c_params is never null.
    (static_cast<const ParameterHandler*>(context))
      ->onSet(
        client_id, request_id, ParameterArrayView(c_params).parameters(), std::move(responder)
      );
  });
}

}  // namespace foxglove::internal
