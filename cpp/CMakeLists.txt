# This CMake file is used for CI, examples, and tests. It builds the whole project from scratch
# including the Rust code. If you would like to use the C++ library only, you only need to compile
# the sources under `cpp/foxglove`, link with the C static or shared library, and add both the C/C++
# includes to include paths.

cmake_minimum_required(VERSION 3.25)
project(foxglove-sdk)

# Enable ccache if available
find_program(CCACHE_PROGRAM ccache)
if(CCACHE_PROGRAM)
  set(CMAKE_C_COMPILER_LAUNCHER "${CCACHE_PROGRAM}")
  set(CMAKE_CXX_COMPILER_LAUNCHER "${CCACHE_PROGRAM}")
  message(STATUS "Using ccache: ${CCACHE_PROGRAM}")
endif()

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED True)

include(CTest)
include(FetchContent)

FetchContent_Declare(
  Catch2
  GIT_REPOSITORY https://github.com/catchorg/Catch2.git
  GIT_TAG        v3.8.0
)
FetchContent_MakeAvailable(Catch2)
list(APPEND CMAKE_MODULE_PATH ${catch2_SOURCE_DIR}/extras)
include(Catch)

FetchContent_Declare(
  Corrosion
  GIT_REPOSITORY https://github.com/corrosion-rs/corrosion.git
  GIT_TAG v0.5.1
)
# Clear CMAKE_CONFIGURATION_TYPES before making Corrosion available
if(CMAKE_GENERATOR MATCHES "Unix Makefiles|Ninja")
  unset(CMAKE_CONFIGURATION_TYPES CACHE)
endif()
FetchContent_MakeAvailable(Corrosion)

FetchContent_Declare(
  websocketpp
  GIT_REPOSITORY https://github.com/zaphoyd/websocketpp.git
  GIT_TAG 0.8.2
  OVERRIDE_FIND_PACKAGE
)
# Set WebSocketPP CMake policy to suppress the warning
set(FETCHCONTENT_TRY_FIND_PACKAGE_MODE NEVER)
FetchContent_GetProperties(websocketpp)
if(NOT websocketpp_POPULATED)
  FetchContent_Populate(websocketpp)
  # Don't call add_subdirectory to avoid running the original CMakeLists.txt
  # Just set up the include directory
  add_library(websocketpp INTERFACE)
  target_include_directories(websocketpp INTERFACE ${websocketpp_SOURCE_DIR})
endif()
FetchContent_MakeAvailable(websocketpp)

FetchContent_Declare(asio
  GIT_REPOSITORY https://github.com/chriskohlhoff/asio.git
  GIT_TAG asio-1-31-0
)
FetchContent_MakeAvailable(asio)

FetchContent_Declare(json
  GIT_REPOSITORY https://github.com/nlohmann/json.git
  GIT_TAG v3.12.0
)
FetchContent_MakeAvailable(json)

FetchContent_Declare(base64
  GIT_REPOSITORY https://github.com/tobiaslocker/base64.git
  GIT_TAG 8d96a2a737ac1396304b1de289beb3a5ea0cb752
)
set(BASE64_ENABLE_TESTING OFF CACHE BOOL "" FORCE)
FetchContent_MakeAvailable(base64)

option(BUILD_DATA_PROVIDER_EXAMPLE "Build the data provider example (requires cpp-httplib and date)" ON)

