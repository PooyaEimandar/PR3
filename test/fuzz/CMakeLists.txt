# libFuzzer targets (doc/14-build-and-quality.md, "Fuzzing"). The fuzz preset builds every
# target with ASan, UBSan and coverage; each target here adds libFuzzer's main. ctest
# replays the seed corpus and <target>/regressions/ (label "fuzz"); tools/fuzz.sh runs a
# target for a set time.
if(NOT CMAKE_C_COMPILER_ID MATCHES "Clang")
  message(FATAL_ERROR "PR3_BUILD_FUZZ needs clang: use the fuzz preset")
endif()

find_package(Python3 COMPONENTS Interpreter REQUIRED)
set(PR3_FUZZ_SEEDS "${CMAKE_CURRENT_BINARY_DIR}/seeds")

# The seed corpus: every input and output in the golden vectors, one file each.
add_test(NAME fuzz.seeds
  COMMAND Python3::Interpreter -I "${PROJECT_SOURCE_DIR}/tools/fuzz-seeds.py"
          "${PROJECT_SOURCE_DIR}/test/vectors" "${PR3_FUZZ_SEEDS}")
set_tests_properties(fuzz.seeds PROPERTIES FIXTURES_SETUP pr3_fuzz_seeds LABELS fuzz)

function(pr3_add_fuzz name)
  cmake_parse_arguments(F "" "DICT" "SOURCES;LINK" ${ARGN})
  add_executable(fuzz_${name} ${F_SOURCES})
  target_link_libraries(fuzz_${name} PRIVATE ${F_LINK})
  target_link_options(fuzz_${name} PRIVATE -fsanitize=fuzzer)
  pr3_target_warnings(fuzz_${name})
  pr3_target_own_sanitizers(fuzz_${name})
  set(args -runs=0)
  if(F_DICT)
    list(APPEND args "-dict=${CMAKE_CURRENT_SOURCE_DIR}/${F_DICT}")
  endif()
  set(dirs "${PR3_FUZZ_SEEDS}/${name}")
  if(EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/${name}/regressions")
    list(APPEND dirs "${CMAKE_CURRENT_SOURCE_DIR}/${name}/regressions")
  endif()
  add_test(NAME fuzz.${name} COMMAND fuzz_${name} ${args} ${dirs})
  set_tests_properties(fuzz.${name} PROPERTIES FIXTURES_REQUIRED pr3_fuzz_seeds LABELS fuzz)
endfunction()

pr3_add_fuzz(json SOURCES fuzz_json.c LINK pr3::json DICT json.dict)
pr3_add_fuzz(core_step SOURCES fuzz_core_step.c LINK pr3::core pr3::json)
