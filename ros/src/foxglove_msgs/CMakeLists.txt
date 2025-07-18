cmake_minimum_required(VERSION 3.5)
project(foxglove_msgs)

if(CMAKE_CXX_STANDARD LESS 17)
  set(CMAKE_CXX_STANDARD 17)
  set(CMAKE_CXX_STANDARD_REQUIRED ON)
endif()

if(CMAKE_COMPILER_IS_GNUCXX OR CMAKE_CXX_COMPILER_ID MATCHES "Clang")
  add_compile_options(-Wall -Wextra -Wpedantic)
endif()

find_package(ament_cmake REQUIRED)
find_package(builtin_interfaces REQUIRED)
find_package(geometry_msgs REQUIRED)
find_package(rosidl_default_generators REQUIRED)

# Copy msg files from schemas/ros2
set(SCHEMAS_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../../schemas/ros2")
file(GLOB _ros2_schemas "${SCHEMAS_DIR}/*.msg")
if(NOT _ros2_schemas)
  message(FATAL_ERROR "No .msg files found in ${SCHEMAS_DIR}")
endif()
file(REMOVE_RECURSE "${CMAKE_CURRENT_SOURCE_DIR}/msg")
file(COPY ${_ros2_schemas} DESTINATION "${CMAKE_CURRENT_SOURCE_DIR}/msg")

# Generate ROS interfaces
file(GLOB msg_files RELATIVE "${CMAKE_CURRENT_SOURCE_DIR}" "msg/*.msg")
list(LENGTH msg_files _msg_files_count)
message(STATUS "Generating interfaces for ${PROJECT_NAME} with ${_msg_files_count} msg files")
rosidl_generate_interfaces(${PROJECT_NAME}
  ${msg_files}
  DEPENDENCIES builtin_interfaces geometry_msgs
  ADD_LINTER_TESTS
)

ament_export_dependencies(rosidl_default_runtime)

if(BUILD_TESTING)
  find_package(ament_lint_auto REQUIRED)
  list(APPEND AMENT_LINT_AUTO_EXCLUDE
    ament_cmake_copyright
    ament_cmake_uncrustify
  )
  ament_lint_auto_find_test_dependencies()

  find_package(ament_cmake_pytest REQUIRED)
  ament_add_pytest_test(${PROJECT_NAME}_test_msgs test/test_msgs.py)
endif()

ament_package()
