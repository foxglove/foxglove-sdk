cmake_minimum_required(VERSION 3.17)

if(POLICY CMP0048)
  cmake_policy(SET CMP0048 NEW)
  set(CMAKE_POLICY_DEFAULT_CMP0048 NEW)
endif()
if(POLICY CMP0024)
  cmake_policy(SET CMP0024 NEW)
  set(CMAKE_POLICY_DEFAULT_CMP0024 NEW)
endif()

project(foxglove_bridge_core LANGUAGES CXX VERSION 3.4.1)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Avoid DOWNLOAD_EXTRACT_TIMESTAMP warning for CMake >= 3.24
if (POLICY CMP0135)
  cmake_policy(SET CMP0135 NEW)
endif()

# Remote access requires glibc >= 2.35 on Linux. Detect the build host's glibc
# (when not cross-compiling) so we can lower the default to OFF on older
# systems and warn if the user pins it ON.
set(_glibc_detected FALSE)
set(_glibc_too_old FALSE)
set(_glibc_version "")
if(CMAKE_SYSTEM_NAME STREQUAL "Linux" AND NOT CMAKE_CROSSCOMPILING)
  execute_process(
    COMMAND getconf GNU_LIBC_VERSION
    OUTPUT_VARIABLE _glibc_version_output
    OUTPUT_STRIP_TRAILING_WHITESPACE
    ERROR_QUIET
    RESULT_VARIABLE _glibc_result
  )
  if(_glibc_result EQUAL 0 AND _glibc_version_output MATCHES "glibc ([0-9]+)\\.([0-9]+)")
    set(_glibc_major ${CMAKE_MATCH_1})
    set(_glibc_minor ${CMAKE_MATCH_2})
    set(_glibc_detected TRUE)
    set(_glibc_version "${_glibc_major}.${_glibc_minor}")
    if(_glibc_major LESS 2 OR (_glibc_major EQUAL 2 AND _glibc_minor LESS 35))
      set(_glibc_too_old TRUE)
    endif()
  endif()
endif()

set(_remote_access_default ON)
if(NOT DEFINED CACHE{FOXGLOVE_BRIDGE_REMOTE_ACCESS})
  if(_glibc_too_old)
    message(STATUS "Detected glibc ${_glibc_version} (< 2.35); defaulting FOXGLOVE_BRIDGE_REMOTE_ACCESS to OFF")
    set(_remote_access_default OFF)
  elseif(NOT _glibc_detected)
    message(STATUS "Could not detect glibc version, defaulting FOXGLOVE_BRIDGE_REMOTE_ACCESS to ON")
  endif()
endif()
option(FOXGLOVE_BRIDGE_REMOTE_ACCESS "Build with remote access gateway support" ${_remote_access_default})

if(FOXGLOVE_BRIDGE_REMOTE_ACCESS AND _glibc_too_old)
  message(WARNING "FOXGLOVE_BRIDGE_REMOTE_ACCESS=ON but build host has glibc ${_glibc_version} (< 2.35); the link step is likely to fail.")
endif()

macro(enable_strict_compiler_warnings target)
  if (MSVC)
    target_compile_options(${target} PRIVATE /WX /W4)
  elseif(CMAKE_CXX_COMPILER_ID MATCHES "Clang")
    target_compile_options(${target} PRIVATE -Wall -Wextra -Wpedantic -Werror -Wold-style-cast -Wfloat-equal -Wmost -Wunused-exception-parameter)
  else()
    target_compile_options(${target} PRIVATE -Wall -Wextra -Wpedantic -Werror -Wold-style-cast -Wfloat-equal)
  endif()
endmacro()

find_package(Threads REQUIRED)

if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE RelWithDebInfo)
endif()

# Detect big-endian architectures
include(TestBigEndian)
TEST_BIG_ENDIAN(ENDIAN)
if (ENDIAN)
  add_compile_definitions(ARCH_IS_BIG_ENDIAN=1)
endif()

# Fetch the released SDK zip from GitHub. To build against a locally-modified SDK,
# override FETCHCONTENT_SOURCE_DIR_FOXGLOVE_SDK (cmake's standard FetchContent override)
# to point at a `cpp/dist`-shaped directory — the ros/Makefile's FOXGLOVE_CPP_SDK_DIR
# variable does this for you (see ros/README.md). The dist directory needs to contain
# lib/cmake/foxglove-sdk/foxglove-sdkConfig.cmake; release zips built before the SDK's
# cmake-glue change don't yet, so the FetchContent download path is broken until a
# release ships with that glue. For local development, point the override at a
# `make build-cpp-dist`-produced tree.
set(FOXGLOVE_SDK_VERSION "0.25.1")

if (CMAKE_SYSTEM_NAME STREQUAL "Linux" AND CMAKE_SYSTEM_PROCESSOR STREQUAL "aarch64")
  set(FOXGLOVE_SDK_PLATFORM "aarch64-unknown-linux-gnu")
  set(FOXGLOVE_SDK_SHA "c70e35c78f0e37f3ba8d0251cd31f353bb10b1fd106200dc6c5f409ed4df1918")
