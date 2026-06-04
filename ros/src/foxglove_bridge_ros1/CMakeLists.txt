cmake_minimum_required(VERSION 3.17)
project(foxglove_bridge_ros1 LANGUAGES CXX VERSION 3.4.1)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

macro(enable_strict_compiler_warnings target)
  if(CMAKE_CXX_COMPILER_ID MATCHES "Clang")
    target_compile_options(${target} PRIVATE -Wall -Wextra -Wpedantic -Werror -Wold-style-cast -Wfloat-equal -Wmost -Wunused-exception-parameter)
  else()
    target_compile_options(${target} PRIVATE -Wall -Wextra -Wpedantic -Werror -Wold-style-cast -Wfloat-equal)
  endif()
endmacro()

if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE RelWithDebInfo)
endif()

find_package(catkin REQUIRED COMPONENTS
  roscpp
  rosgraph_msgs
  topic_tools
  ros_babel_fish
)

# The ROS-agnostic bridge core (plain CMake package, built earlier in the same
# workspace). Provides the SDK transports and the FOXGLOVE_REMOTE_ACCESS
# compile definition when built with remote access.
find_package(foxglove_bridge_core CONFIG REQUIRED)

catkin_package()

add_executable(foxglove_bridge
  src/param_interface.cpp
  src/ros1_foxglove_bridge.cpp
  src/ros1_foxglove_bridge_node.cpp
  src/service_utils.cpp
)
target_include_directories(foxglove_bridge
  PRIVATE
    include
)
target_include_directories(foxglove_bridge SYSTEM PRIVATE ${catkin_INCLUDE_DIRS})
target_link_libraries(foxglove_bridge
  foxglove_bridge_core::foxglove_bridge_core
  ${catkin_LIBRARIES}
)
enable_strict_compiler_warnings(foxglove_bridge)
set_target_properties(foxglove_bridge PROPERTIES
  INSTALL_RPATH "$ORIGIN/../../lib"
  BUILD_WITH_INSTALL_RPATH TRUE
)

install(TARGETS foxglove_bridge
  RUNTIME DESTINATION ${CATKIN_PACKAGE_BIN_DESTINATION}
)
install(DIRECTORY launch
  DESTINATION ${CATKIN_PACKAGE_SHARE_DESTINATION}
)
