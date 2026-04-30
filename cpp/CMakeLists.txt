# This CMake file is used for CI, examples, and tests. It builds the whole project from scratch
# including the Rust code. If you would like to use the C++ library only, you only need to compile
# the sources under `cpp/foxglove`, link with the C static or shared library, and add both the C/C++
# includes to include paths.

cmake_minimum_required(VERSION 3.22)
project(foxglove-sdk VERSION 0.23.0)

# Enable ccache if available
find_program(CCACHE_PROGRAM ccache)
if(CCACHE_PROGRAM)
  set(CMAKE_C_COMPILER_LAUNCHER "${CCACHE_PROGRAM}")
  set(CMAKE_CXX_COMPILER_LAUNCHER "${CCACHE_PROGRAM}")
  message(STATUS "Using ccache: ${CCACHE_PROGRAM}")
endif()

option(USE_PACKAGE_MANAGER_DEPENDENCIES
  "Try to find dependencies provided by the local package manager, and if not found, fall back to building them from source"
  ON)
option(FOXGLOVE_REMOTE_ACCESS "Build with remote access gateway support" OFF)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED True)

include(CTest)
include(FetchContent)
include(${CMAKE_CURRENT_SOURCE_DIR}/cmake/FindOrFetch.cmake)

# Suppress CMake deprecation warnings from vendored dependencies with older cmake_minimum_required
set(CMAKE_POLICY_VERSION_MINIMUM 3.5)
set(CMAKE_WARN_DEPRECATED OFF CACHE BOOL "" FORCE)

find_or_fetch(Catch2
  GIT_REPOSITORY https://github.com/catchorg/Catch2.git
  GIT_TAG        v3.8.0
  VERSION 3
)
if(catch2_SOURCE_DIR)
  list(APPEND CMAKE_MODULE_PATH ${catch2_SOURCE_DIR}/extras)
endif()
include(Catch)

find_or_fetch(Corrosion
  GIT_REPOSITORY https://github.com/corrosion-rs/corrosion.git
  GIT_TAG v0.5.1
  VERSION 0.5.1
)

set(LWS_WITH_SSL OFF CACHE BOOL "" FORCE)
set(LWS_WITH_SHARED OFF CACHE BOOL "" FORCE)
set(LWS_WITH_STATIC ON CACHE BOOL "" FORCE)
set(LWS_WITHOUT_TESTAPPS ON CACHE BOOL "" FORCE)
set(LWS_WITHOUT_TEST_SERVER ON CACHE BOOL "" FORCE)
set(LWS_WITHOUT_TEST_PING ON CACHE BOOL "" FORCE)
set(LWS_WITHOUT_TEST_CLIENT ON CACHE BOOL "" FORCE)
find_or_fetch(libwebsockets
  GIT_REPOSITORY https://github.com/warmcat/libwebsockets.git
  GIT_TAG v4.3.3
  # Ubuntu packages the config file as LibwebsocketsConfig.cmake (capital L),
  # which doesn't match find_package(libwebsockets) on case-sensitive filesystems.
  ALT_NAME Libwebsockets
  # Prevent libwebsockets' install rules from polluting our install.
  EXCLUDE_FROM_ALL
)
# System-installed libwebsockets provides the "websockets_shared" target, while the FetchContent
# build provides "websockets". Create an alias so the rest of this file can use "websockets"
# consistently.
if(NOT TARGET websockets AND TARGET websockets_shared)
  add_library(websockets ALIAS websockets_shared)
endif()

find_or_fetch(nlohmann_json
  GIT_REPOSITORY https://github.com/nlohmann/json.git
  GIT_TAG v3.12.0
)

set(BASE64_ENABLE_TESTING OFF CACHE BOOL "" FORCE)
find_or_fetch(base64
  GIT_REPOSITORY https://github.com/tobiaslocker/base64.git
  GIT_TAG 8d96a2a737ac1396304b1de289beb3a5ea0cb752
)

