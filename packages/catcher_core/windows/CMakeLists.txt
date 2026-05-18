cmake_minimum_required(VERSION 3.14)

set(PROJECT_NAME "catcher_core")
project(${PROJECT_NAME} LANGUAGES CXX)

set(CATCHER_CORE_LIBRARY
  "${CMAKE_CURRENT_SOURCE_DIR}/lib/x64/catcher_ffi.dll"
)

if(NOT EXISTS "${CATCHER_CORE_LIBRARY}")
  message(FATAL_ERROR "Missing prebuilt catcher native library: ${CATCHER_CORE_LIBRARY}")
endif()

set(catcher_core_bundled_libraries
  "${CATCHER_CORE_LIBRARY}"
  PARENT_SCOPE
)
