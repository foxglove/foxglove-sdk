cmake_minimum_required(VERSION 3.17)

if(POLICY CMP0048)
  cmake_policy(SET CMP0048 NEW)
  set(CMAKE_POLICY_DEFAULT_CMP0048 NEW)
endif()
if(POLICY CMP0024)
  cmake_policy(SET CMP0024 NEW)
  set(CMAKE_POLICY_DEFAULT_CMP0024 NEW)
endif()

project(foxglove_bridge LANGUAGES CXX VERSION 3.4.1)

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
  BUILD_WITH_INSTALL_RPATH TRUE
)

find_program(GIT_SCM git DOC "Git version control")
if (GIT_SCM)
  execute_process(
    COMMAND ${GIT_SCM} describe --always --dirty --exclude="*"
    WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}
    OUTPUT_VARIABLE FOXGLOVE_BRIDGE_GIT_HASH
    OUTPUT_STRIP_TRAILING_WHITESPACE
  )
endif()
set(FOXGLOVE_BRIDGE_VERSION "${CMAKE_PROJECT_VERSION}")

if(NOT DEFINED ENV{AMENT_PREFIX_PATH})
  message(FATAL_ERROR "ament_cmake is not detected but is required for building")
endif()

set(ROS_BUILD_TYPE "ament_cmake")

find_package(ament_cmake REQUIRED)
find_package(ament_index_cpp REQUIRED)
find_package(rcl_interfaces REQUIRED)
find_package(rcpputils REQUIRED)
find_package(rcutils REQUIRED)
find_package(rosgraph_msgs REQUIRED)
find_package(rclcpp REQUIRED)
find_package(rclcpp_components REQUIRED)
find_package(resource_retriever REQUIRED)
find_package(rosidl_typesupport_introspection_cpp REQUIRED)
find_package(rosx_introspection REQUIRED)
find_package(std_msgs REQUIRED)

# Generate version.hpp
configure_file(include/foxglove_bridge/version.hpp.in include/foxglove_bridge/version.hpp @ONLY)

add_library(foxglove_bridge_component SHARED
  src/message_definition_cache.cpp
  src/param_utils.cpp
  src/ros2_foxglove_bridge.cpp
  src/parameter_interface.cpp
  src/generic_client.cpp
)
target_include_directories(foxglove_bridge_component
  PUBLIC
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
    # For generated version.hpp
    $<BUILD_INTERFACE:${CMAKE_CURRENT_BINARY_DIR}/include>
    $<INSTALL_INTERFACE:include>
)

target_compile_definitions(foxglove_bridge_component
  PRIVATE
    RESOURCE_RETRIEVER_VERSION_MAJOR=${resource_retriever_VERSION_MAJOR}
    RESOURCE_RETRIEVER_VERSION_MINOR=${resource_retriever_VERSION_MINOR}
    RESOURCE_RETRIEVER_VERSION_PATCH=${resource_retriever_VERSION_PATCH}
)

if (FOXGLOVE_BRIDGE_REMOTE_ACCESS)
  target_compile_definitions(foxglove_bridge_component PUBLIC FOXGLOVE_REMOTE_ACCESS)
endif()

target_link_libraries(foxglove_bridge_component
  foxglove_cpp_shared
  ${rcl_interfaces_TARGETS}
  ${rosgraph_msgs_TARGETS}
  ${std_msgs_TARGETS}
  ament_index_cpp::ament_index_cpp
  rclcpp::rclcpp
  rclcpp_components::component
  rcpputils::rcpputils
  rcutils::rcutils
  resource_retriever::resource_retriever
  rosidl_typesupport_introspection_cpp::rosidl_typesupport_introspection_cpp
  rosx_introspection::rosx_introspection
)

rclcpp_components_register_nodes(foxglove_bridge_component "foxglove_bridge::FoxgloveBridge")
enable_strict_compiler_warnings(foxglove_bridge_component)
set_target_properties(foxglove_bridge_component PROPERTIES
  INSTALL_RPATH "$ORIGIN"
  BUILD_WITH_INSTALL_RPATH TRUE
)

add_executable(foxglove_bridge
  src/ros2_foxglove_bridge_node.cpp
)
target_include_directories(foxglove_bridge SYSTEM PRIVATE ${rclcpp_INCLUDE_DIRS})
target_link_libraries(foxglove_bridge
  foxglove_bridge_component
  ${rcl_interfaces_TARGETS}
  rclcpp::rclcpp
)
enable_strict_compiler_warnings(foxglove_bridge)

if(BUILD_TESTING)
  find_package(ament_cmake_gtest REQUIRED)
  find_package(ament_lint_auto REQUIRED)
  find_package(nlohmann_json REQUIRED)
  find_package(std_srvs REQUIRED)
  find_package(websocketpp REQUIRED)

  ament_lint_auto_find_test_dependencies()

  # smoke_test pulls in tests/client/test_client.hpp, which uses nlohmann::json for the
  # test WebSocket client's serializers. utils_test doesn't touch it.
  ament_add_gtest(smoke_test tests/smoke_test.cpp TIMEOUT 600)
  # ASIO_STANDALONE makes websocketpp's templates dispatch to standalone asio (header-only,
  # supplied by the `asio` system package) rather than boost::asio. Only smoke_test pulls
  # in websocketpp at all — production sources don't touch it — so the define is scoped here.
  target_compile_definitions(smoke_test PRIVATE ASIO_STANDALONE)
  target_link_libraries(smoke_test
    foxglove_bridge_component
    nlohmann_json::nlohmann_json
    ${std_msgs_TARGETS}
    ${std_srvs_TARGETS}
    rclcpp::rclcpp
    websocketpp::websocketpp
  )
  enable_strict_compiler_warnings(smoke_test)

  ament_add_gtest(utils_test tests/utils_test.cpp)
  target_link_libraries(utils_test foxglove_bridge_component)
  enable_strict_compiler_warnings(utils_test)
endif()

install(FILES include/foxglove_bridge/ros2_foxglove_bridge.hpp
  DESTINATION include/${PROJECT_NAME}/
)
install(TARGETS foxglove_bridge_component
  ARCHIVE DESTINATION lib
  LIBRARY DESTINATION lib
  RUNTIME DESTINATION bin
)
# Install the SDK shared library alongside the bridge component library so the
# runtime linker can find it via RPATH ($ORIGIN). Not needed when remote access
# is disabled, since the SDK is statically linked into foxglove_cpp_shared.
install(FILES $<TARGET_FILE:foxglove_cpp_shared> DESTINATION lib)
if (FOXGLOVE_BRIDGE_REMOTE_ACCESS)
  install(FILES $<TARGET_FILE:foxglove-shared> DESTINATION lib)
endif()
install(TARGETS foxglove_bridge
  DESTINATION lib/${PROJECT_NAME}
)
install(DIRECTORY launch
  DESTINATION share/${PROJECT_NAME}/
)
install(FILES include/foxglove_bridge/utils.hpp
  DESTINATION include/${PROJECT_NAME}/
)
ament_export_libraries(foxglove_bridge_component)
ament_package()