option(BUILD_REMOTE_DATA_LOADER_BACKEND_EXAMPLE "Build the remote data loader backend example (requires cpp-httplib and date)" ON)

if(BUILD_REMOTE_DATA_LOADER_BACKEND_EXAMPLE)
  set(ENABLE_DATE_TESTING OFF CACHE BOOL "" FORCE)
  set(ENABLE_DATE_INSTALL OFF CACHE BOOL "" FORCE)
  find_or_fetch(date
    GIT_REPOSITORY https://github.com/HowardHinnant/date.git
    GIT_TAG v3.0.4
  )

  set(HTTPLIB_COMPILE OFF CACHE BOOL "" FORCE)
  set(HTTPLIB_REQUIRE_OPENSSL OFF CACHE BOOL "" FORCE)
  set(HTTPLIB_USE_OPENSSL_IF_AVAILABLE OFF CACHE BOOL "" FORCE)
  set(HTTPLIB_REQUIRE_ZLIB OFF CACHE BOOL "" FORCE)
  set(HTTPLIB_USE_ZLIB_IF_AVAILABLE OFF CACHE BOOL "" FORCE)
  set(HTTPLIB_USE_BROTLI_IF_AVAILABLE OFF CACHE BOOL "" FORCE)
  set(HTTPLIB_USE_ZSTD_IF_AVAILABLE OFF CACHE BOOL "" FORCE)
  set(HTTPLIB_USE_NON_BLOCKING_GETADDRINFO OFF CACHE BOOL "" FORCE)
  set(HTTPLIB_INSTALL OFF CACHE BOOL "" FORCE)
  find_or_fetch(httplib
    GIT_REPOSITORY https://github.com/yhirose/cpp-httplib.git
    GIT_TAG v0.32.0
  )
endif()

set(CMAKE_WARN_DEPRECATED ON CACHE BOOL "" FORCE)

option(BUILD_WS_STREAM_MCAP_EXAMPLE "Build the MCAP websocket streaming example (requires mcap and lz4)" ON)

if(BUILD_WS_STREAM_MCAP_EXAMPLE)
  find_or_fetch(mcap
    GIT_REPOSITORY https://github.com/foxglove/mcap.git
    GIT_TAG releases/cpp/v2.1.2
  )

  set(LZ4_BUILD_CLI OFF CACHE BOOL "" FORCE)
  set(LZ4_BUILD_LEGACY_LZ4C OFF CACHE BOOL "" FORCE)
  FetchContent_Declare(
    lz4
    GIT_REPOSITORY https://github.com/lz4/lz4.git
    GIT_TAG v1.10.0
    SOURCE_SUBDIR build/cmake
  )
  FetchContent_MakeAvailable(lz4)
endif()

if(DEFINED SANITIZE)
  if(NOT CMAKE_CXX_COMPILER_ID MATCHES ".*Clang")
    message(FATAL_ERROR "Sanitizers are only supported with Clang, not ${CMAKE_CXX_COMPILER_ID}")
  endif()
  set(SANITIZER_COMPILE_OPTIONS -fsanitize=${SANITIZE} -fsanitize-ignorelist=${CMAKE_CURRENT_SOURCE_DIR}/sanitize-ignorelist.txt -fno-omit-frame-pointer -fno-sanitize-recover=all)
  set(SANITIZER_LINK_OPTIONS -fsanitize=${SANITIZE})
  set(SANITIZER_CARGO_FLAGS -Zbuild-std)
endif()

option(STRICT "Enable strict compiler warnings" ON)
if (STRICT)
  if(CMAKE_CXX_COMPILER_ID MATCHES ".*Clang")
    set (STRICT_COMPILE_OPTIONS -Wall -Wextra -Wpedantic -Werror -Wold-style-cast -Wmost -Wunused-exception-parameter)
  elseif(MSVC)
    set (STRICT_COMPILE_OPTIONS /W4 /WX)
  endif()
endif()

