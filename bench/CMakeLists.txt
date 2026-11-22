# Benchmarks (doc/15-benchmarks.md). Nothing here ships.
add_subdirectory(c)

if(NOT PR3_BUILD_MLPERF)
  message(STATUS "bench/mlperf skipped: PR3_BUILD_MLPERF is OFF")
elseif(EXISTS "${PR3_THIRD_PARTY_DIR}/mlperf-inference/loadgen/loadgen.h")
  add_subdirectory(mlperf)
else()
  message(STATUS "bench/mlperf skipped: LoadGen not fetched; run tools/fetch-third-party.sh")
endif()
