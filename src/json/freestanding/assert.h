/* See inttypes.h here. Ryu's asserts check its own arithmetic; boards build with
 * NDEBUG, as hosted release builds do. */
#ifndef PR3_RYU_SHIM_ASSERT_H
#define PR3_RYU_SHIM_ASSERT_H
#define assert(x) ((void)0)
#endif
