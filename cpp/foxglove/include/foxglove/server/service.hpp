#pragma once

#include <foxglove/channel.hpp>
#include <foxglove/error.hpp>

#include <array>
#include <cstdint>
#include <functional>
#include <memory>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

struct foxglove_service;
struct foxglove_service_message_schema;
struct foxglove_service_request;
struct foxglove_service_responder;
struct foxglove_service_schema;

namespace foxglove {

/// @brief A service message schema, for either a request or a response.
struct ServiceMessageSchema {
  /// Message encoding.
  std::string encoding;
  /// Message schema.
  Schema schema;

private:
  friend struct ServiceSchema;

  /// @brief Writes the schema to the provided C-style struct.
  void writeTo(foxglove_service_message_schema* c) const noexcept;
};

/// @brief A service schema.
struct ServiceSchema {
  /// Schema name.
  std::string name;
  /// Request schema.
  std::optional<ServiceMessageSchema> request = std::nullopt;
  /// Response schema.
  std::optional<ServiceMessageSchema> response = std::nullopt;

private:
  friend class Service;

  /// @brief Writes the schema to the provided C-style structs.
  void writeTo(
    foxglove_service_schema* c, std::array<foxglove_service_message_schema, 2>& msg_schemas
  ) const noexcept;
};

/// A service request.
///
/// This represents an individual client request. The service implementation is
/// responsible for parsing the request and sending a response in a timely
/// manner.
struct ServiceRequest {
  /// Service name.
  std::string service_name;
  /// Locally unique client ID.
  uint32_t client_id;
  /// Call ID, unique to the client.
  uint32_t call_id;
  /// Message encoding.
  std::string encoding;
  /// Request message data.
  std::vector<std::byte> payload;

  /// @brief Returns a string view of the payload.
  [[nodiscard]] std::string_view payloadStr() const noexcept {
    return {reinterpret_cast<const char*>(this->payload.data()), this->payload.size()};
  }

private:
  friend class Service;

  /// @brief Constructor from a raw pointer.
  explicit ServiceRequest(const foxglove_service_request* ptr) noexcept;
};

/// @brief A service responder.
///
/// This is the means by which a service implementation responds to a request
/// from a client. Each request is paired with a unique responder instance, and
/// must be used exactly once.
class ServiceResponder final {
public:
  /// @brief Sends response data to the client.
  ///
  /// @param data Response data pointer.
  /// @param size Response data length.
  void respondOk(const std::byte* data, size_t size) && noexcept;

  /// @brief Sends response data to the client.
  ///
  /// @param data Response data.
  void respondOk(const std::vector<std::byte>& data) && noexcept {
    std::move(*this).respondOk(data.data(), data.size());
  };

  /// @brief Sends an error message to the client.
  ///
  /// @param message Error message, which must be valid UTF-8.
  void respondError(std::string_view message) && noexcept;

  /// @brief Default destructor.
  ~ServiceResponder() = default;
  /// @brief Default move constructor.
  ServiceResponder(ServiceResponder&&) noexcept = default;
  /// @brief Default move assignment.
  ServiceResponder& operator=(ServiceResponder&&) noexcept = default;
  ServiceResponder(const ServiceResponder&) = delete;
  ServiceResponder& operator=(const ServiceResponder&) = delete;

private:
  friend class Service;

  struct Deleter {
    void operator()(foxglove_service_responder* ptr) const noexcept;
  };

  std::unique_ptr<foxglove_service_responder, Deleter> impl_;

  /// @brief Constructor from a raw pointer.
  explicit ServiceResponder(foxglove_service_responder* ptr) noexcept
      : impl_(ptr) {}
};

/// @brief A service handler callback.
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
/// @param request The client request.
/// @param responder The responder used to send a response to the client.
using ServiceHandler =
  std::function<void(const ServiceRequest& request, ServiceResponder&& responder)>;

/// @brief A service.
class Service final {
public:
  /// @brief Constructs a new service.
  ///
  /// The service will not be active until it is registered with a server using
  /// `WebSocketServer::addService()`.
  ///
  /// This constructor will fail with `FoxgloveError::Utf8Error` if the name is
  /// not a valid UTF-8 string.
  ///
  /// @param name Locally unique service name.
  /// @param schema Service schema.
  /// @param handler Service handler callback.
  static FoxgloveResult<Service> create(
    std::string_view name, ServiceSchema& schema, ServiceHandler& handler
  );

  /// @brief Default destructor.
  ~Service() = default;
  /// @brief Default move constructor.
  Service(Service&&) noexcept = default;
  /// @brief Default move assignment.
  Service& operator=(Service&&) noexcept = default;
  Service(const Service&) = delete;
  Service& operator=(const Service&) = delete;

private:
  friend class WebSocketServer;

  struct Deleter {
    void operator()(foxglove_service* ptr) const noexcept;
  };

  std::unique_ptr<foxglove_service, Deleter> impl_;

  /// @brief Constructor from a raw pointer.
  explicit Service(foxglove_service* ptr) noexcept
      : impl_(ptr) {}

  /// @brief Releases ownership of the underlying storage.
  foxglove_service* release() noexcept;
};

}  // namespace foxglove
