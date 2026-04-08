# find_or_fetch(<package_name> [VERSION <version>] [ALT_NAME <name>]
#               [EXCLUDE_FROM_ALL] [<FetchContent_Declare args>...])
#
# Declare a dependency via FetchContent_Declare(), then try to find a
# system-installed package via find_package() when
# USE_PACKAGE_MANAGER_DEPENDENCIES is ON. If not found, fall back to
# FetchContent_MakeAvailable().
#
# VERSION          - optional minimum version passed to find_package()
# ALT_NAME         - alternative package name to try if find_package() fails
#                    (e.g. for case-sensitivity differences across distros)
# EXCLUDE_FROM_ALL - use FetchContent_Populate + add_subdirectory(EXCLUDE_FROM_ALL)
#                    instead of FetchContent_MakeAvailable, to prevent the
#                    dependency's install rules from polluting our install
#
# All other arguments are forwarded to FetchContent_Declare().
macro(find_or_fetch package_name)
  cmake_parse_arguments(_FOF "EXCLUDE_FROM_ALL" "VERSION;ALT_NAME" "" ${ARGN})
  FetchContent_Declare(${package_name} ${_FOF_UNPARSED_ARGUMENTS})
  if(USE_PACKAGE_MANAGER_DEPENDENCIES)
    find_package(${package_name} ${_FOF_VERSION} QUIET)
    if(NOT ${package_name}_FOUND AND _FOF_ALT_NAME)
      find_package(${_FOF_ALT_NAME} ${_FOF_VERSION} QUIET)
      if(${_FOF_ALT_NAME}_FOUND)
        set(${package_name}_FOUND TRUE)
      endif()
    endif()
  endif()
  if(NOT ${package_name}_FOUND)
    if(_FOF_EXCLUDE_FROM_ALL)
      FetchContent_Populate(${package_name})
      add_subdirectory(${${package_name}_SOURCE_DIR} ${${package_name}_BINARY_DIR} EXCLUDE_FROM_ALL)
    else()
      FetchContent_MakeAvailable(${package_name})
    endif()
  endif()
  unset(_FOF_EXCLUDE_FROM_ALL)
  unset(_FOF_VERSION)
  unset(_FOF_ALT_NAME)
  unset(_FOF_UNPARSED_ARGUMENTS)
endmacro()