elseif(CMAKE_SYSTEM_NAME STREQUAL "Linux" AND CMAKE_SYSTEM_PROCESSOR STREQUAL "x86_64")
  set(FOXGLOVE_SDK_PLATFORM "x86_64-unknown-linux-gnu")
  set(FOXGLOVE_SDK_SHA "0b9d348df3a8e98d2c2cee2360cc9f52e83bb3445044debd771d1b46dc358be4")
else()
  message(FATAL_ERROR "Unsupported platform/architecture combination: ${CMAKE_SYSTEM_PROCESSOR}-${CMAKE_SYSTEM_NAME}")
endif()
set(FOXGLOVE_SDK_URL "https://github.com/foxglove/foxglove-sdk/releases/download/sdk%2Fv${FOXGLOVE_SDK_VERSION}/foxglove-v${FOXGLOVE_SDK_VERSION}-cpp-${FOXGLOVE_SDK_PLATFORM}.zip")

include(FetchContent)
FetchContent_Declare(
  foxglove_sdk
  URL ${FOXGLOVE_SDK_URL}
  URL_HASH SHA256=${FOXGLOVE_SDK_SHA}
)
FetchContent_MakeAvailable(foxglove_sdk)

# The dist's CMake package config defines IMPORTED targets for the prebuilt C libraries
# (foxglove-static, foxglove-shared) and the foxglove_sdk_add_cpp_library() helper, which
# compiles the SDK's C++ wrapper sources against our toolchain. find_package looks for
# the config at <sdk>/lib/cmake/foxglove-sdk/foxglove-sdkConfig.cmake.
find_package(foxglove-sdk CONFIG REQUIRED HINTS "${foxglove_sdk_SOURCE_DIR}")

# Always build the C++ wrapper as a SHARED library: with RA, dynamic-link the RA
# cdylib (libfoxglove.so); without RA, statically link the staticlib into the wrapper.
if (FOXGLOVE_BRIDGE_REMOTE_ACCESS)
  foxglove_sdk_add_cpp_library(foxglove_cpp_shared TYPE SHARED REMOTE_ACCESS ON)
else()
  foxglove_sdk_add_cpp_library(foxglove_cpp_shared TYPE SHARED REMOTE_ACCESS OFF)
endif()
set_target_properties(foxglove_cpp_shared PROPERTIES
  INSTALL_RPATH "$ORIGIN"
  BUILD_WITH_INSTALL_RPATH TRUE
)

add_library(foxglove_bridge_core SHARED
  src/capabilities.cpp
  src/transport_manager.cpp
)
target_include_directories(foxglove_bridge_core
  PUBLIC
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
    $<INSTALL_INTERFACE:include>
)
target_link_libraries(foxglove_bridge_core
  PUBLIC foxglove_cpp_shared
  PRIVATE Threads::Threads
)
if (FOXGLOVE_BRIDGE_REMOTE_ACCESS)
  target_compile_definitions(foxglove_bridge_core PUBLIC FOXGLOVE_REMOTE_ACCESS)
endif()
enable_strict_compiler_warnings(foxglove_bridge_core)
set_target_properties(foxglove_bridge_core PROPERTIES
  INSTALL_RPATH "$ORIGIN"
  BUILD_WITH_INSTALL_RPATH TRUE
)

# ---------------------------------------------------------------------------
# Install & export
#
# Installs the core library, the SDK C++ wrapper, the SDK headers, and (with
# remote access) the SDK cdylib, then ships a hand-written CMake config that
# defines IMPORTED targets for consumers (the same approach as the SDK dist's
# own config). Consumers use:
#
#   find_package(foxglove_bridge_core CONFIG REQUIRED)
#   target_link_libraries(my_target foxglove_bridge_core::foxglove_bridge_core)
#
# The foxglove_bridge_core::foxglove_bridge_core target transitively provides
# the SDK headers, libraries, and (when enabled) the FOXGLOVE_REMOTE_ACCESS
# compile definition.
# ---------------------------------------------------------------------------

install(TARGETS foxglove_bridge_core foxglove_cpp_shared
  ARCHIVE DESTINATION lib
  LIBRARY DESTINATION lib
  RUNTIME DESTINATION bin
)
# The remote-access cdylib must live alongside the wrapper for its $ORIGIN rpath.
if (FOXGLOVE_BRIDGE_REMOTE_ACCESS)
  install(FILES $<TARGET_FILE:foxglove-shared> DESTINATION lib)
endif()

install(DIRECTORY include/foxglove_bridge_core
  DESTINATION include
)
# SDK headers (C++ wrapper + C), so consumers can include <foxglove/...>.
install(DIRECTORY
  "${foxglove_sdk_SOURCE_DIR}/include/foxglove"
  "${foxglove_sdk_SOURCE_DIR}/include/foxglove-c"
  DESTINATION include
)

configure_file(cmake/foxglove_bridge_coreConfig.cmake.in
  "${CMAKE_CURRENT_BINARY_DIR}/foxglove_bridge_coreConfig.cmake" @ONLY)
install(FILES "${CMAKE_CURRENT_BINARY_DIR}/foxglove_bridge_coreConfig.cmake"
  DESTINATION share/${PROJECT_NAME}/cmake
)
