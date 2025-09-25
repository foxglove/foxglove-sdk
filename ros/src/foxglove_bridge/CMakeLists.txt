cmake_minimum_required(VERSION 3.10.2)

if(POLICY CMP0048)
  cmake_policy(SET CMP0048 NEW)
  set(CMAKE_POLICY_DEFAULT_CMP0048 NEW)
endif()
if(POLICY CMP0024)
  cmake_policy(SET CMP0024 NEW)
  set(CMAKE_POLICY_DEFAULT_CMP0024 NEW)
endif()

project(foxglove_bridge LANGUAGES CXX VERSION 3.2.0)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

option(USE_LOCAL_SDK "Use pre-built Foxglove SDK binaries from local source tree" OFF)

macro(enable_strict_compiler_warnings target)
  if (MSVC)
    target_compile_options(${target} PRIVATE /WX /W4)
  elseif(CMAKE_CXX_COMPILER_ID MATCHES "Clang")
    target_compile_options(${target} PRIVATE -Wall -Wextra -Wpedantic -Werror -Wold-style-cast -Wfloat-equal -Wmost -Wunused-exception-parameter)
  else()
    target_compile_options(${target} PRIVATE -Wall -Wextra -Wpedantic -Werror -Wold-style-cast -Wfloat-equal)
  endif()
endmacro()

find_package(nlohmann_json QUIET)
find_package(Threads REQUIRED)

if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE RelWithDebInfo)
endif()

# Determine wheter to use standalone or boost asio
option(USE_ASIO_STANDALONE "Build with standalone ASIO" ON)
if(USE_ASIO_STANDALONE)
  message(STATUS "Using standalone ASIO")
  add_definitions(-DASIO_STANDALONE)
else()
  message(STATUS "Using Boost ASIO")
  find_package(Boost REQUIRED)
endif(USE_ASIO_STANDALONE)

# Detect big-endian architectures
include(TestBigEndian)
TEST_BIG_ENDIAN(ENDIAN)
if (ENDIAN)
  add_compile_definitions(ARCH_IS_BIG_ENDIAN=1)
endif()

if (USE_LOCAL_SDK)
  set(FOXGLOVE_SDK_ROOT ${CMAKE_CURRENT_SOURCE_DIR}/../../..)
  add_library(foxglove_cpp_static STATIC IMPORTED)
  set_target_properties(foxglove_cpp_static PROPERTIES
    IMPORTED_LOCATION ${FOXGLOVE_SDK_ROOT}/cpp/build/libfoxglove_cpp_static.a
  )
  add_library(foxglove_c_static STATIC IMPORTED)
  set_target_properties(foxglove_c_static PROPERTIES
    IMPORTED_LOCATION ${FOXGLOVE_SDK_ROOT}/cpp/build/libfoxglove.a
  )
else()
  message(STATUS "Fetching Foxglove SDK artifacts from GitHub")
  set(FOXGLOVE_SDK_VERSION "0.14.2")

  if (CMAKE_SYSTEM_NAME STREQUAL "Linux" AND CMAKE_SYSTEM_PROCESSOR STREQUAL "aarch64")
    set(FOXGLOVE_SDK_PLATFORM "aarch64-unknown-linux-gnu")
    set(FOXGLOVE_SDK_SHA "f54e3e08926a8882a9bc8031cfb24a040be2638f1ef4e87236fa3aded1ab2a23")
  elseif(CMAKE_SYSTEM_NAME STREQUAL "Linux" AND CMAKE_SYSTEM_PROCESSOR STREQUAL "x86_64")
    set(FOXGLOVE_SDK_PLATFORM "x86_64-unknown-linux-gnu")
    set(FOXGLOVE_SDK_SHA "574c3ce006d6131d6519b416f25ef37c5b2a0721ef49c894286c79a5a5e0b4fc")
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
  add_library(foxglove_cpp_static STATIC)
  target_include_directories(foxglove_cpp_static SYSTEM
    PUBLIC
      $<BUILD_INTERFACE:${foxglove_sdk_SOURCE_DIR}/include>
      $<INSTALL_INTERFACE:include>
  )
  file(GLOB_RECURSE FOXGLOVE_SDK_SOURCES CONFIGURE_DEPENDS "${foxglove_sdk_SOURCE_DIR}/src/*.cpp")
  target_sources(foxglove_cpp_static PRIVATE ${FOXGLOVE_SDK_SOURCES})
  set_target_properties(foxglove_cpp_static PROPERTIES POSITION_INDEPENDENT_CODE ON)
  target_link_libraries(foxglove_cpp_static PRIVATE ${foxglove_sdk_SOURCE_DIR}/lib/libfoxglove.a)