# GCC 11 (e.g. Ubuntu 22.04) requires -fpermissive for some constructs compiled by webrtc-sys.
# Propagate to Rust crate builds that compile C++ via the cc crate.
if(CMAKE_CXX_COMPILER_ID STREQUAL "GNU" AND CMAKE_CXX_COMPILER_VERSION VERSION_LESS 12)
  set(FOXGLOVE_CORROSION_CXXFLAGS "-fpermissive")
endif()

option(CLANG_TIDY "Run clang-tidy static analysis" OFF)
if(CLANG_TIDY)
  find_program(CLANG_TIDY_EXE NAMES clang-tidy REQUIRED)
  message(STATUS "Found clang-tidy: ${CLANG_TIDY_EXE}")
  set(CMAKE_CXX_CLANG_TIDY "${CLANG_TIDY_EXE}")
endif()

### C

# FOXGLOVE_PREBUILT_LIB_DIR: path to a directory containing pre-built libfoxglove.{a,so,lib,dll,dylib}.
# When set, these are used directly instead of building via corrosion. This is used by CI to avoid
# redundant Rust builds and Windows long-path issues with corrosion's nested build directory.
if(FOXGLOVE_PREBUILT_LIB_DIR)
  get_filename_component(FOXGLOVE_PREBUILT_LIB_DIR "${FOXGLOVE_PREBUILT_LIB_DIR}" ABSOLUTE)
  if(NOT FOXGLOVE_REMOTE_ACCESS)
    add_library(foxglove-static STATIC IMPORTED GLOBAL)
    set_target_properties(foxglove-static PROPERTIES
      IMPORTED_LOCATION "${FOXGLOVE_PREBUILT_LIB_DIR}/${CMAKE_STATIC_LIBRARY_PREFIX}foxglove${CMAKE_STATIC_LIBRARY_SUFFIX}"
    )
  endif()
  add_library(foxglove-shared SHARED IMPORTED GLOBAL)
  set_target_properties(foxglove-shared PROPERTIES
    IMPORTED_LOCATION "${FOXGLOVE_PREBUILT_LIB_DIR}/${CMAKE_SHARED_LIBRARY_PREFIX}foxglove${CMAKE_SHARED_LIBRARY_SUFFIX}"
  )
  if(WIN32)
    set_target_properties(foxglove-shared PROPERTIES
      IMPORTED_IMPLIB "${FOXGLOVE_PREBUILT_LIB_DIR}/foxglove.dll.lib"
    )
    # Copy the DLL into each build output directory so executables can find it at runtime.
    # This is needed for catch_discover_tests (which runs the test exe at build time) and ctest.
    foreach(_config Release RelWithDebInfo Debug MinSizeRel)
      file(COPY "${FOXGLOVE_PREBUILT_LIB_DIR}/foxglove.dll" DESTINATION "${CMAKE_BINARY_DIR}/${_config}")
    endforeach()
  endif()
else()
  if(FOXGLOVE_REMOTE_ACCESS)
    # RA builds only produce the cdylib (shared library). A staticlib is not provided because
    # the LiveKit/WebRTC dependency has strict ABI requirements and would leak internal symbols
    # into the consumer's binary.
    corrosion_import_crate(
      MANIFEST_PATH ../c/Cargo.toml
      CRATES foxglove_c
      CRATE_TYPES cdylib
      FEATURES remote-access
      FLAGS ${SANITIZER_CARGO_FLAGS}
    )
  else()
    corrosion_import_crate(
      MANIFEST_PATH ../c/Cargo.toml
      CRATES foxglove_c
      FLAGS ${SANITIZER_CARGO_FLAGS}
    )
  endif()
  if(DEFINED SANITIZE)
    # Rust does not have flags for certain sanitizers so we strip them out
    string(REGEX REPLACE undefined "" rust_sanitizers ${SANITIZE})
    string(REGEX REPLACE ,, , rust_sanitizers ${rust_sanitizers})
    string(REGEX REPLACE ,$ "" rust_sanitizers ${rust_sanitizers})
    message(WARNING "Enabling -Zsanitizer=${rust_sanitizers} for Rust")
    corrosion_add_target_rustflags(foxglove -Zsanitizer=${rust_sanitizers} -Cforce-frame-pointers=yes)
  endif()
  if(FOXGLOVE_CORROSION_CXXFLAGS)
    corrosion_set_env_vars(foxglove "CXXFLAGS=${FOXGLOVE_CORROSION_CXXFLAGS}")
  endif()
