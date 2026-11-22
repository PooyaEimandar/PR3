/* Arena: every allocation is inside the block, aligned, after the previous one, and the
 * accounting is exact; a failed allocation changes nothing; rewind restores the mark. */
#include "cbmc.h"

#define CAP 64
#define STEPS 6

int main(void) {
    static uint8_t mem[CAP];
    size_t cap = nondet_size();
    assume(cap <= CAP);
    pr3_arena a;
    pr3_arena_init(&a, mem, cap, "cbmc");
    uint8_t *prev_end = mem;
    pr3_arena_mark mark = pr3_arena_mark_get(&a);
    for (int i = 0; i < STEPS; i++) {
        size_t size = nondet_size();
        unsigned shift = nondet_unsigned();
        assume(shift < 5);
        size_t align = (size_t)1 << shift;
        size_t used = a.used;
        if (i == 2) mark = pr3_arena_mark_get(&a);
        uint8_t *p = pr3_arena_alloc(&a, size, align);
        if (p == NULL) {
            check(a.used == used, "a failed allocation changes nothing");
            continue;
        }
        check(p >= prev_end, "allocations are ordered and disjoint");
        check((size_t)(p - mem) + size <= cap, "allocations stay inside the block");
        check(((uintptr_t)p & (align - 1)) == 0, "allocations are aligned");
        check(a.used == (size_t)(p - mem) + size, "used ends at the last allocation");
        check(a.high_water >= a.used, "the high-water mark covers use");
        prev_end = p + size;
    }
    size_t high = a.high_water;
    if (mark.used <= a.used) {
        pr3_arena_rewind(&a, mark);
        check(a.used == mark.used, "rewind restores the mark");
        check(a.high_water == high, "rewind keeps the high-water mark");
    }
    pr3_arena_reset(&a);
    check(a.used == 0, "reset releases everything");
    return 0;
}
