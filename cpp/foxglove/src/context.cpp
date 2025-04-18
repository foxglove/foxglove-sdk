#include <foxglove-c/foxglove-c.h>
#include <foxglove/context.hpp>

namespace foxglove {

Context::Context(const foxglove_context* context)
    : _impl(context, foxglove_context_free) {}

Context Context::create() {
  return Context(foxglove_context_new());
}

}  // namespace foxglove
