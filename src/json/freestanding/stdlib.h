/* See inttypes.h here. Only Ryu's d2s(), which PR3 never calls, uses malloc; the link
 * drops it with --gc-sections, and the board's symbol list proves malloc is absent. */
#ifndef PR3_RYU_SHIM_STDLIB_H
#define PR3_RYU_SHIM_STDLIB_H
#include <stddef.h>
void *malloc(size_t size);
#endif
