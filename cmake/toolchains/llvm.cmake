# Upstream LLVM's clang rather than the system's: libFuzzer, and the Arm and RISC-V
# targets Apple's clang does not ship. Homebrew's keg is searched first on macOS.
set(_pr3_llvm_hints /opt/homebrew/opt/llvm/bin /usr/local/opt/llvm/bin /usr/lib/llvm-21/bin /usr/lib/llvm-20/bin /usr/lib/llvm-19/bin /usr/lib/llvm-18/bin)
find_program(PR3_LLVM_CLANG NAMES clang HINTS ${_pr3_llvm_hints} REQUIRED)
find_program(PR3_LLVM_CLANGXX NAMES clang++ HINTS ${_pr3_llvm_hints})
set(CMAKE_C_COMPILER "${PR3_LLVM_CLANG}")
if(PR3_LLVM_CLANGXX)
  set(CMAKE_CXX_COMPILER "${PR3_LLVM_CLANGXX}")
endif()
