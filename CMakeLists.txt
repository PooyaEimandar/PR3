# PR3: the root project. Modules join as their issues land (doc/13-project-structure.md).
cmake_minimum_required(VERSION 3.28)

project(pr3
  VERSION 0.1.0
  DESCRIPTION "Privacy-first personal AI runtime"
  LANGUAGES C)

set(CMAKE_C_STANDARD 17)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_C_EXTENSIONS OFF)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

option(PR3_WARNINGS_AS_ERRORS "Treat warnings in PR3's own sources as errors" ON)
option(PR3_BUILD_TESTS "Build the unit tests under test/" ON)
option(PR3_BUILD_BENCH "Build the benchmarks under bench/" ON)
option(PR3_BUILD_MLPERF "Build the MLPerf LoadGen harness under bench/mlperf/ (C++17)" ON)
option(PR3_BUILD_FUZZ "Build the libFuzzer targets under test/fuzz/ (clang; the fuzz preset)" OFF)
set(PR3_SANITIZE "" CACHE STRING "Sanitizers for every target, third-party code included (e.g. address,undefined)")

list(APPEND CMAKE_MODULE_PATH "${PROJECT_SOURCE_DIR}/cmake")
include(pr3_warnings)
include(pr3_sanitizers)
include(pr3_hardening)
include(pr3_module)

set(PR3_THIRD_PARTY_DIR "${PROJECT_SOURCE_DIR}/third_party")

enable_testing()

add_subdirectory(src)

if(PR3_FREESTANDING)
  # Boards: the portable libraries and the freestanding demo only (doc/11).
  add_subdirectory(board/freestanding)
  return()
endif()

if(PR3_BUILD_TESTS)
  add_subdirectory(test)
endif()

if(PR3_BUILD_FUZZ)
  add_subdirectory(test/fuzz)
endif()

if(PR3_BUILD_BENCH)
  add_subdirectory(bench)
endif()
