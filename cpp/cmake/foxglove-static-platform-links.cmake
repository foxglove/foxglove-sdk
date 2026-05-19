# Platform link libraries required by the Rust C library's symbol surface. Applied
# as INTERFACE link libraries on the foxglove-static IMPORTED target so anything
# linking the static lib pulls these in automatically.
#
# Single source of truth for three sites:
#   - cpp/CMakeLists.txt              (in-tree build, includes from source dir)
#   - cpp/cmake/foxglove-sdkConfig.cmake.in  (local install, includes from package dir)
#   - cpp/cmake/foxglove-sdk-dist-config.cmake (dist, includes from package dir)
#
# Update this file when the Rust std lib's platform dependencies change.

if(NOT TARGET foxglove-static)
  return()
endif()

if(WIN32)
  set_property(TARGET foxglove-static APPEND PROPERTY
    INTERFACE_LINK_LIBRARIES Bcrypt SChannel Crypt32 Ncrypt)
elseif(APPLE)
  set_property(TARGET foxglove-static APPEND PROPERTY
    INTERFACE_LINK_LIBRARIES "-framework Security" "-framework CoreFoundation")
elseif(UNIX)
  # Required for the Rust standard library on older glibc (< 2.34) where these are separate libraries.
  set_property(TARGET foxglove-static APPEND PROPERTY
    INTERFACE_LINK_LIBRARIES pthread dl m)
endif()