if(BUILD_DATA_PROVIDER_EXAMPLE)
  FetchContent_Declare(date
    GIT_REPOSITORY https://github.com/HowardHinnant/date.git
    GIT_TAG v3.0.4
  )
  set(ENABLE_DATE_TESTING OFF CACHE BOOL "" FORCE)
  FetchContent_MakeAvailable(date)

  FetchContent_Declare(httplib
    GIT_REPOSITORY https://github.com/yhirose/cpp-httplib.git
    GIT_TAG v0.32.0
  )
  set(HTTPLIB_COMPILE OFF CACHE BOOL "" FORCE)
  set(HTTPLIB_REQUIRE_OPENSSL OFF CACHE BOOL "" FORCE)
  set(HTTPLIB_USE_OPENSSL_IF_AVAILABLE OFF CACHE BOOL "" FORCE)
  set(HTTPLIB_REQUIRE_ZLIB OFF CACHE BOOL "" FORCE)
  set(HTTPLIB_USE_ZLIB_IF_AVAILABLE OFF CACHE BOOL "" FORCE)
  set(HTTPLIB_USE_BROTLI_IF_AVAILABLE OFF CACHE BOOL "" FORCE)
  set(HTTPLIB_USE_ZSTD_IF_AVAILABLE OFF CACHE BOOL "" FORCE)
  set(HTTPLIB_USE_NON_BLOCKING_GETADDRINFO OFF CACHE BOOL "" FORCE)
  FetchContent_MakeAvailable(httplib)
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

if(DEFINED CLANG_TIDY)
  set(CMAKE_CXX_CLANG_TIDY clang-tidy)
endif()

### C

corrosion_import_crate(
  MANIFEST_PATH ../c/Cargo.toml
  CRATES foxglove_c
  FLAGS ${SANITIZER_CARGO_FLAGS}
)
if(DEFINED SANITIZE)
  # Rust does not have flags for certain sanitizers so we strip them out
  string(REGEX REPLACE undefined "" rust_sanitizers ${SANITIZE})
  string(REGEX REPLACE ,, , rust_sanitizers ${rust_sanitizers})
  string(REGEX REPLACE ,$ "" rust_sanitizers ${rust_sanitizers})
  message(WARNING "Enabling -Zsanitizer=${rust_sanitizers} for Rust")
  corrosion_add_target_rustflags(foxglove -Zsanitizer=${rust_sanitizers} -Cforce-frame-pointers=yes)
endif()

if(WIN32)
  target_link_libraries(foxglove-static INTERFACE Bcrypt SChannel Crypt32 Ncrypt)
elseif(APPLE)
  target_link_libraries(foxglove-static INTERFACE "-framework Security" "-framework CoreFoundation")
endif()


### C++

file(GLOB_RECURSE foxglove_cpp_srcs CONFIGURE_DEPENDS
    "foxglove/include/*.hpp"
    "foxglove/src/*.hpp"
    "foxglove/src/*.cpp"
)

add_library(foxglove_cpp_obj OBJECT "${foxglove_cpp_srcs}")
set_property(TARGET foxglove_cpp_obj PROPERTY POSITION_INDEPENDENT_CODE True)
set_property(TARGET foxglove_cpp_obj PROPERTY CXX_STANDARD 17)
set_property(TARGET foxglove_cpp_obj PROPERTY CXX_STANDARD_REQUIRED True)
target_include_directories(foxglove_cpp_obj PUBLIC foxglove/include ${CMAKE_CURRENT_SOURCE_DIR}/../c/include)
target_compile_options(foxglove_cpp_obj PUBLIC ${SANITIZER_COMPILE_OPTIONS} ${STRICT_COMPILE_OPTIONS})
target_link_options(foxglove_cpp_obj PUBLIC ${SANITIZER_LINK_OPTIONS})

add_library(foxglove_cpp_static STATIC)
add_library(foxglove_cpp_shared SHARED)
target_link_libraries(foxglove_cpp_static PUBLIC foxglove_cpp_obj)
target_link_libraries(foxglove_cpp_static PRIVATE foxglove-static)
target_link_libraries(foxglove_cpp_shared PUBLIC foxglove_cpp_obj)
target_link_libraries(foxglove_cpp_shared PRIVATE foxglove-shared)


### Tests

