cmake_minimum_required(VERSION 3.10)

set(PROJECT_NAME "catcher_core")
project(${PROJECT_NAME} LANGUAGES CXX)

if(CMAKE_SYSTEM_PROCESSOR MATCHES "^(aarch64|arm64)$")
  set(CATCHER_CORE_ARCH "arm64")
else()
  set(CATCHER_CORE_ARCH "x64")
endif()

set(CATCHER_CORE_LIBRARY
  "${CMAKE_CURRENT_SOURCE_DIR}/lib/${CATCHER_CORE_ARCH}/libcatcher_ffi.so"
)

if(NOT EXISTS "${CATCHER_CORE_LIBRARY}")
  message(FATAL_ERROR "Missing prebuilt catcher native library: ${CATCHER_CORE_LIBRARY}")
endif()

set(catcher_core_bundled_libraries
  "${CATCHER_CORE_LIBRARY}"
  PARENT_SCOPE
)
