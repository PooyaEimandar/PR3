# Freestanding cross-builds of the portable libraries with upstream clang and lld: no C
# library, no threads, files, heap or network (doc/11, "The board contract"; FND-02,
# FND-04). PR3_FREESTANDING_TARGET is the clang triple, PR3_FREESTANDING_FLAGS the core.
set(CMAKE_SYSTEM_NAME Generic)
set(CMAKE_SYSTEM_PROCESSOR "${PR3_FREESTANDING_TARGET}")
set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)
list(APPEND CMAKE_TRY_COMPILE_PLATFORM_VARIABLES PR3_FREESTANDING_TARGET PR3_FREESTANDING_FLAGS)

set(_pr3_llvm_hints /opt/homebrew/opt/llvm/bin /usr/local/opt/llvm/bin /usr/lib/llvm-21/bin /usr/lib/llvm-20/bin /usr/lib/llvm-19/bin /usr/lib/llvm-18/bin)
find_program(PR3_LLVM_CLANG NAMES clang HINTS ${_pr3_llvm_hints} REQUIRED)
find_program(PR3_LLVM_AR NAMES llvm-ar HINTS ${_pr3_llvm_hints} REQUIRED)
find_program(PR3_LLVM_RANLIB NAMES llvm-ranlib HINTS ${_pr3_llvm_hints} REQUIRED)
find_program(PR3_LLVM_NM NAMES llvm-nm HINTS ${_pr3_llvm_hints} REQUIRED)
find_program(PR3_LLVM_SIZE NAMES llvm-size HINTS ${_pr3_llvm_hints})
find_program(PR3_LLD NAMES ld.lld HINTS /opt/homebrew/opt/lld/bin /usr/local/opt/lld/bin ${_pr3_llvm_hints} REQUIRED)

set(CMAKE_C_COMPILER "${PR3_LLVM_CLANG}")
set(CMAKE_C_COMPILER_TARGET "${PR3_FREESTANDING_TARGET}")
set(CMAKE_AR "${PR3_LLVM_AR}")
set(CMAKE_RANLIB "${PR3_LLVM_RANLIB}")
set(CMAKE_C_FLAGS_INIT "${PR3_FREESTANDING_FLAGS} -ffreestanding -fno-builtin-malloc -ffunction-sections -fdata-sections")
set(PR3_FREESTANDING ON CACHE BOOL "Freestanding build: portable libraries and the board demo only" FORCE)
