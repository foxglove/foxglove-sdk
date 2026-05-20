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

/// @brief Responder for a client `getParameters` request.
///
/// This is the means by which a parameter handler responds to a get request
/// from a client. Each request is paired with a unique responder instance, and
/// must be used exactly once. Dropping the responder without responding sends
/// a generic error status to the requesting client.
class GetParametersResponder final {
public:
  /// @brief Send parameter values back to the requesting client.
  ///
  /// Entries with an unset value are dropped before serialization.
  ///
  /// @param params Parameter values to send.
  void respond(std::vector<Parameter>&& params) && noexcept;

  /// @brief Default destructor. Sends a generic error status if the responder
  /// has not been consumed by `respond()`.
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
/// from a client. Each request is paired with a unique responder instance, and
/// must be used exactly once. The values passed to `respond()` are echoed back
/// to the requester (when the request carried a request_id) and broadcast to
/// all clients subscribed to those parameter names. Dropping the responder
/// without responding sends a generic error status to the requesting client
/// and does not broadcast anything.
class SetParametersResponder final {
public:
  /// @brief Acknowledge the set request with the values that were actually
  /// applied.
  ///
  /// Entries with an unset value are dropped before serialization.
  ///
  /// @param params Parameter values that were applied.
  void respond(std::vector<Parameter>&& params) && noexcept;

  /// @brief Default destructor. Sends a generic error status if the responder
  /// has not been consumed by `respond()`.
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
/// `onSetParameters` callbacks. Registering a handler automatically enables
/// the `Parameters` capability.
///
/// @note These callbacks are invoked from time-sensitive contexts and must not
/// block. If long-running work is required, the implementation should hand the
/// responder off to another thread and return immediately.
struct ParameterHandler {
  /// @brief Callback invoked when a client requests parameters.
  ///
  /// The implementation takes ownership of `responder`, and must eventually
  /// complete it by calling `responder.respond(...)`, or letting it go out of
  /// scope to send a generic error status.
  ///
  /// @param client_id The requesting client's ID.
  /// @param request_id A request ID unique to this client. May be empty.
  /// @param param_names A list of parameter names to fetch, or empty to
  /// request all parameters.
  /// @param responder The responder used to complete the request.
  std::function<void(
    uint32_t client_id, std::optional<std::string_view> request_id,
    const std::vector<std::string_view>& param_names, GetParametersResponder&& responder
  )>
    onGet;

  /// @brief Callback invoked when a client sets parameters.
  ///
  /// The implementation takes ownership of `responder`, and must eventually
  /// complete it by calling `responder.respond(...)` with the values that
  /// were actually applied, or letting it go out of scope to send a generic
  /// error status.
  ///
  /// @param client_id The requesting client's ID.
  /// @param request_id A request ID unique to this client. May be empty.
  /// @param params A list of parameter values the client wishes to set.
  /// @param responder The responder used to complete the request.
  std::function<void(
    uint32_t client_id, std::optional<std::string_view> request_id,
    const std::vector<ParameterView>& params, SetParametersResponder&& responder
  )>
    onSet;
};

}  // namespace foxglove
