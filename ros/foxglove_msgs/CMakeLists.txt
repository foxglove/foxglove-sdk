find_package(ros_environment REQUIRED)

set(ROS_VERSION $ENV{ROS_VERSION})

if(${ROS_VERSION} EQUAL 1)
  cmake_minimum_required(VERSION 3.0.2)
  project(foxglove_msgs)

  # Default to C++11
  if(NOT CMAKE_CXX_STANDARD)
    set(CMAKE_CXX_STANDARD 11)
  endif()

  find_package(catkin REQUIRED
    COMPONENTS
    message_generation
    visualization_msgs
    geometry_msgs
  )

  # Copy .msg files into a msg/ folder, so that add_message_files installs them
  # into share/${project_name}/msg.
  file(GLOB _generated_ros1_schemas ros1/*)
  file(REMOVE_RECURSE msg)
  file(COPY
    ${_generated_ros1_schemas}
    ImageMarkerArray.msg
    DESTINATION "${CMAKE_CURRENT_SOURCE_DIR}/msg"
  )

  file(GLOB _all_msg_files RELATIVE "${CMAKE_CURRENT_SOURCE_DIR}" msg/*)
  message(STATUS "Using msg files: ${_all_msg_files}")
  if(NOT _all_msg_files)
    message(FATAL_ERROR "No msg files found")
  endif()

  add_message_files(DIRECTORY msg)

  generate_messages(DEPENDENCIES visualization_msgs geometry_msgs)

  catkin_package(CATKIN_DEPENDS message_runtime)

  if(CATKIN_ENABLE_TESTING)
    catkin_add_nosetests(test/test_msgs.py DEPENDENCIES ${${PROJECT_NAME}_EXPORTED_TARGETS})
  endif()

elseif(${ROS_VERSION} EQUAL 2)
  cmake_minimum_required(VERSION 3.5)
  project(foxglove_msgs)

  if(NOT CMAKE_CXX_STANDARD)
    set(CMAKE_CXX_STANDARD_REQUIRED ON)
    set(CMAKE_CXX_STANDARD 14)
  endif()

  if(CMAKE_COMPILER_IS_GNUCXX OR CMAKE_CXX_COMPILER_ID MATCHES "Clang")
    add_compile_options(-Wall -Wextra -Wpedantic)
  endif()

  find_package(ament_cmake REQUIRED)
  find_package(visualization_msgs REQUIRED)
  find_package(geometry_msgs REQUIRED)
  find_package(rosidl_default_generators REQUIRED)

  # Copy .msg files into a msg/ folder, so that rosidl_generate_interfaces
  # installs them into share/${project_name}/msg. Subdirectories not named "msg"
  # will confuse various tooling like rosbridge_server and rosbag2_storage_mcap.
  file(GLOB _generated_ros2_schemas ros2/*)
  file(REMOVE_RECURSE msg)
  file(COPY
    ${_generated_ros2_schemas}
    ImageMarkerArray.msg
    DESTINATION "${CMAKE_CURRENT_SOURCE_DIR}/msg"
  )

  file(GLOB _all_msg_files RELATIVE "${CMAKE_CURRENT_SOURCE_DIR}" msg/*)
  message(STATUS "Using msg files: ${_all_msg_files}")
  if(NOT _all_msg_files)
    message(FATAL_ERROR "No msg files found")
  endif()

  rosidl_generate_interfaces(${PROJECT_NAME}
    ${_all_msg_files}
    DEPENDENCIES visualization_msgs geometry_msgs
    ADD_LINTER_TESTS
  )

  ament_export_dependencies(rosidl_default_runtime)

  if(BUILD_TESTING)
    find_package(ament_cmake_pytest REQUIRED)
    find_package(ament_lint_auto REQUIRED)
    list(APPEND AMENT_LINT_AUTO_EXCLUDE
      ament_cmake_copyright
    )
    ament_lint_auto_find_test_dependencies()
    ament_add_pytest_test(${PROJECT_NAME}_test_msgs test/test_msgs.py)
  endif()

  ament_package()

endif()