endif()

if(TARGET foxglove-static)
  if(WIN32)
    target_link_libraries(foxglove-static INTERFACE Bcrypt SChannel Crypt32 Ncrypt)
  elseif(APPLE)
    target_link_libraries(foxglove-static INTERFACE "-framework Security" "-framework CoreFoundation")
  elseif(UNIX)
    # Required for the Rust standard library on older glibc (< 2.34) where these are separate libraries.
    target_link_libraries(foxglove-static INTERFACE pthread dl m)
  endif()
endif()


### C++

# Generated code (excluded from clang-tidy)
set(foxglove_generated_srcs
    "foxglove/include/foxglove/messages.hpp"
    "foxglove/src/messages.cpp"
)
add_library(foxglove_generated_obj OBJECT "${foxglove_generated_srcs}")
set_property(TARGET foxglove_generated_obj PROPERTY POSITION_INDEPENDENT_CODE True)
set_property(TARGET foxglove_generated_obj PROPERTY CXX_STANDARD 17)
set_property(TARGET foxglove_generated_obj PROPERTY CXX_STANDARD_REQUIRED True)
set_property(TARGET foxglove_generated_obj PROPERTY CXX_CLANG_TIDY "")
target_include_directories(foxglove_generated_obj PRIVATE
  ${CMAKE_CURRENT_SOURCE_DIR}/foxglove/include
  ${CMAKE_CURRENT_SOURCE_DIR}/../c/include
)
target_compile_options(foxglove_generated_obj PRIVATE ${SANITIZER_COMPILE_OPTIONS} ${STRICT_COMPILE_OPTIONS})
target_link_options(foxglove_generated_obj PRIVATE ${SANITIZER_LINK_OPTIONS})

# Hand-written C++ code
file(GLOB_RECURSE foxglove_cpp_srcs CONFIGURE_DEPENDS
    "foxglove/include/*.hpp"
    "foxglove/src/*.hpp"
    "foxglove/src/*.cpp"
)
list(REMOVE_ITEM foxglove_cpp_srcs
    "${CMAKE_CURRENT_SOURCE_DIR}/foxglove/include/foxglove/messages.hpp"
    "${CMAKE_CURRENT_SOURCE_DIR}/foxglove/src/messages.cpp"
    "${CMAKE_CURRENT_SOURCE_DIR}/foxglove/include/foxglove/remote_access.hpp"
    "${CMAKE_CURRENT_SOURCE_DIR}/foxglove/src/remote_access.cpp"
)

add_library(foxglove_cpp_obj OBJECT "${foxglove_cpp_srcs}")
set_property(TARGET foxglove_cpp_obj PROPERTY POSITION_INDEPENDENT_CODE True)
set_property(TARGET foxglove_cpp_obj PROPERTY CXX_STANDARD 17)
set_property(TARGET foxglove_cpp_obj PROPERTY CXX_STANDARD_REQUIRED True)
target_include_directories(foxglove_cpp_obj PRIVATE
  ${CMAKE_CURRENT_SOURCE_DIR}/foxglove/include
  ${CMAKE_CURRENT_SOURCE_DIR}/../c/include
)
target_compile_options(foxglove_cpp_obj PRIVATE ${SANITIZER_COMPILE_OPTIONS} ${STRICT_COMPILE_OPTIONS})
target_link_options(foxglove_cpp_obj PRIVATE ${SANITIZER_LINK_OPTIONS})

if(NOT FOXGLOVE_REMOTE_ACCESS)
  add_library(foxglove_cpp_static STATIC
    $<TARGET_OBJECTS:foxglove_cpp_obj>
    $<TARGET_OBJECTS:foxglove_generated_obj>
  )
  target_link_libraries(foxglove_cpp_static PRIVATE foxglove-static)
