# The warning set of doc/14-build-and-quality.md, applied per target to PR3's own sources.
# Third-party targets keep their own warnings.
include_guard(GLOBAL)
include(CheckCCompilerFlag)
include(CheckCXXCompilerFlag)

set(_pr3_c_warning_candidates
  -Wall -Wextra -Wpedantic -Wconversion -Wsign-conversion -Wshadow -Wcast-qual
  -Wvla -Wswitch-enum -Wimplicit-fallthrough -Wstrict-prototypes -Wmissing-prototypes
  -Wmissing-variable-declarations -Wnull-dereference -Wformat=2 -Wundef
  -Wdouble-promotion -Wpointer-arith -Wwrite-strings)
if(CMAKE_C_COMPILER_ID STREQUAL "GNU")
  list(APPEND _pr3_c_warning_candidates -Wcast-align=strict)
else()
  list(APPEND _pr3_c_warning_candidates -Wcast-align)
endif()

set(_pr3_cxx_warning_candidates
  -Wall -Wextra -Wpedantic -Wconversion -Wsign-conversion -Wshadow -Wcast-qual
  -Wnull-dereference -Wformat=2 -Wundef -Wdouble-promotion)

set(PR3_C_WARNINGS "")
set(PR3_CXX_WARNINGS "")
if(MSVC)
  # C11 atomics in MSVC's C mode are behind a flag (Visual Studio 17.5 and later).
  set(PR3_C_WARNINGS /W4 /sdl /permissive- /experimental:c11atomics)
  set(PR3_CXX_WARNINGS /W4 /sdl /permissive-)
  if(PR3_WARNINGS_AS_ERRORS)
    list(APPEND PR3_C_WARNINGS /WX)
    list(APPEND PR3_CXX_WARNINGS /WX)
  endif()
else()
  set(_pr3_saved_required_flags "${CMAKE_REQUIRED_FLAGS}")
  set(CMAKE_REQUIRED_FLAGS "${CMAKE_REQUIRED_FLAGS} -Werror")
  foreach(flag IN LISTS _pr3_c_warning_candidates)
    string(MAKE_C_IDENTIFIER "PR3_HAS_C${flag}" var)
    check_c_compiler_flag("${flag}" ${var})
    if(${var})
      list(APPEND PR3_C_WARNINGS "${flag}")
    endif()
  endforeach()
  set(CMAKE_REQUIRED_FLAGS "${_pr3_saved_required_flags}")
  if(PR3_WARNINGS_AS_ERRORS)
    list(APPEND PR3_C_WARNINGS -Werror)
  endif()
endif()

# pr3_target_warnings(<target>): the C set on C sources, and the C++ set on the few
# C++ sources PR3 owns (bench/mlperf only, doc/19 D-22).
function(pr3_target_warnings target)
  target_compile_options(${target} PRIVATE $<$<COMPILE_LANGUAGE:C>:${PR3_C_WARNINGS}>)
  get_property(languages GLOBAL PROPERTY ENABLED_LANGUAGES)
  if("CXX" IN_LIST languages AND NOT MSVC)
    if(NOT DEFINED CACHE{PR3_CXX_WARNINGS_CHECKED})
      set(CMAKE_REQUIRED_FLAGS "${CMAKE_REQUIRED_FLAGS} -Werror")  # function scope only
      set(found "")
      foreach(flag IN LISTS _pr3_cxx_warning_candidates)
        string(MAKE_C_IDENTIFIER "PR3_HAS_CXX${flag}" var)
        check_cxx_compiler_flag("${flag}" ${var})
        if(${var})
          list(APPEND found "${flag}")
        endif()
      endforeach()
      if(PR3_WARNINGS_AS_ERRORS)
        list(APPEND found -Werror)
      endif()
      set(PR3_CXX_WARNINGS_CHECKED "${found}" CACHE INTERNAL "C++ warning flags PR3 uses")
    endif()
    target_compile_options(${target} PRIVATE $<$<COMPILE_LANGUAGE:CXX>:${PR3_CXX_WARNINGS_CHECKED}>)
  elseif("CXX" IN_LIST languages)
    target_compile_options(${target} PRIVATE $<$<COMPILE_LANGUAGE:CXX>:${PR3_CXX_WARNINGS}>)
  endif()
endfunction()
