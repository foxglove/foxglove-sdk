# This CMake file is used for CI, examples, and tests. It builds the whole project from scratch
# including the Rust code. If you would like to use the C++ library only, you only need to compile
# the sources under `cpp/foxglove`, link with the C static or shared library, and add both the C/C++
# includes to include paths.

cmake_minimum_required(VERSION 3.25)
project(foxglove-sdk)

# Enable ccache if available
find_program(CCACHE_PROGRAM ccache)
if(CCACHE_PROGRAM)
    set(CMAKE_C_COMPILER_LAUNCHER "${CCACHE_PROGRAM}")
    set(CMAKE_CXX_COMPILER_LAUNCHER "${CCACHE_PROGRAM}")
    message(STATUS "Using ccache: ${CCACHE_PROGRAM}")
endif()

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED True)

# Include common configurations
include(CTest)
include(${CMAKE_CURRENT_SOURCE_DIR}/cmake/compiler_options.cmake)

# Import Rust integration
include(FetchContent)
FetchContent_Declare(
    Corrosion
    GIT_REPOSITORY https://github.com/corrosion-rs/corrosion.git
    GIT_TAG v0.5.1
)
if(CMAKE_GENERATOR MATCHES "Unix Makefiles|Ninja")
    unset(CMAKE_CONFIGURATION_TYPES CACHE)
endif()
FetchContent_MakeAvailable(Corrosion)

# Import Rust C library
corrosion_import_crate(
    MANIFEST_PATH ../c/Cargo.toml
    CRATES foxglove_c
    FLAGS ${SANITIZER_CARGO_FLAGS}
)

if(DEFINED SANITIZE)
    string(REGEX REPLACE undefined "" rust_sanitizers ${SANITIZE})
    string(REGEX REPLACE ,, , rust_sanitizers ${rust_sanitizers})
    string(REGEX REPLACE ,$ "" rust_sanitizers ${rust_sanitizers})
    message(WARNING "Enabling -Zsanitizer=${rust_sanitizers} for Rust")
    corrosion_add_target_rustflags(foxglove -Zsanitizer=${rust_sanitizers} -Cforce-frame-pointers=yes)
endif()

if(WIN32)
    target_link_libraries(foxglove-static INTERFACE Bcrypt)
endif()

# Add subdirectories for C++ library, tests and examples
add_subdirectory(foxglove)
add_subdirectory(tests)
add_subdirectory(examples)
