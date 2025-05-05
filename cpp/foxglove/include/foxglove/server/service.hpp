#pragma once

#include <foxglove/channel.hpp>
#include <foxglove/error.hpp>

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

/**
 * A service message schema, for either a request or a response.
 */
struct ServiceMessageSchema {
  std::string encoding;
  Schema schema;

private:
  friend struct ServiceSchema;
  void writeTo(foxglove_service_message_schema* c) const noexcept;
};

/**
 * A service schema.
 */
struct ServiceSchema {
  std::string name;
  std::optional<ServiceMessageSchema> request;
  std::optional<ServiceMessageSchema> response;

private:
  friend class Service;
  void writeTo(
    foxglove_service_schema* c, foxglove_service_message_schema* request,
    foxglove_service_message_schema* response
  ) const noexcept;
};

/**
 * A service request.
 *
 * This represents an individual client request. The service implementation is
 * responsible for parsing the request and sending a response in a timely
 * manner.
 */
struct ServiceRequest {
  std::string service_name;
  uint32_t client_id;
  uint32_t call_id;
  std::string encoding;
  std::vector<std::byte> payload;

private:
  friend class Service;
  explicit ServiceRequest(const foxglove_service_request* ptr) noexcept;
};

/**
 * A service responder.
 *
 * This is the means by which a service implementation responds to a request
 * from a client. Each request is paired with a unique responder instance, and
 * must be used exactly once.
 */
class ServiceResponder final {
public:
  /**
   * Sends response data to the client.
   */
  void respondOk(const std::vector<std::byte>& data) && noexcept;

  /**
   * Sends an error message to the client.
   */
  void respondError(std::string_view message) && noexcept;

  // Default destructor & move, disable copy.
  ~ServiceResponder() = default;
  ServiceResponder(ServiceResponder&&) noexcept = default;
  ServiceResponder& operator=(ServiceResponder&&) noexcept = default;
  ServiceResponder(const ServiceResponder&) = delete;
  ServiceResponder& operator=(const ServiceResponder&) = delete;

private:
  friend class Service;
  explicit ServiceResponder(foxglove_service_responder* ptr);

  struct Deleter {
    void operator()(foxglove_service_responder* ptr) const noexcept;
  };
  std::unique_ptr<foxglove_service_responder, Deleter> impl_;
};

/**
 * A service handler callback.
 *
 * This callback is invoked from the client's main poll loop and must not block.
 * If blocking or long-running behavior is required, the implementation should
 * return immediately and handle the request asynchronously.
 *
 * The `responder` represents an unfulfilled response. The implementation must
 * eventually call either `respondOk` or `respondError`, exactly once, in order
 * to complete the request. It is safe to invoke these completion methods
 * synchronously from the context of the callback.
 */
using ServiceHandler =
  std::function<void(const ServiceRequest& request, ServiceResponder&& responder)>;

/**
 * A service.
 */
class Service final {
public:
  /**
   * Constructs a new service.
   *
   * The service will not be active until it is registered with a server using
   * `WebSocketServer::addService()`.
   *
   * This constructor will fail with `FoxgloveError::Utf8Error` if the name is
   * not a valid UTF-8 string.
   */
  static FoxgloveResult<Service> create(
    std::string_view name, ServiceSchema& schema, ServiceHandler& handler
  );

  // Default destructor & move, disable copy.
  ~Service() = default;
  Service(Service&&) noexcept = default;
  Service& operator=(Service&&) noexcept = default;
  Service(const Service&) = delete;
  Service& operator=(const Service&) = delete;

private:
  friend class WebSocketServer;

  explicit Service(foxglove_service* ptr);
  foxglove_service* release() noexcept;

  struct Deleter {
    void operator()(foxglove_service* ptr) const noexcept;
  };
  std::unique_ptr<foxglove_service, Deleter> impl_;
};

}  // namespace foxglove
