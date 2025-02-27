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


### C

corrosion_import_crate(MANIFEST_PATH ../c/Cargo.toml CRATES foxglove_c)

if(WIN32)
  target_link_libraries(foxglove_c-static INTERFACE Bcrypt)
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
target_include_directories(foxglove_cpp_obj PUBLIC foxglove/include ${CMAKE_CURRENT_SOURCE_DIR}/../c/include)

add_library(foxglove_cpp_static STATIC)
add_library(foxglove_cpp_shared SHARED)
target_link_libraries(foxglove_cpp_static PUBLIC foxglove_cpp_obj)
target_link_libraries(foxglove_cpp_static PRIVATE foxglove_c-static)
target_link_libraries(foxglove_cpp_shared PUBLIC foxglove_cpp_obj)
target_link_libraries(foxglove_cpp_shared PRIVATE foxglove_c-shared)


### Tests

file(GLOB_RECURSE foxglove_test_srcs CONFIGURE_DEPENDS
    "foxglove/tests/*.hpp"
    "foxglove/tests/*.cpp"
)
add_executable(tests "${foxglove_test_srcs}")
set_property(TARGET tests PROPERTY CXX_STANDARD 17)
set_property(TARGET tests PROPERTY CXX_STANDARD_REQUIRED True)
target_link_libraries(tests PRIVATE foxglove_cpp_static Catch2::Catch2WithMain)
catch_discover_tests(tests)


### Examples

add_executable(example_server examples/ws-server/src/main.cpp)
set_property(TARGET example_server PROPERTY CXX_STANDARD 17)
set_property(TARGET example_server PROPERTY CXX_STANDARD_REQUIRED True)
target_link_libraries(example_server foxglove_cpp_static)
