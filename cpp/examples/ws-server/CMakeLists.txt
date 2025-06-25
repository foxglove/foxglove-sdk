cmake_minimum_required(VERSION 3.14)
project(ws-server LANGUAGES CXX)

# Set C++ standard
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

include_directories(${CMAKE_CURRENT_BINARY_DIR})

# Fetch Foxglove SDK, add this your CMakeLists.txt or build system and modify it for the released version you want.
include(FetchContent)
FetchContent_Declare(
   foxglove
   DOWNLOAD_EXTRACT_TIMESTAMP TRUE
   # See available releases and builds here: https://github.com/foxglove/foxglove-sdk/releases
   URL https://github.com/foxglove/foxglove-sdk/releases/download/sdk%2Fv0.9.0/foxglove-v0.9.0-cpp-x86_64-unknown-linux-gnu.zip
   URL_HASH SHA256=48ea17c348583154d7c1934c9f7e82f57067e2afa76b586e1f7ac14cc6b6c1d0
)
FetchContent_MakeAvailable(foxglove)

add_executable(ws-server
    main.cpp
)

target_include_directories(ws-server PRIVATE
    ${foxglove_SOURCE_DIR}/include
    ${foxglove_SOURCE_DIR}/include/foxglove
    ${CMAKE_CURRENT_BINARY_DIR}
    ${CMAKE_CURRENT_SOURCE_DIR}
)

file(GLOB FOXGLOVE_SOURCES CONFIGURE_DEPENDS
    "${foxglove_SOURCE_DIR}/src/*.cpp"
    "${foxglove_SOURCE_DIR}/src/server/*.cpp"
)

target_sources(ws-server PRIVATE ${FOXGLOVE_SOURCES})

target_link_libraries(ws-server PRIVATE
    ${foxglove_SOURCE_DIR}/lib/libfoxglove.a
)
