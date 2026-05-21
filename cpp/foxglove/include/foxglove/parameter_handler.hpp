#pragma once

#include <foxglove/parameter.hpp>

#include <cstdint>
#include <functional>
#include <memory>
#include <optional>
#include <string_view>
#include <vector>

struct foxglove_get_parameters_responder;
struct foxglove_set_parameters_responder;

namespace foxglove {

/// @cond foxglove_internal
namespace internal {
struct ForwarderAccess;
}
/// @endcond

/// @brief Responder for a client `getParameters` request.
///
/// This is the means by which a parameter handler responds to a get request
/// from a client. Each request is paired with a unique responder instance,
/// and the handler **must** complete it by calling `respond()` exactly once
/// (pass an empty vector if no values are available). Dropping the responder
/// without responding is reserved for unrecoverable internal errors, and
/// sends a generic error status to the requesting client.
class GetParametersResponder final {
public:
  /// @brief Send parameter values back to the requesting client.
  ///
  /// Entries with an unset value are dropped before serialization.
  ///
  /// May throw `std::runtime_error` if the underlying parameter-array
  /// allocation fails. If this happens, the responder is left intact;
  /// dropping it (without another `respond()`) sends the generic error
  /// status to the client.
  ///
  /// @param params Parameter values to send.
  void respond(std::vector<Parameter>&& params) &&;

  ~GetParametersResponder() = default;
  /// @brief Default move constructor.
  GetParametersResponder(GetParametersResponder&&) noexcept = default;
  /// @brief Default move assignment.
  GetParametersResponder& operator=(GetParametersResponder&&) noexcept = default;
  GetParametersResponder(const GetParametersResponder&) = delete;
  GetParametersResponder& operator=(const GetParametersResponder&) = delete;

private:
  friend class WebSocketServer;
  friend class RemoteAccessGateway;
  friend struct internal::ForwarderAccess;

  struct Deleter {
    void operator()(foxglove_get_parameters_responder* ptr) const noexcept;
  };

  std::unique_ptr<foxglove_get_parameters_responder, Deleter> impl_;

  explicit GetParametersResponder(foxglove_get_parameters_responder* ptr)
      : impl_(ptr) {}
};

/// @brief Responder for a client `setParameters` request.
///
/// This is the means by which a parameter handler responds to a set request
/// from a client. Each request is paired with a unique responder instance,
/// and the handler **must** complete it by calling `respond()` exactly once
/// with the values that were actually applied (pass an empty vector if the
/// request could not be handled). When the request carried a request_id, the
/// values passed to `respond()` are echoed back to the requesting client;
/// otherwise the responder does nothing on the wire. The responder does not
/// notify other parameter subscribers; a handler may be shared across
/// multiple sinks, so it is the implementer's responsibility to broadcast
/// applied updates to subscribers on each sink (for example, via
/// `WebSocketServer::publishParameterValues` and
/// `RemoteAccessGateway::publishParameterValues`). Dropping the responder
/// without responding is reserved for unrecoverable internal errors, and
/// sends a generic error status to the requesting client.
class SetParametersResponder final {
public:
  /// @brief Acknowledge the set request with the values that were actually
  /// applied.
  ///
  /// Echoes those values back to the requesting client when the request
  /// carried a request_id; otherwise does nothing on the wire. Entries with an
  /// unset value are dropped before serialization.
  ///
  /// May throw `std::runtime_error` if the underlying parameter-array
  /// allocation fails. If this happens, the responder is left intact;
  /// dropping it (without another `respond()`) sends the generic error
  /// status to the client.
  ///
  /// @param params Parameter values that were applied.
  void respond(std::vector<Parameter>&& params) &&;

  ~SetParametersResponder() = default;
  /// @brief Default move constructor.
  SetParametersResponder(SetParametersResponder&&) noexcept = default;
  /// @brief Default move assignment.
  SetParametersResponder& operator=(SetParametersResponder&&) noexcept = default;
  SetParametersResponder(const SetParametersResponder&) = delete;
  SetParametersResponder& operator=(const SetParametersResponder&) = delete;

private:
  friend class WebSocketServer;
  friend class RemoteAccessGateway;
  friend struct internal::ForwarderAccess;

  struct Deleter {
    void operator()(foxglove_set_parameters_responder* ptr) const noexcept;
  };

  std::unique_ptr<foxglove_set_parameters_responder, Deleter> impl_;

  explicit SetParametersResponder(foxglove_set_parameters_responder* ptr)
      : impl_(ptr) {}
};

/// @brief Handler for client-initiated parameter operations.
///
/// When supplied to a `WebSocketServerOptions` or `RemoteAccessGatewayOptions`,
/// this handler takes precedence over the deprecated `onGetParameters` /
/// `onSetParameters` callbacks. Registering a handler also automatically
/// advertises the `Parameters` capability. Subscribe/unsubscribe notifications
/// still go through the `onParametersSubscribe` / `onParametersUnsubscribe`
/// callbacks on `WebSocketServerCallbacks` /
/// `RemoteAccessGatewayCallbacks`; wire those up separately if you want to be
/// notified.
///
/// Both `onGet` and `onSet` are required: if a `ParameterHandler` is provided
/// with only one of these set, `WebSocketServer::create` /
/// `RemoteAccessGateway::create` returns `FoxgloveError::ValueError`. To omit
/// a handler entirely, leave both callbacks unset.
///
/// @note These callbacks are invoked from time-sensitive contexts and must not
/// block. If long-running work is required, the implementation should hand the
/// responder off to another thread and return immediately.
struct ParameterHandler {
  /// @brief Callback invoked when a client requests parameters.
  ///
  /// Required when a `ParameterHandler` is registered.
  ///
  /// The implementation takes ownership of `responder`; see
  /// `GetParametersResponder` for the completion contract.
  ///
  /// @param client_id The requesting client's ID.
  /// @param request_id A request ID unique to this client. May be std::nullopt.
  /// When present, the buffer is valid for the duration of this call.
  /// @param param_names A list of parameter names to fetch, or empty to
  /// request all parameters. The buffer is valid for the duration of this
  /// call; if the callback wishes to store these values (e.g. to hand them
  /// off to another thread along with the responder), it must copy them out.
  /// @param responder The responder used to complete the request.
  std::function<void(
    uint32_t client_id, std::optional<std::string_view> request_id,
    const std::vector<std::string_view>& param_names, GetParametersResponder&& responder
  )>
    onGet;

  /// @brief Callback invoked when a client sets parameters.
  ///
  /// Required when a `ParameterHandler` is registered.
  ///
  /// The implementation takes ownership of `responder`; see
  /// `SetParametersResponder` for the completion contract, the request_id
  /// echo behavior, and the implementer's responsibility to broadcast applied
  /// updates to other parameter subscribers.
  ///
  /// @param client_id The requesting client's ID.
  /// @param request_id A request ID unique to this client. May be std::nullopt.
  /// When present, the buffer is valid for the duration of this call.
  /// @param params A list of parameter values the client wishes to set. The
  /// buffer is valid for the duration of this call; if the callback wishes
  /// to store these values (e.g. to hand them off to another thread along
  /// with the responder), it must copy them out via `ParameterView::clone`.
  /// @param responder The responder used to complete the request.
  std::function<void(
    uint32_t client_id, std::optional<std::string_view> request_id,
    const std::vector<ParameterView>& params, SetParametersResponder&& responder
  )>
    onSet;
};

}  // namespace foxglove
