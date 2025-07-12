# Import JSON for modern C++
include(FetchContent)
FetchContent_Declare(json
    GIT_REPOSITORY https://github.com/nlohmann/json.git
    GIT_TAG v3.12.0
)
FetchContent_MakeAvailable(json)

function(add_example name)
    add_executable(example_${name} ${name}/src/main.cpp)
    set_property(TARGET example_${name} PROPERTY CXX_STANDARD 17)
    set_property(TARGET example_${name} PROPERTY CXX_STANDARD_REQUIRED True)
    target_compile_options(example_${name} PUBLIC ${SANITIZER_COMPILE_OPTIONS} ${STRICT_COMPILE_OPTIONS})
    target_link_options(example_${name} PUBLIC ${SANITIZER_LINK_OPTIONS})
    target_link_libraries(example_${name} foxglove_cpp_static)
endfunction()

function(add_example_with_json name)
    add_example(${name})
    target_include_directories(example_${name} SYSTEM PRIVATE ${json_SOURCE_DIR}/include)
endfunction()

# Add examples
add_example(ws-server)
add_example_with_json(ws-services)
add_example(mcap)
add_example(foxglove-schemas)
add_example(param-server)
add_example(quickstart)
add_example(connection-graph)
add_example_with_json(auto-serialize)
