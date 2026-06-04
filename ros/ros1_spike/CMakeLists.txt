cmake_minimum_required(VERSION 3.16)
project(ros1_spike CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Path to a `make build-cpp-dist`-produced SDK tree (see Container.mk). Must have
# been built with FOXGLOVE_REMOTE_ACCESS=ON (the default).
if(NOT DEFINED FOXGLOVE_CPP_SDK_DIR)
  message(FATAL_ERROR "Pass -DFOXGLOVE_CPP_SDK_DIR=/path/to/cpp/dist")
endif()

find_package(foxglove-sdk CONFIG REQUIRED HINTS "${FOXGLOVE_CPP_SDK_DIR}")

# Compiles the SDK's C++ wrapper sources and dynamic-links the remote-access
# cdylib (libfoxglove.so). Defines FOXGLOVE_REMOTE_ACCESS for consumers.
foxglove_sdk_add_cpp_library(foxglove_cpp TYPE SHARED REMOTE_ACCESS ON)

add_executable(ros1_spike src/main.cpp)
target_link_libraries(ros1_spike PRIVATE foxglove_cpp)

# Optionally link roscpp + topic_tools (ShapeShifter) to prove that a single
# binary can host both a ROS 1 Noetic node and the remote-access SDK.
option(SPIKE_WITH_ROS "Build the roscpp + ShapeShifter mini-bridge" OFF)
if(SPIKE_WITH_ROS)
  find_package(roscpp REQUIRED)
  find_package(topic_tools REQUIRED)
  target_include_directories(ros1_spike PRIVATE
    ${roscpp_INCLUDE_DIRS}
    ${topic_tools_INCLUDE_DIRS}
  )
  target_link_libraries(ros1_spike PRIVATE
    ${roscpp_LIBRARIES}
    ${topic_tools_LIBRARIES}
  )
  target_compile_definitions(ros1_spike PRIVATE SPIKE_WITH_ROS)
endif()
