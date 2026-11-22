# PR3's microbenchmarks for the module budgets (doc/15-benchmarks.md, "Module budgets"):
# one runner per module, every device reading the same workload files.
function(pr3_add_bench module)
  cmake_parse_arguments(B "" "" "SOURCES;LINK" ${ARGN})
  add_executable(bench_${module} ${B_SOURCES})
  target_link_libraries(bench_${module} PRIVATE ${B_LINK})
  target_compile_definitions(bench_${module} PRIVATE
    PR3_BENCH_WORKLOADS="${CMAKE_CURRENT_SOURCE_DIR}/workloads"
    PR3_BENCH_CONFIG="${CMAKE_BUILD_TYPE}"
    $<$<C_COMPILER_ID:MSVC>:_CRT_SECURE_NO_WARNINGS>)
  pr3_target_warnings(bench_${module})
  # ctest checks the runner works, with short runs; timings come from tools/bench.sh
  add_test(NAME bench.${module}.smoke COMMAND bench_${module} --runs 3 --min-ms 1)
  set_tests_properties(bench.${module}.smoke PROPERTIES LABELS bench)
endfunction()

pr3_add_bench(json SOURCES bench_json.c LINK pr3::json)

pr3_add_bench(core SOURCES bench_core.c LINK pr3::core)
