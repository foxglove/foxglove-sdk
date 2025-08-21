#include <foxglove-c/foxglove-c.h>
#include <foxglove/server/service.hpp>

#include <memory>

using std::string_view_literals::operator""sv;

namespace foxglove {

/**
 * ServiceMessageSchema implementation.
 */
void ServiceMessageSchema::writeTo(foxglove_service_message_schema* c) const noexcept {
  c->encoding = {this->encoding.data(), this->encoding.length()};
  c->schema.name = {this->schema.name.data(), this->schema.name.length()};
  c->schema.encoding = {this->schema.encoding.data(), this->schema.encoding.length()};
  c->schema.data = reinterpret_cast<const uint8_t*>(this->schema.data);
  c->schema.data_len = this->schema.data_len;
}

/**
 * ServiceSchema implementation.
 */
void ServiceSchema::writeTo(
  foxglove_service_schema* c, std::array<foxglove_service_message_schema, 2>& msg_schemas
) const noexcept {
  c->name = {this->name.data(), this->name.length()};
  if (this->request.has_value()) {
    this->request->writeTo(msg_schemas.data());
    c->request = msg_schemas.data();
  } else {
    c->request = nullptr;
  }
  if (this->response.has_value()) {
    this->response->writeTo(&msg_schemas[1]);
    c->response = &msg_schemas[1];
  } else {
    c->response = nullptr;
  }
}

/**
 * ServiceRequest implementation.
 */

ServiceRequest::ServiceRequest(const foxglove_service_request* r) noexcept
    : service_name(r->service_name.data, r->service_name.len)
    , client_id(r->client_id)
    , call_id(r->call_id)
    , encoding(r->encoding.data, r->encoding.len)
    , payload(
        reinterpret_cast<const std::byte*>(r->payload.data),
        reinterpret_cast<const std::byte*>(r->payload.data + r->payload.len)
      ) {}

/**
 * ServiceResponder implementation.
 */
void ServiceResponder::Deleter::operator()(foxglove_service_responder* ptr) const noexcept {
  auto message = "Internal server error: Service failed to send a response"sv;
  foxglove_service_respond_error(ptr, {message.data(), message.length()});
}

void ServiceResponder::respondOk(const std::byte* data, size_t size) && noexcept {
  auto* ptr = impl_.release();
  foxglove_service_respond_ok(ptr, {reinterpret_cast<const uint8_t*>(data), size});
}

void ServiceResponder::respondError(std::string_view message) && noexcept {
  auto* ptr = impl_.release();
  foxglove_service_respond_error(ptr, {message.data(), message.length()});
}

/**
 * Service implementation.
 */
FoxgloveResult<Service> Service::create(
  std::string_view name, ServiceSchema& schema, ServiceHandler& handler
) {
  std::array<foxglove_service_message_schema, 2> msg_schemas{};
  foxglove_service_schema c_schema;
  schema.writeTo(&c_schema, msg_schemas);
  foxglove_service* ptr = nullptr;
  auto error = foxglove_service_create(
    &ptr,
    {name.data(), name.length()},
    &c_schema,
    &handler,
    [](
      const void* context,
      const foxglove_service_request* c_request,
      foxglove_service_responder* c_responder
    ) {
      const auto* handler = static_cast<const ServiceHandler*>(context);
      ServiceRequest request(c_request);
      ServiceResponder responder(c_responder);
      try {
        (*handler)(request, std::move(responder));
      } catch (const std::exception& exc) {
        warn() << "Service handler failed: " << exc.what();
      }
    }
  );
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    return tl::unexpected(static_cast<FoxgloveError>(error));
  }
  return Service(ptr);
}

void Service::Deleter::operator()(foxglove_service* ptr) const noexcept {
  foxglove_service_free(ptr);
}

foxglove_service* Service::release() noexcept {
  return impl_.release();
}

}  // namespace foxglove
