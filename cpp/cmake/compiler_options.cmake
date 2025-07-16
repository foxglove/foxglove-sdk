# Sanitizer configuration
if(DEFINED SANITIZE)
    if(NOT CMAKE_CXX_COMPILER_ID MATCHES ".*Clang")
        message(FATAL_ERROR "Sanitizers are only supported with Clang, not ${CMAKE_CXX_COMPILER_ID}")
    endif()
    set(SANITIZER_COMPILE_OPTIONS -fsanitize=${SANITIZE} -fsanitize-ignorelist=${CMAKE_CURRENT_SOURCE_DIR}/sanitize-ignorelist.txt -fno-omit-frame-pointer -fno-sanitize-recover=all)
    set(SANITIZER_LINK_OPTIONS -fsanitize=${SANITIZE})
    set(SANITIZER_CARGO_FLAGS -Zbuild-std)
endif()

# Strict compilation options
option(STRICT "Enable strict compiler warnings" ON)
if (STRICT)
    if(CMAKE_CXX_COMPILER_ID MATCHES ".*Clang")
        set (STRICT_COMPILE_OPTIONS -Wall -Wextra -Wpedantic -Werror -Wold-style-cast -Wmost -Wunused-exception-parameter)
    elseif(MSVC)
        set (STRICT_COMPILE_OPTIONS /W4 /WX)
    endif()
endif()

# Enable clang-tidy if specified
if(DEFINED CLANG_TIDY)
    set(CMAKE_CXX_CLANG_TIDY clang-tidy)
endif()
