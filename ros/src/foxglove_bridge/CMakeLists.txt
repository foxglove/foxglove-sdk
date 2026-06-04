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

# The ROS-agnostic bridge core. Owns the Foxglove SDK (fetched or local via
# FETCHCONTENT_SOURCE_DIR_FOXGLOVE_SDK at its build time) and the transports;
# whether remote access is available is decided when foxglove_bridge_core is
# built (FOXGLOVE_BRIDGE_REMOTE_ACCESS) and propagated here through the
# imported target's FOXGLOVE_REMOTE_ACCESS compile definition.
find_package(foxglove_bridge_core CONFIG REQUIRED)

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

target_link_libraries(foxglove_bridge_component
  foxglove_bridge_core::foxglove_bridge_core
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
