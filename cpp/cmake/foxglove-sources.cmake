# Single source of truth for the C++ wrapper source filenames. Used by:
#   - cpp/CMakeLists.txt (in-tree build; splits the buckets into separate OBJECT
#     libs so generated code can disable clang-tidy and RA code can be conditional)
#   - cpp/cmake/foxglove-sdk-dist-config.cmake (dist; consumers compile the union
#     of handwritten + generated into one wrapper library, with the RA source added
#     when REMOTE_ACCESS is on)
#
# Each list contains bare filenames; consumers prepend their own source root.
#
# Add new source files here. The in-tree build and the dist both pick them up.

set(FOXGLOVE_CPP_HANDWRITTEN_SOURCES
  callback_forwarders.cpp
  channel.cpp
  connection_graph.cpp
  context.cpp
  error.cpp
  fetch_asset.cpp
  foxglove.cpp
  mcap.cpp
  parameter.cpp
  parameter_handler.cpp
  service.cpp
  system_info.cpp
  websocket.cpp
)

set(FOXGLOVE_CPP_GENERATED_SOURCES
  messages.cpp
)

set(FOXGLOVE_CPP_REMOTE_ACCESS_SOURCES
  remote_access.cpp
)
