# Sanitizers apply to every target, third-party code included, so a sanitizer sees across
# the boundary (doc/14-build-and-quality.md).
include_guard(GLOBAL)

if(PR3_SANITIZE)
  if(MSVC)
    if(PR3_SANITIZE MATCHES "address")
      add_compile_options(/fsanitize=address)
    endif()
  else()
    add_compile_options(-fsanitize=${PR3_SANITIZE} -fno-omit-frame-pointer -fno-sanitize-recover=all)
    add_link_options(-fsanitize=${PR3_SANITIZE})
  endif()
  if(PR3_SANITIZE MATCHES "address")
    add_compile_definitions(PR3_ASAN_POISON=1)
  endif()
  if(PR3_SANITIZE MATCHES "memory")
    add_compile_options(-fsanitize-memory-track-origins)
  endif()
  message(STATUS "PR3 sanitizers: ${PR3_SANITIZE}")
endif()

# Checks that are not undefined behaviour but catch mistakes in PR3's own code: unsigned
# wraparound, implicit truncation and sign changes, shifted-out bits. They apply only to
# PR3's sources (pr3_add_module, pr3_add_test); code that wraps on purpose is marked
# PR3_WRAPPING. The ubsan preset turns them on.
set(PR3_SANITIZE_OWN "" CACHE STRING "Extra sanitizers for PR3's own targets only (e.g. integer,nullability)")
if(PR3_SANITIZE_OWN AND NOT MSVC)
  include(CheckCCompilerFlag)
  set(_pr3_saved_required_flags "${CMAKE_REQUIRED_FLAGS}")
  set(CMAKE_REQUIRED_FLAGS "${CMAKE_REQUIRED_FLAGS} -fsanitize=${PR3_SANITIZE_OWN}")
  check_c_compiler_flag("-fsanitize=${PR3_SANITIZE_OWN}" PR3_HAS_SANITIZE_OWN)
  set(CMAKE_REQUIRED_FLAGS "${_pr3_saved_required_flags}")
  if(NOT PR3_HAS_SANITIZE_OWN)
    message(STATUS "PR3 own-target sanitizers ${PR3_SANITIZE_OWN} not supported by this compiler; skipped")
  endif()
endif()

function(pr3_target_own_sanitizers target)
  if(PR3_SANITIZE_OWN AND PR3_HAS_SANITIZE_OWN)
    target_compile_options(${target} PRIVATE -fsanitize=${PR3_SANITIZE_OWN} -fno-sanitize-recover=all)
    target_link_options(${target} PRIVATE -fsanitize=${PR3_SANITIZE_OWN})
  endif()
endfunction()
