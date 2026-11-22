/* Shared declarations for the CBMC harnesses. */
#ifndef PR3_CBMC_H
#define PR3_CBMC_H

#include <pr3/base.h>

/* CBMC treats undefined functions named nondet_* as returning any value. */
size_t nondet_size(void);
unsigned nondet_unsigned(void);
uint8_t nondet_u8(void);
bool nondet_bool(void);

void __CPROVER_assume(_Bool condition);
#define assume(c) __CPROVER_assume(c)
void __CPROVER_assert(_Bool condition, const char *description);
#define check(c, what) __CPROVER_assert((c), (what))

#endif
