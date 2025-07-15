cmake_minimum_required(VERSION 3.14)
project(custom-protobuf LANGUAGES CXX)

# Set C++ standard
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Find Protocol Buffers
set(Protobuf_DEBUG ON)  # Enable debug output
set(Protobuf_USE_STATIC_LIBS ON)  # Force static libraries
set(Protobuf_VERSION 3.21.12)  # Explicitly set version to match system library
find_package(Protobuf 3.21.12 REQUIRED)

# Print debug information
message(STATUS "Protobuf_VERSION: ${Protobuf_VERSION}")
message(STATUS "Protobuf_PROTOC_EXECUTABLE: ${Protobuf_PROTOC_EXECUTABLE}")
message(STATUS "Protobuf_INCLUDE_DIR: ${Protobuf_INCLUDE_DIR}")
message(STATUS "Protobuf_LIBRARIES: ${Protobuf_LIBRARIES}")

# Ensure we use the protoc found by FindProtobuf
set(Protobuf_PROTOC_EXECUTABLE ${Protobuf_PROTOC_EXECUTABLE} CACHE FILEPATH "Protobuf compiler" FORCE)

include_directories(${Protobuf_INCLUDE_DIR})
include_directories(${CMAKE_CURRENT_BINARY_DIR})

# Generate Protocol Buffer files
set(PROTO_FILES
    ${CMAKE_CURRENT_SOURCE_DIR}/protos/fruit.proto
)

# Create a target for the protobuf generation
add_library(fruit_proto STATIC)
protobuf_generate(
    TARGET fruit_proto
    LANGUAGE cpp
    PROTOS ${PROTO_FILES}
    IMPORT_DIRS ${CMAKE_CURRENT_SOURCE_DIR}/protos
)

# Add Protocol Buffers include directories to fruit_proto
target_include_directories(fruit_proto PUBLIC
    ${Protobuf_INCLUDE_DIR}
    ${CMAKE_CURRENT_BINARY_DIR}
)

# Link Protocol Buffers to fruit_proto
target_link_libraries(fruit_proto PUBLIC ${Protobuf_LIBRARIES})

# Fetch Foxglove SDK, add this your CMakeLists.txt or build system and modify it for the released version you want.
include(FetchContent)
FetchContent_Declare(
   foxglove
   # See available releases and builds here: https://github.com/foxglove/foxglove-sdk/releases
   URL https://github.com/foxglove/foxglove-sdk/releases/download/sdk%2Fv0.9.0/foxglove-v0.9.0-cpp-x86_64-unknown-linux-gnu.zip
   DOWNLOAD_EXTRACT_TIMESTAMP TRUE
)
FetchContent_MakeAvailable(foxglove)

# Add executable
add_executable(custom_protobuf
    main.cpp
)

# Add include directory for Foxglove SDK
target_include_directories(custom_protobuf PRIVATE
    ${foxglove_SOURCE_DIR}/include
    ${foxglove_SOURCE_DIR}/include/foxglove
    ${CMAKE_CURRENT_BINARY_DIR}
    ${Protobuf_INCLUDE_DIRS}
    ${CMAKE_CURRENT_SOURCE_DIR}
)

# Find all Foxglove SDK source files
file(GLOB FOXGLOVE_SOURCES CONFIGURE_DEPENDS
    "${foxglove_SOURCE_DIR}/src/*.cpp"
    "${foxglove_SOURCE_DIR}/src/server/*.cpp"
)

# Add Foxglove SDK source files
target_sources(custom_protobuf PRIVATE ${FOXGLOVE_SOURCES})

# Link against libfoxglove.a, Protocol Buffers, and our generated protobuf library
target_link_libraries(custom_protobuf PRIVATE
    ${foxglove_SOURCE_DIR}/lib/libfoxglove.a
    ${Protobuf_LIBRARIES}
    fruit_proto
)
