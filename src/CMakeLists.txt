# The portable libraries, one static library per module, in layer order
# (doc/02-architecture.md#modules). Modules join as their issues land.
add_subdirectory(base)
add_subdirectory(json)
add_subdirectory(core)
