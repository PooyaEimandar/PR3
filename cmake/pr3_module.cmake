# pr3_add_module() and pr3_add_test() (doc/13-project-structure.md, rules 1 and 5).
include_guard(GLOBAL)

# The layer order of doc/02-architecture.md#modules. A module may depend only on modules
# before it; tools/check-layers.sh reads this list to check every #include.
set(PR3_LAYERS base json cbor crypto core trusted memory proto infer engine store tool hybrid platform ui)
# trusted/ may use only these, so it can move behind a protected boundary later.
set(PR3_TRUSTED_ALLOWED base json crypto)

# pr3_add_module(<name> SOURCES <file>... [DEPENDS <module>...] [LINK <target>...])
# Builds the static library pr3_<name> (alias pr3::<name>) with the warning set and
# checks at configure time that its dependencies point down the layer order.
function(pr3_add_module name)
  cmake_parse_arguments(PARSE_ARGV 1 arg "" "" "SOURCES;DEPENDS;LINK")
  list(FIND PR3_LAYERS "${name}" layer)
  if(layer EQUAL -1)
    message(FATAL_ERROR "pr3_add_module: '${name}' is not in PR3_LAYERS (doc/02-architecture.md#modules)")
  endif()
  foreach(dep IN LISTS arg_DEPENDS)
    list(FIND PR3_LAYERS "${dep}" dep_layer)
    if(dep_layer EQUAL -1 OR NOT dep_layer LESS layer)
      message(FATAL_ERROR "pr3_${name} may not depend on '${dep}': layers point down (doc/02-architecture.md#modules)")
    endif()
    if(name STREQUAL "trusted" AND NOT dep IN_LIST PR3_TRUSTED_ALLOWED)
      message(FATAL_ERROR "pr3_trusted may depend only on ${PR3_TRUSTED_ALLOWED}")
    endif()
  endforeach()
  add_library(pr3_${name} STATIC ${arg_SOURCES})
  add_library(pr3::${name} ALIAS pr3_${name})
  target_include_directories(pr3_${name}
    PUBLIC "${PROJECT_SOURCE_DIR}/include"
    PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
  foreach(dep IN LISTS arg_DEPENDS)
    target_link_libraries(pr3_${name} PUBLIC pr3_${dep})
  endforeach()
  if(arg_LINK)
    target_link_libraries(pr3_${name} PRIVATE ${arg_LINK})
  endif()
  pr3_target_warnings(pr3_${name})
  pr3_target_own_sanitizers(pr3_${name})
  set_property(GLOBAL APPEND PROPERTY PR3_MODULES "${name}")
endfunction()

# pr3_add_test(<module> SOURCES <file>... [LINK <target>...])
# Builds the runner test_<module> and registers each `static void test_<behaviour>(pr3_t *t)`
# in its sources as its own ctest, named <module>.test_<behaviour>.
function(pr3_add_test module)
  cmake_parse_arguments(PARSE_ARGV 1 arg "" "" "SOURCES;LINK")
  set(runner test_${module})
  add_executable(${runner} ${arg_SOURCES})
  target_include_directories(${runner} PRIVATE "${PROJECT_SOURCE_DIR}/test")
  target_link_libraries(${runner} PRIVATE pr3_${module} ${arg_LINK})
  target_compile_definitions(${runner} PRIVATE PR3_VECTORS_DIR="${PROJECT_SOURCE_DIR}/test/vectors")
  if(MSVC)
    target_compile_definitions(${runner} PRIVATE _CRT_SECURE_NO_WARNINGS) # the harness uses fopen
  endif()
  pr3_target_warnings(${runner})
  pr3_target_own_sanitizers(${runner})
  foreach(src IN LISTS arg_SOURCES)
    file(STRINGS "${src}" lines REGEX "^static void test_[a-z0-9_]+\\(pr3_t \\*t\\)")
    foreach(line IN LISTS lines)
      string(REGEX REPLACE "^static void (test_[a-z0-9_]+)\\(.*$" "\\1" test "${line}")
      add_test(NAME ${module}.${test} COMMAND ${runner} --only ${test})
      set_tests_properties(${module}.${test} PROPERTIES LABELS "unit;${module}" TIMEOUT 120)
    endforeach()
  endforeach()
  set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS ${arg_SOURCES})
endfunction()
