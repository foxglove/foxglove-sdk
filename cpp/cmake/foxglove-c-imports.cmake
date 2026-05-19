# Imports the Rust/C library targets foxglove-static and foxglove-shared from the
# given artifact paths. Used by both package configs (local install and dist);
# the static lib's platform link libraries come from the colocated
# foxglove-static-platform-links.cmake file.
#
# Each target is created only if the corresponding artifact exists on disk, so
# callers can pass candidate paths without knowing which flavor was shipped.
# An RA local install (no static C lib) and a future static-only dist (no cdylib)
# both work without caller-side gating.

function(foxglove_sdk_import_c_libs)
  cmake_parse_arguments(_arg "" "STATIC_LIB;SHARED_LIB;SHARED_IMPLIB" "" ${ARGN})

  if(_arg_STATIC_LIB AND EXISTS "${_arg_STATIC_LIB}" AND NOT TARGET foxglove-static)
    add_library(foxglove-static STATIC IMPORTED)
    set_target_properties(foxglove-static PROPERTIES IMPORTED_LOCATION "${_arg_STATIC_LIB}")
    include("${CMAKE_CURRENT_FUNCTION_LIST_DIR}/foxglove-static-platform-links.cmake")
  endif()

  if(_arg_SHARED_LIB AND EXISTS "${_arg_SHARED_LIB}" AND NOT TARGET foxglove-shared)
    add_library(foxglove-shared SHARED IMPORTED)
    set_target_properties(foxglove-shared PROPERTIES IMPORTED_LOCATION "${_arg_SHARED_LIB}")
    if(_arg_SHARED_IMPLIB AND EXISTS "${_arg_SHARED_IMPLIB}")
      set_target_properties(foxglove-shared PROPERTIES IMPORTED_IMPLIB "${_arg_SHARED_IMPLIB}")
    endif()
  endif()
endfunction()
