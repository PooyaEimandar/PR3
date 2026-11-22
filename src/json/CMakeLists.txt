# Ryu's shortest double-to-text, built from the pinned sources with their own warnings.
set(RYU_DIR "${PR3_THIRD_PARTY_DIR}/ryu")
if(NOT EXISTS "${RYU_DIR}/ryu/d2s.c")
  message(FATAL_ERROR "Ryu is not fetched: run tools/fetch-third-party.sh ryu")
endif()
add_library(pr3_ryu STATIC "${RYU_DIR}/ryu/d2s.c")
target_include_directories(pr3_ryu PUBLIC "${RYU_DIR}")
if(CMAKE_SIZEOF_VOID_P EQUAL 4)
  target_compile_definitions(pr3_ryu PRIVATE RYU_32_BIT_PLATFORM) # 32-bit multiplies
endif()
if(PR3_FREESTANDING)
  # the small tables, and the few hosted headers d2s.c includes
  target_compile_definitions(pr3_ryu PRIVATE RYU_OPTIMIZE_SIZE)
  target_include_directories(pr3_ryu BEFORE PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}/freestanding")
endif()

pr3_add_module(json
  SOURCES
    number.c
    parse.c
    write.c
  DEPENDS base
  LINK pr3_ryu)
