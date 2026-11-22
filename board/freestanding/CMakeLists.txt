# Freestanding link checks (FND-04): each image is linked with lld and no C library,
# keeping only what its entry point reaches; every symbol left undefined is listed and
# compared with the reviewed list for this target.
#   demo       the accessory demo: base only (pr3_demo_run)
#   json-link  the json module on a board (pr3_json_link_run); not part of the accessory
function(pr3_freestanding_image name entry source)
  cmake_parse_arguments(I "" "" "LIBS" ${ARGN})
  add_library(pr3_freestanding_${name} OBJECT ${source})
  target_include_directories(pr3_freestanding_${name} PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
  target_link_libraries(pr3_freestanding_${name} PRIVATE ${I_LIBS})
  pr3_target_warnings(pr3_freestanding_${name})

  set(elf "${CMAKE_CURRENT_BINARY_DIR}/pr3_${name}.elf")
  set(files)
  foreach(lib IN LISTS I_LIBS)
    list(APPEND files $<TARGET_FILE:${lib}>)
  endforeach()
  add_custom_command(
    OUTPUT "${elf}"
    COMMAND "${PR3_LLD}" --entry=${entry} --gc-sections --unresolved-symbols=ignore-all
            -Map=${CMAKE_CURRENT_BINARY_DIR}/pr3_${name}.map -o "${elf}"
            $<TARGET_OBJECTS:pr3_freestanding_${name}> --start-group ${files} --end-group
    DEPENDS pr3_freestanding_${name} ${I_LIBS} $<TARGET_OBJECTS:pr3_freestanding_${name}>
    COMMAND_EXPAND_LISTS
    COMMENT "Linking the freestanding ${name} image for ${PR3_FREESTANDING_TARGET}")
  add_custom_target(pr3_freestanding_${name}_elf ALL DEPENDS "${elf}")

  add_test(NAME freestanding.${name}.symbols
    COMMAND sh "${PROJECT_SOURCE_DIR}/tools/check-freestanding.sh" "${PR3_LLVM_NM}" "${elf}"
            "${CMAKE_CURRENT_SOURCE_DIR}/symbols-${name}-${PR3_FREESTANDING_NAME}.txt" "${PR3_LLVM_SIZE}")
  set_tests_properties(freestanding.${name}.symbols PROPERTIES LABELS "freestanding")
endfunction()

pr3_freestanding_image(demo pr3_demo_run demo.c LIBS pr3_base)
pr3_freestanding_image(json-link pr3_json_link_run json_link.c LIBS pr3_json pr3_ryu pr3_base)
