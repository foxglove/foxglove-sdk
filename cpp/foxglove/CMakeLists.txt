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
