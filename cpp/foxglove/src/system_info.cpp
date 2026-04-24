#include <foxglove-c/foxglove-c.h>
#include <foxglove/error.hpp>
#include <foxglove/system_info.hpp>

#include <algorithm>

namespace foxglove {

FoxgloveResult<SystemInfoPublisher> SystemInfoPublisher::create(
  SystemInfoOptions&& options  // NOLINT(cppcoreguidelines-rvalue-reference-param-not-moved)
) {
  foxglove_internal_register_cpp_wrapper();

  foxglove_system_info_publisher_options c_options = {};
  c_options.context = options.context.getInner();

  if (options.topic) {
    c_options.topic = foxglove_string{options.topic->data(), options.topic->length()};
  }

  std::optional<uint64_t> refresh_interval_ms;
  if (options.refresh_interval) {
    refresh_interval_ms = static_cast<uint64_t>(std::max(options.refresh_interval->count(), 0));
    c_options.refresh_interval_ms = &*refresh_interval_ms;
  }

  foxglove_system_info_publisher* publisher = nullptr;
  foxglove_error error = foxglove_system_info_publisher_start(&c_options, &publisher);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK || publisher == nullptr) {
    return tl::unexpected(static_cast<FoxgloveError>(error));
  }
  return SystemInfoPublisher(publisher);
}

SystemInfoPublisher::SystemInfoPublisher(foxglove_system_info_publisher* impl)
    : impl_(impl, foxglove_system_info_publisher_detach) {}

FoxgloveError SystemInfoPublisher::stop() noexcept {
  if (auto* impl = impl_.release()) {
    return static_cast<FoxgloveError>(foxglove_system_info_publisher_stop(impl));
  }
  return FoxgloveError::Ok;
}

}  // namespace foxglove
