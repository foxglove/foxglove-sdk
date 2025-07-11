# Import required dependencies
include(FetchContent)

# WebSocket++
FetchContent_Declare(
    websocketpp
    GIT_REPOSITORY https://github.com/zaphoyd/websocketpp.git
    GIT_TAG 0.8.2
    OVERRIDE_FIND_PACKAGE
)
set(FETCHCONTENT_TRY_FIND_PACKAGE_MODE NEVER)
FetchContent_GetProperties(websocketpp)
if(NOT websocketpp_POPULATED)
    FetchContent_Populate(websocketpp)
    add_library(websocketpp INTERFACE)
    target_include_directories(websocketpp INTERFACE ${websocketpp_SOURCE_DIR})
endif()
FetchContent_MakeAvailable(websocketpp)

# ASIO for async I/O
FetchContent_Declare(asio
    GIT_REPOSITORY https://github.com/chriskohlhoff/asio.git
    GIT_TAG asio-1-31-0
)
FetchContent_MakeAvailable(asio)

# JSON for modern C++
FetchContent_Declare(json
    GIT_REPOSITORY https://github.com/nlohmann/json.git
    GIT_TAG v3.12.0
)
FetchContent_MakeAvailable(json)

# Foxglove library
file(GLOB_RECURSE foxglove_cpp_srcs CONFIGURE_DEPENDS
    "include/*.hpp"
    "src/*.hpp"
    "src/*.cpp"
)

add_library(foxglove_cpp_obj OBJECT "${foxglove_cpp_srcs}")
set_property(TARGET foxglove_cpp_obj PROPERTY POSITION_INDEPENDENT_CODE True)
set_property(TARGET foxglove_cpp_obj PROPERTY CXX_STANDARD 17)
set_property(TARGET foxglove_cpp_obj PROPERTY CXX_STANDARD_REQUIRED True)
target_include_directories(foxglove_cpp_obj PUBLIC
    include
    ${CMAKE_CURRENT_SOURCE_DIR}/../../c/include
    ${asio_SOURCE_DIR}/asio/include
    ${json_SOURCE_DIR}/include
    ${websocketpp_SOURCE_DIR}
)
target_compile_options(foxglove_cpp_obj PUBLIC ${SANITIZER_COMPILE_OPTIONS} ${STRICT_COMPILE_OPTIONS})
target_link_options(foxglove_cpp_obj PUBLIC ${SANITIZER_LINK_OPTIONS})

add_library(foxglove_cpp_static STATIC)
add_library(foxglove_cpp_shared SHARED)
target_link_libraries(foxglove_cpp_static PUBLIC foxglove_cpp_obj)
target_link_libraries(foxglove_cpp_static PRIVATE foxglove-static)
target_link_libraries(foxglove_cpp_shared PUBLIC foxglove_cpp_obj)
target_link_libraries(foxglove_cpp_shared PRIVATE foxglove-shared)

add_custom_target(doxygen
  COMMAND doxygen Doxyfile
  WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/..
)

add_custom_target(html-docs
  COMMAND poetry run sphinx-build --fail-on-warning . ${CMAKE_CURRENT_BINARY_DIR}/docs/html
  WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/docs
)
