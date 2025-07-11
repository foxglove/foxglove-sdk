# Import Catch2 for testing
include(FetchContent)
FetchContent_Declare(
    Catch2
    GIT_REPOSITORY https://github.com/catchorg/Catch2.git
    GIT_TAG        v3.8.0
)
FetchContent_MakeAvailable(Catch2)
list(APPEND CMAKE_MODULE_PATH ${catch2_SOURCE_DIR}/extras)
include(Catch)

# Test executable
file(GLOB_RECURSE foxglove_test_srcs CONFIGURE_DEPENDS
    "*.hpp"
    "*.cpp"
)

add_executable(tests "${foxglove_test_srcs}")
set_property(TARGET tests PROPERTY CXX_STANDARD 17)
set_property(TARGET tests PROPERTY CXX_STANDARD_REQUIRED True)
target_compile_definitions(tests PRIVATE ASIO_STANDALONE)
target_compile_options(tests PUBLIC ${SANITIZER_COMPILE_OPTIONS})
target_link_options(tests PUBLIC ${SANITIZER_LINK_OPTIONS})
target_include_directories(tests SYSTEM PRIVATE
    ${asio_SOURCE_DIR}/asio/include
    ${json_SOURCE_DIR}/include
    ${websocketpp_SOURCE_DIR}
)
target_link_libraries(tests PRIVATE foxglove_cpp_static Catch2::Catch2WithMain)
catch_discover_tests(tests)

if(MSVC)
    # Auto-detection of C++11 support is not working on the windows-2025 runner with default settings
    target_compile_options(tests PRIVATE /Zc:__cplusplus)
endif()