endif()

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
find_package(rosgraph_msgs REQUIRED)
find_package(rclcpp REQUIRED)
find_package(rclcpp_components REQUIRED)
find_package(resource_retriever REQUIRED)
find_package(rosx_introspection REQUIRED)

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
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/../../../c/include>
    # For generated version.hpp
    $<BUILD_INTERFACE:${CMAKE_CURRENT_BINARY_DIR}/include>
    $<INSTALL_INTERFACE:include>
)

# If using the local SDK, include the C++ SDK headers from the local source tree, else
# include the C++ SDK headers from the installed location
if (USE_LOCAL_SDK)
  target_include_directories(foxglove_bridge_component
    PUBLIC
      $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/../../../cpp/foxglove/include>
  )
else()
  target_include_directories(foxglove_bridge_component
    PUBLIC
      $<BUILD_INTERFACE:${foxglove_sdk_SOURCE_DIR}/include>
  )
endif()

if (USE_LOCAL_SDK)
  # The locally-built C++ SDK needs to be statically linked to the C SDK
  target_link_libraries(foxglove_bridge_component foxglove_c_static)
endif()

if (nlohmann_json_FOUND)
  target_link_libraries(foxglove_bridge_component nlohmann_json::nlohmann_json)
endif()

target_compile_definitions(foxglove_bridge_component
  PRIVATE
    RESOURCE_RETRIEVER_VERSION_MAJOR=${resource_retriever_VERSION_MAJOR}
    RESOURCE_RETRIEVER_VERSION_MINOR=${resource_retriever_VERSION_MINOR}
    RESOURCE_RETRIEVER_VERSION_PATCH=${resource_retriever_VERSION_PATCH}
)

target_link_libraries(foxglove_bridge_component
  ${rosgraph_msgs_TARGETS}
  ament_index_cpp::ament_index_cpp
  foxglove_cpp_static
  rclcpp::rclcpp
  rclcpp_components::component
  rclcpp_components::component_manager
  resource_retriever::resource_retriever
  rosx_introspection::rosx_introspection
)

rclcpp_components_register_nodes(foxglove_bridge_component "foxglove_bridge::FoxgloveBridge")
enable_strict_compiler_warnings(foxglove_bridge_component)

add_executable(foxglove_bridge
  src/ros2_foxglove_bridge_node.cpp
)
target_include_directories(foxglove_bridge SYSTEM PRIVATE ${rclcpp_INCLUDE_DIRS})
target_link_libraries(foxglove_bridge
  foxglove_bridge_component
  rclcpp::rclcpp
  rclcpp_components::component
  rclcpp_components::component_manager
)
enable_strict_compiler_warnings(foxglove_bridge)

if(BUILD_TESTING)
  find_package(ament_cmake_gtest REQUIRED)
  find_package(ament_lint_auto REQUIRED)
  find_package(OpenSSL REQUIRED)
  find_package(websocketpp REQUIRED)

  ament_lint_auto_find_test_dependencies()

  # Repeat tests several times to catch nondeterministic issues
  ament_add_gtest(smoke_test tests/smoke_test.cpp ENV GTEST_REPEAT=50 TIMEOUT 600)
  target_link_libraries(smoke_test
    foxglove_bridge_component
    ${std_msgs_TARGETS}
    ${std_srvs_TARGETS}
    rclcpp::rclcpp
    rclcpp_components::component
    rclcpp_components::component_manager
    OpenSSL::SSL
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
