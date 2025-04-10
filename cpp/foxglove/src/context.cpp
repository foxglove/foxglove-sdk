#include <foxglove-c/foxglove-c.h>
#include <foxglove/context.hpp>

namespace foxglove {

Context::Context()
    : _impl(foxglove_context_new(), foxglove_context_free) {}

Context::Context(const foxglove_context* context)
    : _impl(context, foxglove_context_free) {}

Context Context::get_default() {
  return Context(foxglove_context_get_default());
}

}  // namespace foxglove
