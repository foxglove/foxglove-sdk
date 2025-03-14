#include <foxglove-c/foxglove-c.h>
#include <foxglove/mcap.hpp>

namespace foxglove {

McapWriter::McapWriter(McapWriterOptions options)
    : _impl(nullptr, foxglove_mcap_free) {
    foxglove_mcap_options cOptions = {};
    cOptions.path = options.path.data();
    cOptions.path_len = options.path.length();
    cOptions.create = options.create;
    cOptions.truncate = options.truncate;
    _impl.reset(foxglove_mcap_open(&cOptions));
}

void McapWriter::close() {
    foxglove_mcap_close(_impl.get());
}

}  // namespace foxglove