endif()

add_library(foxglove_cpp_shared SHARED
  $<TARGET_OBJECTS:foxglove_cpp_obj>
  $<TARGET_OBJECTS:foxglove_generated_obj>
)
# Export all symbols so MSVC generates the import library needed for linking
# against the DLL. This is simpler than annotating every public function with
# __declspec(dllexport), and the surface area is small (just our thin C++
# wrapper, not the Rust lib or third-party deps). FLE-382 tracks the work to
# switch to __declspec(dllexport), if we decide to go down that path.
set_target_properties(foxglove_cpp_shared PROPERTIES WINDOWS_EXPORT_ALL_SYMBOLS ON)
target_link_libraries(foxglove_cpp_shared PUBLIC foxglove-shared)
if(APPLE)
  set_target_properties(foxglove_cpp_shared PROPERTIES INSTALL_RPATH "@loader_path")
else()
  set_target_properties(foxglove_cpp_shared PROPERTIES INSTALL_RPATH "$ORIGIN")
endif()

set(_foxglove_cpp_targets foxglove_cpp_shared)
if(NOT FOXGLOVE_REMOTE_ACCESS)
  list(APPEND _foxglove_cpp_targets foxglove_cpp_static)
endif()
foreach(_target ${_foxglove_cpp_targets})
  target_include_directories(${_target} PUBLIC
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/foxglove/include>
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/../c/include>
    $<INSTALL_INTERFACE:include>
  )
  target_compile_options(${_target} PRIVATE ${SANITIZER_COMPILE_OPTIONS} ${STRICT_COMPILE_OPTIONS})
  target_link_options(${_target} PRIVATE ${SANITIZER_LINK_OPTIONS})
endforeach()


### Remote Access (optional)

if(FOXGLOVE_REMOTE_ACCESS)
  add_library(foxglove_ra_cpp_obj OBJECT
    "foxglove/include/foxglove/remote_access.hpp"
    "foxglove/src/remote_access.cpp"
  )
  set_property(TARGET foxglove_ra_cpp_obj PROPERTY POSITION_INDEPENDENT_CODE True)
  set_property(TARGET foxglove_ra_cpp_obj PROPERTY CXX_STANDARD 17)
  set_property(TARGET foxglove_ra_cpp_obj PROPERTY CXX_STANDARD_REQUIRED True)
  target_include_directories(foxglove_ra_cpp_obj PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}/foxglove/include
    ${CMAKE_CURRENT_SOURCE_DIR}/../c/include
  )
  target_compile_options(foxglove_ra_cpp_obj PRIVATE ${SANITIZER_COMPILE_OPTIONS} ${STRICT_COMPILE_OPTIONS})
  target_link_options(foxglove_ra_cpp_obj PRIVATE ${SANITIZER_LINK_OPTIONS})

  # Augment the shared library with RA objects
  target_sources(foxglove_cpp_shared PRIVATE $<TARGET_OBJECTS:foxglove_ra_cpp_obj>)
  target_compile_definitions(foxglove_cpp_shared PUBLIC FOXGLOVE_REMOTE_ACCESS)

endif()


### Tests

file(GLOB_RECURSE foxglove_test_srcs CONFIGURE_DEPENDS
    "foxglove/tests/*.hpp"
    "foxglove/tests/*.cpp"
)
add_executable(tests "${foxglove_test_srcs}")
set_property(TARGET tests PROPERTY CXX_STANDARD 17)
set_property(TARGET tests PROPERTY CXX_STANDARD_REQUIRED True)
target_compile_options(tests PUBLIC ${SANITIZER_COMPILE_OPTIONS})
target_link_options(tests PUBLIC ${SANITIZER_LINK_OPTIONS})
target_link_libraries(tests PRIVATE foxglove_cpp_shared Catch2::Catch2WithMain nlohmann_json::nlohmann_json websockets)
# Suppress warnings from third-party headers by treating them as system includes.
# SYSTEM on FetchContent_Declare requires CMake 3.25+, so we add system includes manually.
get_target_property(LWS_INCLUDE_DIRS websockets INTERFACE_INCLUDE_DIRECTORIES)
if(LWS_INCLUDE_DIRS)
  target_include_directories(tests SYSTEM PRIVATE ${LWS_INCLUDE_DIRS})