file(GLOB_RECURSE foxglove_test_srcs CONFIGURE_DEPENDS
    "foxglove/tests/*.hpp"
    "foxglove/tests/*.cpp"
)
add_executable(tests "${foxglove_test_srcs}")
set_property(TARGET tests PROPERTY CXX_STANDARD 17)
set_property(TARGET tests PROPERTY CXX_STANDARD_REQUIRED True)
target_compile_definitions(tests PRIVATE ASIO_STANDALONE)
target_compile_options(tests PUBLIC ${SANITIZER_COMPILE_OPTIONS})
target_link_options(tests PUBLIC ${SANITIZER_LINK_OPTIONS})
target_include_directories(tests SYSTEM PRIVATE ${asio_SOURCE_DIR}/asio/include)
target_include_directories(tests SYSTEM PRIVATE ${json_SOURCE_DIR}/include)
target_include_directories(tests SYSTEM PRIVATE ${websocketpp_SOURCE_DIR})
target_link_libraries(tests PRIVATE foxglove_cpp_static Catch2::Catch2WithMain)
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
target_include_directories(example_services SYSTEM PRIVATE ${json_SOURCE_DIR}/include)
target_link_options(example_services PUBLIC ${SANITIZER_LINK_OPTIONS})
target_link_libraries(example_services foxglove_cpp_static)

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
target_include_directories(example_auto_serialize SYSTEM PRIVATE ${json_SOURCE_DIR}/include)
target_link_options(example_auto_serialize PUBLIC ${SANITIZER_LINK_OPTIONS})
target_link_libraries(example_auto_serialize foxglove_cpp_static)

option(BUILD_OPENCV_EXAMPLE "Build the RGB camera visualization example (requires OpenCV)" OFF)

if(BUILD_OPENCV_EXAMPLE)
  find_package(OpenCV REQUIRED)
  include_directories(${OpenCV_INCLUDE_DIRS})
  add_executable(example_rgb_camera_visualization examples/rgb-camera-visualization/main.cpp)
  set_property(TARGET example_rgb_camera_visualization PROPERTY EXCLUDE_FROM_ALL TRUE)
  set_property(TARGET example_rgb_camera_visualization PROPERTY CXX_STANDARD 17)
  set_property(TARGET example_rgb_camera_visualization PROPERTY CXX_STANDARD_REQUIRED True)
  target_compile_options(example_rgb_camera_visualization PUBLIC ${SANITIZER_COMPILE_OPTIONS} ${STRICT_COMPILE_OPTIONS})
  target_link_options(example_rgb_camera_visualization PUBLIC ${SANITIZER_LINK_OPTIONS})
  target_include_directories(example_rgb_camera_visualization PRIVATE ${OpenCV_INCLUDE_DIRS})
  target_link_libraries(example_rgb_camera_visualization foxglove_cpp_static ${OpenCV_LIBS})
endif()

add_executable(example_message_filtering examples/message-filtering/src/main.cpp)
set_property(TARGET example_message_filtering PROPERTY CXX_STANDARD 17)
set_property(TARGET example_message_filtering PROPERTY CXX_STANDARD_REQUIRED True)
target_compile_options(example_message_filtering PUBLIC ${SANITIZER_COMPILE_OPTIONS})
target_link_options(example_message_filtering PUBLIC ${SANITIZER_LINK_OPTIONS})
target_link_libraries(example_message_filtering foxglove_cpp_static)

if(BUILD_DATA_PROVIDER_EXAMPLE)
  add_executable(example_data_provider examples/data-provider/src/main.cpp)
  set_property(TARGET example_data_provider PROPERTY CXX_STANDARD 17)
  set_property(TARGET example_data_provider PROPERTY CXX_STANDARD_REQUIRED True)
  target_compile_options(example_data_provider PUBLIC ${SANITIZER_COMPILE_OPTIONS} ${STRICT_COMPILE_OPTIONS})
  target_include_directories(example_data_provider SYSTEM PRIVATE ${json_SOURCE_DIR}/include)
  target_link_options(example_data_provider PUBLIC ${SANITIZER_LINK_OPTIONS})
  target_link_libraries(example_data_provider foxglove_cpp_static httplib::httplib base64 date::date)
endif()
