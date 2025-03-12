# This CMake file is used for CI, examples, and tests. It builds the whole project from scratch
# including the Rust code. If you would like to use the C++ library only, you only need to compile
# the sources under `cpp/foxglove`, link with the C static or shared library, and add both the C/C++
# includes to include paths.

cmake_minimum_required(VERSION 3.25)
project(foxglove-sdk)

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
FetchContent_MakeAvailable(Corrosion)


FetchContent_Declare(
  websocketpp
  GIT_REPOSITORY https://github.com/zaphoyd/websocketpp.git
  GIT_TAG 0.8.2
)
FetchContent_MakeAvailable(websocketpp)

FetchContent_Declare(asio
  GIT_REPOSITORY https://github.com/chriskohlhoff/asio.git
  GIT_TAG asio-1-31-0
)
FetchContent_MakeAvailable(asio)

if(DEFINED SANITIZE)
  if(NOT CMAKE_CXX_COMPILER_ID STREQUAL "Clang")
    message(FATAL_ERROR "Sanitizers are only supported with Clang")
  endif()
  set(SANITIZER_COMPILE_OPTIONS -fsanitize=${SANITIZE} -fsanitize-ignorelist=${CMAKE_CURRENT_SOURCE_DIR}/sanitize-ignorelist.txt -fno-omit-frame-pointer -fno-sanitize-recover=all)
  set(SANITIZER_LINK_OPTIONS -fsanitize=${SANITIZE})
  set(SANITIZER_CARGO_FLAGS -Zbuild-std)
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
  target_link_libraries(foxglove-static INTERFACE Bcrypt)
endif()


### C++

file(GLOB_RECURSE foxglove_cpp_srcs CONFIGURE_DEPENDS
    "foxglove/include/*.hpp"
    "foxglove/src/*.hpp"
    "foxglove/src/*.cpp"
)

add_library(foxglove_cpp_obj OBJECT "${foxglove_cpp_srcs}")
set_property(TARGET foxglove_cpp_obj PROPERTY CXX_STANDARD 17)
set_property(TARGET foxglove_cpp_obj PROPERTY CXX_STANDARD_REQUIRED True)
set_property(TARGET foxglove_cpp_obj PROPERTY POSITION_INDEPENDENT_CODE True)
target_include_directories(foxglove_cpp_obj PUBLIC foxglove/include ${CMAKE_CURRENT_SOURCE_DIR}/../c/include)
target_compile_options(foxglove_cpp_obj PUBLIC ${SANITIZER_COMPILE_OPTIONS})
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
target_compile_definitions(tests PRIVATE ASIO_STANDALONE)
target_compile_options(tests PUBLIC ${SANITIZER_COMPILE_OPTIONS})
target_link_options(tests PUBLIC ${SANITIZER_LINK_OPTIONS})
target_include_directories(tests SYSTEM PRIVATE ${asio_SOURCE_DIR}/asio/include)
target_include_directories(tests SYSTEM PRIVATE ${websocketpp_SOURCE_DIR})
set_property(TARGET tests PROPERTY CXX_STANDARD 17)
set_property(TARGET tests PROPERTY CXX_STANDARD_REQUIRED True)
target_link_libraries(tests PRIVATE foxglove_cpp_static Catch2::Catch2WithMain)
catch_discover_tests(tests)
if(MSVC)
  # Auto-detection of C++11 support is not working on the windows-2025 runner with default settings
  target_compile_options(tests PRIVATE /Zc:__cplusplus)
endif()

### Examples

add_executable(example_server examples/ws-server/src/main.cpp)
target_compile_options(example_server PUBLIC ${SANITIZER_COMPILE_OPTIONS})
target_link_options(example_server PUBLIC ${SANITIZER_LINK_OPTIONS})
set_property(TARGET example_server PROPERTY CXX_STANDARD 17)
set_property(TARGET example_server PROPERTY CXX_STANDARD_REQUIRED True)
target_link_libraries(example_server foxglove_cpp_static)
