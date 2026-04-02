# This CMake file is used for CI, examples, and tests. It builds the whole project from scratch
# including the Rust code. If you would like to use the C++ library only, you only need to compile
# the sources under `cpp/foxglove`, link with the C static or shared library, and add both the C/C++
# includes to include paths.

cmake_minimum_required(VERSION 3.25)
project(foxglove-sdk VERSION 0.21.0)

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
option(FOXGLOVE_BUILD_REMOTE_ACCESS "Build with remote access gateway support" OFF)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED True)

include(CTest)
include(FetchContent)

# When USE_PACKAGE_MANAGER_DEPENDENCIES is ON, FetchContent will try find_package() first for
# dependencies that specify FIND_PACKAGE_ARGS, falling back to source builds if not found.
# When OFF, FetchContent always builds from source.
if(USE_PACKAGE_MANAGER_DEPENDENCIES)
  set(FETCHCONTENT_TRY_FIND_PACKAGE_MODE OPT_IN)
else()
  set(FETCHCONTENT_TRY_FIND_PACKAGE_MODE NEVER)
endif()

# Suppress CMake deprecation warnings from vendored dependencies with older cmake_minimum_required
set(CMAKE_POLICY_VERSION_MINIMUM 3.5)
set(CMAKE_WARN_DEPRECATED OFF CACHE BOOL "" FORCE)

FetchContent_Declare(Catch2
  GIT_REPOSITORY https://github.com/catchorg/Catch2.git
  GIT_TAG        v3.8.0
  FIND_PACKAGE_ARGS 3 QUIET
)
FetchContent_MakeAvailable(Catch2)
if(catch2_SOURCE_DIR)
  list(APPEND CMAKE_MODULE_PATH ${catch2_SOURCE_DIR}/extras)
endif()
include(Catch)

FetchContent_Declare(Corrosion
  GIT_REPOSITORY https://github.com/corrosion-rs/corrosion.git
  GIT_TAG v0.5.1
  FIND_PACKAGE_ARGS 0.5.1 QUIET
)
# Clear CMAKE_CONFIGURATION_TYPES before making Corrosion available
if(CMAKE_GENERATOR MATCHES "Unix Makefiles|Ninja")
  unset(CMAKE_CONFIGURATION_TYPES CACHE)
endif()
FetchContent_MakeAvailable(Corrosion)

FetchContent_Declare(libwebsockets
  GIT_REPOSITORY https://libwebsockets.org/repo/libwebsockets
  GIT_TAG v4.3.3
  FIND_PACKAGE_ARGS QUIET
)
set(LWS_WITH_SSL OFF CACHE BOOL "" FORCE)
set(LWS_WITH_SHARED OFF CACHE BOOL "" FORCE)
set(LWS_WITH_STATIC ON CACHE BOOL "" FORCE)
set(LWS_WITHOUT_TESTAPPS ON CACHE BOOL "" FORCE)
set(LWS_WITHOUT_TEST_SERVER ON CACHE BOOL "" FORCE)
set(LWS_WITHOUT_TEST_PING ON CACHE BOOL "" FORCE)
set(LWS_WITHOUT_TEST_CLIENT ON CACHE BOOL "" FORCE)
# Avoid FetchContent_MakeAvailable so we can pass EXCLUDE_FROM_ALL to add_subdirectory,
# preventing libwebsockets' install rules from polluting our install.
if(USE_PACKAGE_MANAGER_DEPENDENCIES)
  find_package(libwebsockets QUIET)
endif()
if(NOT libwebsockets_FOUND)
  FetchContent_Populate(libwebsockets)
  add_subdirectory(${libwebsockets_SOURCE_DIR} ${libwebsockets_BINARY_DIR} EXCLUDE_FROM_ALL)
endif()
# System-installed libwebsockets provides the "websockets_shared" target, while the FetchContent
# build provides "websockets". Create an alias so the rest of this file can use "websockets"
# consistently.
if(NOT TARGET websockets AND TARGET websockets_shared)
  add_library(websockets ALIAS websockets_shared)
endif()

FetchContent_Declare(nlohmann_json
  GIT_REPOSITORY https://github.com/nlohmann/json.git
  GIT_TAG v3.12.0
  FIND_PACKAGE_ARGS QUIET
)
FetchContent_MakeAvailable(nlohmann_json)