endif()
get_target_property(NLOHMANN_INCLUDE_DIRS nlohmann_json::nlohmann_json INTERFACE_INCLUDE_DIRECTORIES)
if(NLOHMANN_INCLUDE_DIRS)
  target_include_directories(tests SYSTEM PRIVATE ${NLOHMANN_INCLUDE_DIRS})
endif()
# Skip test discovery when cross-compiling (the test exe can't run on the host).
# FOXGLOVE_SKIP_TEST_DISCOVERY is needed for MSVC -A ARM64 which doesn't set CMAKE_CROSSCOMPILING.
if(NOT CMAKE_CROSSCOMPILING AND NOT FOXGLOVE_SKIP_TEST_DISCOVERY)
  catch_discover_tests(tests)
endif()
if(MSVC)
  # Auto-detection of C++11 support is not working on the windows-2025 runner with default settings
  target_compile_options(tests PRIVATE /Zc:__cplusplus)
endif()

### Docs

add_custom_target(doxygen
  COMMAND doxygen Doxyfile
  WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)

add_custom_target(html-docs
  COMMAND uv run sphinx-build --fail-on-warning . ${CMAKE_CURRENT_BINARY_DIR}/docs/html
  WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/foxglove/docs
)

### Examples

# Helper to define an example target with common boilerplate.
# Extra arguments after the source file(s) are passed to target_link_libraries.
function(add_foxglove_example name)
  cmake_parse_arguments(PARSE_ARGV 1 ARG "" "" "SOURCES;LIBS")
  add_executable(${name} ${ARG_SOURCES})
  set_target_properties(${name} PROPERTIES CXX_STANDARD 17 CXX_STANDARD_REQUIRED True)
  target_compile_options(${name} PRIVATE ${SANITIZER_COMPILE_OPTIONS} ${STRICT_COMPILE_OPTIONS})
  target_link_options(${name} PRIVATE ${SANITIZER_LINK_OPTIONS})
  target_link_libraries(${name} foxglove_cpp_shared ${ARG_LIBS})
  # Mark third-party include directories as SYSTEM to suppress warnings.
  # SYSTEM on target_link_libraries requires CMake 3.25+, so we add them manually.
  foreach(_lib ${ARG_LIBS})
    if(TARGET ${_lib})
      get_target_property(_inc ${_lib} INTERFACE_INCLUDE_DIRECTORIES)
      if(_inc)
        target_include_directories(${name} SYSTEM PRIVATE ${_inc})
      endif()
    endif()
  endforeach()
endfunction()

add_foxglove_example(example_server SOURCES examples/ws-server/src/main.cpp)
add_foxglove_example(example_services SOURCES examples/ws-services/src/main.cpp LIBS nlohmann_json::nlohmann_json)
add_foxglove_example(example_mcap SOURCES examples/mcap/src/main.cpp)
add_foxglove_example(example_foxglove_schemas SOURCES examples/foxglove-schemas/src/main.cpp)
add_foxglove_example(example_param_server SOURCES examples/param-server/src/main.cpp)
add_foxglove_example(example_quickstart SOURCES examples/quickstart/src/main.cpp)
add_foxglove_example(example_connection_graph SOURCES examples/connection-graph/src/main.cpp)
add_foxglove_example(example_auto_serialize SOURCES examples/auto-serialize/src/main.cpp LIBS nlohmann_json::nlohmann_json)
add_foxglove_example(example_message_filtering SOURCES examples/message-filtering/src/main.cpp)

