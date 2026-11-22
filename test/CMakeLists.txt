# Unit tests: one runner per module (doc/13-project-structure.md, rule 5).
find_package(Threads)

pr3_add_test(base SOURCES "${CMAKE_CURRENT_SOURCE_DIR}/unit/test_base.c")
if(Threads_FOUND)
  target_link_libraries(test_base PRIVATE Threads::Threads)
endif()

pr3_add_test(json SOURCES "${CMAKE_CURRENT_SOURCE_DIR}/unit/test_json.c")

pr3_add_test(core SOURCES "${CMAKE_CURRENT_SOURCE_DIR}/unit/test_core.c")
