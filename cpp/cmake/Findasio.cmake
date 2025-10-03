# FindASIO
# -------
#
# Finds the Asio library.
#
# Imported Targets
# ^^^^^^^^^^^^^^^^
#
# This module provides the following imported targets, if found:
#
# ``asio::asio``
#   The Asio library
#
# Result Variables
# ^^^^^^^^^^^^^^^^
#
# This will define the following variables:
#
# ``ASIO_FOUND``
#   True if the system has the Asio library.
# ``ASIO_VERSION``
#   The version of the Asio library which was found.
# ``ASIO_INCLUDE_DIRS``
#   Include directories needed to use Asio.
# ``ASIO_LIBRARIES``
#   Libraries needed to link to Asio.
#
# Cache Variables
# ^^^^^^^^^^^^^^^
#
# The following cache variables may also be set:
#
# ``ASIO_INCLUDE_DIR``
#   The directory containing ``asio.hpp``.


find_path(ASIO_INCLUDE_DIR NAMES asio.hpp NO_CMAKE_FIND_ROOT_PATH)
set(ASIO_HEADER_LOC ${ASIO_INCLUDE_DIR}/asio/version.hpp)
if(EXISTS ${ASIO_HEADER_LOC})
  # Read version info from asio header
  file(READ ${ASIO_HEADER_LOC} ver_file)
  # The line in the file to match: "#define ASIO_VERSION 101801 // 1.18.1"
  # This should work as early as asio 0.3.8
  string(REGEX MATCH "#define ASIO_VERSION ([0-9]*)" res ${ver_file})

  if(res STREQUAL "" OR CMAKE_MATCH_1 STREQUAL "")
    message(FATAL_ERROR
      "Failed to detect version information in ${ASIO_HEADER_LOC}"
    )
  endif()

  math(EXPR
    ASIO_VERSION_PATCH
    "${CMAKE_MATCH_1} % 100"
    OUTPUT_FORMAT DECIMAL
  )
  math(EXPR
    ASIO_VERSION_MINOR
    "${CMAKE_MATCH_1} / 100 % 1000"
    OUTPUT_FORMAT DECIMAL
  )
  math(EXPR
    ASIO_VERSION_MAJOR
    "${CMAKE_MATCH_1} / 100000"
    OUTPUT_FORMAT DECIMAL
  )
  set(ASIO_VERSION
    ${ASIO_VERSION_MAJOR}.${ASIO_VERSION_MINOR}.${ASIO_VERSION_PATCH}
  )

  include(FindPackageHandleStandardArgs)
  find_package_handle_standard_args(asio
    REQUIRED_VARS
    ASIO_INCLUDE_DIR
    VERSION_VAR ASIO_VERSION
  )

  if("${asio_FOUND}" AND NOT TARGET asio::asio)
    add_library(asio IMPORTED INTERFACE)
    add_library(asio::asio ALIAS asio)
    target_include_directories(asio INTERFACE ${ASIO_INCLUDE_DIR})
    target_compile_definitions(asio
      INTERFACE
      ASIO_STANDALONE
    )
    find_package(Threads REQUIRED)
    target_link_libraries(asio INTERFACE Threads::Threads)
  endif()
endif()