if(FOXGLOVE_REMOTE_ACCESS)
  add_foxglove_example(example_remote_access SOURCES examples/remote-access/src/main.cpp)
  add_foxglove_example(example_remote_access_connection_graph SOURCES examples/remote-access-connection-graph/src/main.cpp)
  add_foxglove_example(example_remote_access_fetch_asset SOURCES examples/remote-access-fetch-asset/src/main.cpp)
endif()

option(BUILD_OPENCV_EXAMPLE "Build the RGB camera visualization example (requires OpenCV)" OFF)

if(BUILD_OPENCV_EXAMPLE)
  find_package(OpenCV REQUIRED COMPONENTS core videoio)
  add_foxglove_example(example_rgb_camera_visualization SOURCES examples/rgb-camera-visualization/main.cpp LIBS opencv_core opencv_videoio)
endif()

if(BUILD_REMOTE_DATA_LOADER_BACKEND_EXAMPLE)
  add_foxglove_example(example_remote_data_loader_backend SOURCES examples/remote-data-loader-backend/src/main.cpp LIBS nlohmann_json::nlohmann_json httplib::httplib base64 date::date)
endif()

if(BUILD_WS_STREAM_MCAP_EXAMPLE)
  add_foxglove_example(example_ws_stream_mcap
    SOURCES examples/ws-stream-mcap/src/main.cpp examples/ws-stream-mcap/src/mcap_player.cpp
    LIBS lz4_static
  )
  target_compile_definitions(example_ws_stream_mcap PRIVATE MCAP_COMPRESSION_NO_ZSTD)
  target_include_directories(example_ws_stream_mcap SYSTEM PRIVATE ${mcap_SOURCE_DIR}/cpp/mcap/include)
endif()

### Install

include(GNUInstallDirs)
include(CMakePackageConfigHelpers)

# Install C++ libraries
if(FOXGLOVE_REMOTE_ACCESS)
  install(TARGETS foxglove_cpp_shared
    EXPORT foxglove-sdkTargets
    LIBRARY DESTINATION ${CMAKE_INSTALL_LIBDIR}
  )
else()
  install(TARGETS foxglove_cpp_static foxglove_cpp_shared
    EXPORT foxglove-sdkTargets
    ARCHIVE DESTINATION ${CMAKE_INSTALL_LIBDIR}
    LIBRARY DESTINATION ${CMAKE_INSTALL_LIBDIR}
  )
  # Install the Rust/C static library
  install(FILES $<TARGET_FILE:foxglove-static>
    DESTINATION ${CMAKE_INSTALL_LIBDIR}
  )
endif()

# Install the Rust/C shared library
install(FILES $<TARGET_FILE:foxglove-shared>
  DESTINATION ${CMAKE_INSTALL_LIBDIR}
)

# Install C++ headers
install(DIRECTORY foxglove/include/
  DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}
  FILES_MATCHING PATTERN "*.hpp"
)

# Install C headers
install(DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/../c/include/
  DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}
  FILES_MATCHING PATTERN "*.h"
)

# Generate and install CMake package config files
configure_package_config_file(
  "${CMAKE_CURRENT_SOURCE_DIR}/cmake/foxglove-sdkConfig.cmake.in"
  "${CMAKE_CURRENT_BINARY_DIR}/foxglove-sdkConfig.cmake"
  INSTALL_DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/foxglove-sdk
)

write_basic_package_version_file(
  "${CMAKE_CURRENT_BINARY_DIR}/foxglove-sdkConfigVersion.cmake"
  VERSION ${PROJECT_VERSION}
  COMPATIBILITY SameMinorVersion
)

install(EXPORT foxglove-sdkTargets
  FILE foxglove-sdkTargets.cmake
  NAMESPACE foxglove-sdk::
  DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/foxglove-sdk
)

install(FILES
  "${CMAKE_CURRENT_BINARY_DIR}/foxglove-sdkConfig.cmake"
  "${CMAKE_CURRENT_BINARY_DIR}/foxglove-sdkConfigVersion.cmake"
  DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/foxglove-sdk
)