FetchContent_Declare(base64
  GIT_REPOSITORY https://github.com/tobiaslocker/base64.git
  GIT_TAG 8d96a2a737ac1396304b1de289beb3a5ea0cb752
  FIND_PACKAGE_ARGS QUIET
)
set(BASE64_ENABLE_TESTING OFF CACHE BOOL "" FORCE)
FetchContent_MakeAvailable(base64)

option(BUILD_REMOTE_DATA_LOADER_BACKEND_EXAMPLE "Build the remote data loader backend example (requires cpp-httplib and date)" ON)

if(BUILD_REMOTE_DATA_LOADER_BACKEND_EXAMPLE)
  FetchContent_Declare(date
    GIT_REPOSITORY https://github.com/HowardHinnant/date.git
    GIT_TAG v3.0.4
    FIND_PACKAGE_ARGS QUIET
  )
  set(ENABLE_DATE_TESTING OFF CACHE BOOL "" FORCE)
  set(ENABLE_DATE_INSTALL OFF CACHE BOOL "" FORCE)
  FetchContent_MakeAvailable(date)

  FetchContent_Declare(httplib
    GIT_REPOSITORY https://github.com/yhirose/cpp-httplib.git
    GIT_TAG v0.32.0
    FIND_PACKAGE_ARGS QUIET
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
  FetchContent_MakeAvailable(httplib)
endif()

set(CMAKE_WARN_DEPRECATED ON CACHE BOOL "" FORCE)

option(BUILD_WS_STREAM_MCAP_EXAMPLE "Build the MCAP websocket streaming example (requires mcap and lz4)" ON)

if(BUILD_WS_STREAM_MCAP_EXAMPLE)
  FetchContent_Declare(
    mcap
    GIT_REPOSITORY https://github.com/foxglove/mcap.git
    GIT_TAG releases/cpp/v2.1.2
  )
  FetchContent_MakeAvailable(mcap)

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

option(CLANG_TIDY "Run clang-tidy static analysis" OFF)
if(CLANG_TIDY)
  find_program(CLANG_TIDY_EXE NAMES clang-tidy REQUIRED)
  message(STATUS "Found clang-tidy: ${CLANG_TIDY_EXE}")
  set(CMAKE_CXX_CLANG_TIDY "${CLANG_TIDY_EXE}")
endif()

### C

if(FOXGLOVE_BUILD_REMOTE_ACCESS)
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

if(NOT FOXGLOVE_BUILD_REMOTE_ACCESS)
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

if(NOT FOXGLOVE_BUILD_REMOTE_ACCESS)
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
target_link_libraries(foxglove_cpp_shared PUBLIC foxglove-shared)
if(APPLE)
  set_target_properties(foxglove_cpp_shared PROPERTIES INSTALL_RPATH "@loader_path")
else()
  set_target_properties(foxglove_cpp_shared PROPERTIES INSTALL_RPATH "$ORIGIN")
endif()

set(_foxglove_cpp_targets foxglove_cpp_shared)
if(NOT FOXGLOVE_BUILD_REMOTE_ACCESS)
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

if(FOXGLOVE_BUILD_REMOTE_ACCESS)
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

  add_executable(example_remote_access examples/remote-access/src/main.cpp)
  set_property(TARGET example_remote_access PROPERTY CXX_STANDARD 17)
  set_property(TARGET example_remote_access PROPERTY CXX_STANDARD_REQUIRED True)
  target_compile_options(example_remote_access PUBLIC ${SANITIZER_COMPILE_OPTIONS} ${STRICT_COMPILE_OPTIONS})
  target_link_options(example_remote_access PUBLIC ${SANITIZER_LINK_OPTIONS})
  target_link_libraries(example_remote_access foxglove_cpp_shared)
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
if(FOXGLOVE_BUILD_REMOTE_ACCESS)
  target_link_libraries(tests PRIVATE foxglove_cpp_shared Catch2::Catch2WithMain nlohmann_json::nlohmann_json websockets)
else()
  target_link_libraries(tests PRIVATE foxglove_cpp_static Catch2::Catch2WithMain nlohmann_json::nlohmann_json websockets)
endif()
# Suppress warnings from third-party headers by treating them as system includes.
# We can't use SYSTEM on FetchContent_Declare due to a CMake 3.25 bug.
get_target_property(LWS_INCLUDE_DIRS websockets INTERFACE_INCLUDE_DIRECTORIES)
target_include_directories(tests SYSTEM PRIVATE ${LWS_INCLUDE_DIRS})
get_target_property(NLOHMANN_INCLUDE_DIRS nlohmann_json::nlohmann_json INTERFACE_INCLUDE_DIRECTORIES)
target_include_directories(tests SYSTEM PRIVATE ${NLOHMANN_INCLUDE_DIRS})
catch_discover_tests(tests)
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

add_executable(example_server examples/ws-server/src/main.cpp)
set_property(TARGET example_server PROPERTY CXX_STANDARD 17)
set_property(TARGET example_server PROPERTY CXX_STANDARD_REQUIRED True)
target_compile_options(example_server PUBLIC ${SANITIZER_COMPILE_OPTIONS} ${STRICT_COMPILE_OPTIONS})
target_link_options(example_server PUBLIC ${SANITIZER_LINK_OPTIONS})
target_link_libraries(example_server foxglove_cpp_static)

add_executable(example_services examples/ws-services/src/main.cpp)
set_property(TARGET example_services PROPERTY CXX_STANDARD 17)
set_property(TARGET example_services PROPERTY CXX_STANDARD_REQUIRED True)
target_compile_options(example_services PUBLIC ${SANITIZER_COMPILE_OPTIONS} ${STRICT_COMPILE_OPTIONS})
target_link_options(example_services PUBLIC ${SANITIZER_LINK_OPTIONS})
target_link_libraries(example_services foxglove_cpp_static nlohmann_json::nlohmann_json)
target_include_directories(example_services SYSTEM PRIVATE ${NLOHMANN_INCLUDE_DIRS})

add_executable(example_mcap examples/mcap/src/main.cpp)
set_property(TARGET example_mcap PROPERTY CXX_STANDARD 17)
set_property(TARGET example_mcap PROPERTY CXX_STANDARD_REQUIRED True)
target_compile_options(example_mcap PUBLIC ${SANITIZER_COMPILE_OPTIONS} ${STRICT_COMPILE_OPTIONS})
target_link_options(example_mcap PUBLIC ${SANITIZER_LINK_OPTIONS})
target_link_libraries(example_mcap foxglove_cpp_static)

add_executable(example_foxglove_schemas examples/foxglove-schemas/src/main.cpp)
set_property(TARGET example_foxglove_schemas PROPERTY CXX_STANDARD 17)
set_property(TARGET example_foxglove_schemas PROPERTY CXX_STANDARD_REQUIRED True)
target_compile_options(example_foxglove_schemas PUBLIC ${SANITIZER_COMPILE_OPTIONS} ${STRICT_COMPILE_OPTIONS})
target_link_options(example_foxglove_schemas PUBLIC ${SANITIZER_LINK_OPTIONS})
target_link_libraries(example_foxglove_schemas foxglove_cpp_static)

add_executable(example_param_server examples/param-server/src/main.cpp)
set_property(TARGET example_param_server PROPERTY CXX_STANDARD 17)
set_property(TARGET example_param_server PROPERTY CXX_STANDARD_REQUIRED True)
target_compile_options(example_param_server PUBLIC ${SANITIZER_COMPILE_OPTIONS} ${STRICT_COMPILE_OPTIONS})
target_link_options(example_param_server PUBLIC ${SANITIZER_LINK_OPTIONS})
target_link_libraries(example_param_server foxglove_cpp_static)

add_executable(example_quickstart examples/quickstart/src/main.cpp)
set_property(TARGET example_quickstart PROPERTY CXX_STANDARD 17)
set_property(TARGET example_quickstart PROPERTY CXX_STANDARD_REQUIRED True)
target_compile_options(example_quickstart PUBLIC ${SANITIZER_COMPILE_OPTIONS} ${STRICT_COMPILE_OPTIONS})
target_link_options(example_quickstart PUBLIC ${SANITIZER_LINK_OPTIONS})
target_link_libraries(example_quickstart foxglove_cpp_static)

add_executable(example_connection_graph examples/connection-graph/src/main.cpp)
set_property(TARGET example_connection_graph PROPERTY CXX_STANDARD 17)
set_property(TARGET example_connection_graph PROPERTY CXX_STANDARD_REQUIRED True)
target_compile_options(example_connection_graph PUBLIC ${SANITIZER_COMPILE_OPTIONS} ${STRICT_COMPILE_OPTIONS})
target_link_options(example_connection_graph PUBLIC ${SANITIZER_LINK_OPTIONS})
target_link_libraries(example_connection_graph foxglove_cpp_static)

add_executable(example_auto_serialize examples/auto-serialize/src/main.cpp)
set_property(TARGET example_auto_serialize PROPERTY CXX_STANDARD 17)
set_property(TARGET example_auto_serialize PROPERTY CXX_STANDARD_REQUIRED True)
target_compile_options(example_auto_serialize PUBLIC ${SANITIZER_COMPILE_OPTIONS} ${STRICT_COMPILE_OPTIONS})
target_link_options(example_auto_serialize PUBLIC ${SANITIZER_LINK_OPTIONS})
target_link_libraries(example_auto_serialize foxglove_cpp_static nlohmann_json::nlohmann_json)
target_include_directories(example_auto_serialize SYSTEM PRIVATE ${NLOHMANN_INCLUDE_DIRS})

option(BUILD_OPENCV_EXAMPLE "Build the RGB camera visualization example (requires OpenCV)" OFF)

if(BUILD_OPENCV_EXAMPLE)
  find_package(OpenCV REQUIRED COMPONENTS core videoio)
  add_executable(example_rgb_camera_visualization examples/rgb-camera-visualization/main.cpp)
  set_property(TARGET example_rgb_camera_visualization PROPERTY CXX_STANDARD 17)
  set_property(TARGET example_rgb_camera_visualization PROPERTY CXX_STANDARD_REQUIRED True)
  target_compile_options(example_rgb_camera_visualization PUBLIC ${SANITIZER_COMPILE_OPTIONS} ${STRICT_COMPILE_OPTIONS})
  target_link_options(example_rgb_camera_visualization PUBLIC ${SANITIZER_LINK_OPTIONS})
  target_link_libraries(example_rgb_camera_visualization foxglove_cpp_static opencv_core opencv_videoio)
endif()

add_executable(example_message_filtering examples/message-filtering/src/main.cpp)
set_property(TARGET example_message_filtering PROPERTY CXX_STANDARD 17)
set_property(TARGET example_message_filtering PROPERTY CXX_STANDARD_REQUIRED True)
target_compile_options(example_message_filtering PUBLIC ${SANITIZER_COMPILE_OPTIONS})
target_link_options(example_message_filtering PUBLIC ${SANITIZER_LINK_OPTIONS})
target_link_libraries(example_message_filtering foxglove_cpp_static)

if(BUILD_REMOTE_DATA_LOADER_BACKEND_EXAMPLE)
  add_executable(example_remote_data_loader_backend examples/remote-data-loader-backend/src/main.cpp)
  set_property(TARGET example_remote_data_loader_backend PROPERTY CXX_STANDARD 17)
  set_property(TARGET example_remote_data_loader_backend PROPERTY CXX_STANDARD_REQUIRED True)
  target_compile_options(example_remote_data_loader_backend PUBLIC ${SANITIZER_COMPILE_OPTIONS} ${STRICT_COMPILE_OPTIONS})
  target_link_options(example_remote_data_loader_backend PUBLIC ${SANITIZER_LINK_OPTIONS})
  target_link_libraries(example_remote_data_loader_backend foxglove_cpp_static nlohmann_json::nlohmann_json httplib::httplib base64 date::date)
  target_include_directories(example_remote_data_loader_backend SYSTEM PRIVATE ${NLOHMANN_INCLUDE_DIRS})
endif()

if(BUILD_WS_STREAM_MCAP_EXAMPLE)
  add_executable(example_ws_stream_mcap
    examples/ws-stream-mcap/src/main.cpp
    examples/ws-stream-mcap/src/mcap_player.cpp
  )
  set_property(TARGET example_ws_stream_mcap PROPERTY CXX_STANDARD 17)
  set_property(TARGET example_ws_stream_mcap PROPERTY CXX_STANDARD_REQUIRED True)
  target_compile_options(example_ws_stream_mcap PUBLIC ${SANITIZER_COMPILE_OPTIONS} ${STRICT_COMPILE_OPTIONS})
  target_link_options(example_ws_stream_mcap PUBLIC ${SANITIZER_LINK_OPTIONS})
  target_compile_definitions(example_ws_stream_mcap PRIVATE MCAP_COMPRESSION_NO_ZSTD)
  target_include_directories(example_ws_stream_mcap SYSTEM PRIVATE ${mcap_SOURCE_DIR}/cpp/mcap/include)
  target_link_libraries(example_ws_stream_mcap foxglove_cpp_static lz4_static)
endif()

### Install

include(GNUInstallDirs)
include(CMakePackageConfigHelpers)

# Install C++ libraries
if(FOXGLOVE_BUILD_REMOTE_ACCESS)
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

