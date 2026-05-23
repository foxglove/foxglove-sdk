#pragma once

/// @cond foxglove_internal

#include <foxglove-c/foxglove-c.h>
#include <foxglove/channel.hpp>
#include <foxglove/error.hpp>
#include <foxglove/fetch_asset.hpp>
#include <foxglove/parameter.hpp>
#include <foxglove/parameter_handler.hpp>

#include <cstddef>
#include <cstdint>
#include <memory>
#include <optional>
#include <string_view>
#include <utility>
#include <vector>

namespace foxglove::internal {

/// Grants the forwarder functions access to the responder and ParameterArray
/// types' private raw-pointer accessors. Declared as a friend by each of those
/// classes via its public header.
struct ForwarderAccess {
  static FetchAssetResponder makeFetchAssetResponder(foxglove_fetch_asset_responder* p) {
    return FetchAssetResponder(p);
  }
  static foxglove_fetch_asset_responder* releaseFetchAsset(FetchAssetResponder& r) {
    return r.impl_.release();
  }
  static GetParametersResponder makeGetResponder(foxglove_get_parameters_responder* p) {
    return GetParametersResponder(p);
  }
  static SetParametersResponder makeSetResponder(foxglove_set_parameters_responder* p) {
    return SetParametersResponder(p);
  }
  static foxglove_parameter_array* releaseParameterArray(ParameterArray& a) {
    return a.release();
  }
};

/// try/catch wrapper for callback bodies. Catches std::exception so an
/// exception thrown by user code never propagates back into Rust.
template<class F>
inline void callbackGuard(const char* name, F&& f) noexcept {
  try {
    std::forward<F>(f)();
  } catch (const std::exception& exc) {
    warn() << name << " callback failed: " << exc.what();
  }
}

// Forwarders are pointed to by C ABI function pointer fields. The `context`
// argument is the C++ handler object owned by the create() caller.

void forwardFetchAsset(
  const void* context, const foxglove_string* c_uri, foxglove_fetch_asset_responder* c_responder
);

bool forwardSinkChannelFilter(const void* context, const foxglove_channel_descriptor* channel);

void forwardParameterHandlerGet(
  const void* context, uint32_t client_id, const foxglove_string* c_request_id,
  const foxglove_string* c_param_names, size_t param_names_len,
  foxglove_get_parameters_responder* c_responder
);

void forwardParameterHandlerSet(
  const void* context, uint32_t client_id, const foxglove_string* c_request_id,
  const foxglove_parameter_array* c_params, foxglove_set_parameters_responder* c_responder
);

// Forwarders shared between the WebSocket server and the remote access gateway:
// they have callback fields with identical signatures, and the C ABI uses the
// same function-pointer types. Templated on the C++ callbacks struct
// (WebSocketServerCallbacks or RemoteAccessGatewayCallbacks).

template<class Callbacks>
void forwardParametersSubscribe(
  const void* context, const foxglove_string* c_names, size_t names_len
) {
  callbackGuard("onParametersSubscribe", [&] {
    std::vector<std::string_view> names;
    names.reserve(names_len);
    for (size_t i = 0; i < names_len; ++i) {
      names.emplace_back(c_names[i].data, c_names[i].len);
    }
    (static_cast<const Callbacks*>(context))->onParametersSubscribe(names);
  });
}

template<class Callbacks>
void forwardParametersUnsubscribe(
  const void* context, const foxglove_string* c_names, size_t names_len
) {
  callbackGuard("onParametersUnsubscribe", [&] {
    std::vector<std::string_view> names;
    names.reserve(names_len);
    for (size_t i = 0; i < names_len; ++i) {
      names.emplace_back(c_names[i].data, c_names[i].len);
    }
    (static_cast<const Callbacks*>(context))->onParametersUnsubscribe(names);
  });
}

template<class Callbacks>
void forwardConnectionGraphSubscribe(const void* context) {
  callbackGuard("onConnectionGraphSubscribe", [&] {
    (static_cast<const Callbacks*>(context))->onConnectionGraphSubscribe();
  });
}

template<class Callbacks>
void forwardConnectionGraphUnsubscribe(const void* context) {
  callbackGuard("onConnectionGraphUnsubscribe", [&] {
    (static_cast<const Callbacks*>(context))->onConnectionGraphUnsubscribe();
  });
}

// The legacy forwarders reference [[deprecated]] fields on the C++ callbacks
// structs (onGetParameters/onSetParameters). This file is the internal C-ABI
// forwarder and must keep reading them, so silence the warning at the
// template definition.
#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wdeprecated-declarations"
#elif defined(_MSC_VER)
#pragma warning(push)
#pragma warning(disable : 4996)
#endif

// Build the result entirely inside callbackGuard so that allocation failure
// (e.g. building param_names, or the ParameterArray itself) cannot propagate
// an exception back across the C ABI. On any throw, returns nullptr.
template<class Callbacks>
foxglove_parameter_array* forwardLegacyGetParameters(
  const void* context, uint32_t client_id, const foxglove_string* c_request_id,
  const foxglove_string* c_param_names, size_t param_names_len
) {
  foxglove_parameter_array* result = nullptr;
  callbackGuard("onGetParameters", [&] {
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
    auto params =
      (static_cast<const Callbacks*>(context))->onGetParameters(client_id, request_id, param_names);
    auto array = ParameterArray(std::move(params));
    result = ForwarderAccess::releaseParameterArray(array);
  });
  return result;
}

template<class Callbacks>
foxglove_parameter_array* forwardLegacySetParameters(
  const void* context, uint32_t client_id, const foxglove_string* c_request_id,
  const foxglove_parameter_array* c_params
) {
  if (c_params == nullptr) {
    return nullptr;
  }
  foxglove_parameter_array* result = nullptr;
  callbackGuard("onSetParameters", [&] {
    std::optional<std::string_view> request_id;
    if (c_request_id != nullptr) {
      request_id.emplace(c_request_id->data, c_request_id->len);
    }
    auto params =
      (static_cast<const Callbacks*>(context))
        ->onSetParameters(client_id, request_id, ParameterArrayView(c_params).parameters());
    auto array = ParameterArray(std::move(params));
    result = ForwarderAccess::releaseParameterArray(array);
  });
  return result;
}

#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic pop
#elif defined(_MSC_VER)
#pragma warning(pop)
#endif

// Wiring helpers. Templated on the C options struct because foxglove_server_options
// and foxglove_gateway_options share field names but are distinct types.

template<class COptions>
void wireFetchAsset(
  COptions& c_options, FetchAssetHandler&& cpp_handler, std::unique_ptr<FetchAssetHandler>& out
) {
  if (!cpp_handler) {
    return;
  }
  out = std::make_unique<FetchAssetHandler>(std::move(cpp_handler));
  c_options.fetch_asset_context = out.get();
  c_options.fetch_asset = &forwardFetchAsset;
}

template<class COptions>
void wireSinkChannelFilter(
  COptions& c_options, SinkChannelFilterFn&& cpp_handler, std::unique_ptr<SinkChannelFilterFn>& out
) {
  if (!cpp_handler) {
    return;
  }
  out = std::make_unique<SinkChannelFilterFn>(std::move(cpp_handler));
  c_options.sink_channel_filter_context = out.get();
  c_options.sink_channel_filter = &forwardSinkChannelFilter;
}

/// Validate and wire a ParameterHandler. Returns ValueError if exactly one of
/// onGet/onSet is set. If both are unset, leaves c_options.parameter_handler
/// untouched and returns Ok.
template<class COptions>
FoxgloveError wireParameterHandler(
  COptions& c_options, foxglove_parameter_handler& c_handler, ParameterHandler&& cpp_handler,
  std::unique_ptr<ParameterHandler>& out
) {
  const bool has_on_get = bool(cpp_handler.onGet);
  const bool has_on_set = bool(cpp_handler.onSet);
  if (has_on_get != has_on_set) {
    warn() << "ParameterHandler requires both onGet and onSet to be set";
    return FoxgloveError::ValueError;
  }
  if (!has_on_get) {
    return FoxgloveError::Ok;
  }
  out = std::make_unique<ParameterHandler>(std::move(cpp_handler));
  c_handler.context = out.get();
  c_handler.get = &forwardParameterHandlerGet;
  c_handler.set = &forwardParameterHandlerSet;
  c_options.parameter_handler = &c_handler;
  return FoxgloveError::Ok;
}

}  // namespace foxglove::internal

/// @endcond
