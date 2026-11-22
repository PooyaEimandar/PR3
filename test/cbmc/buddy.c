/* Buddy heap: live blocks never overlap, stay inside the region, are aligned to the
 * smallest block, and the used count equals the sum of their block sizes. */
#include "cbmc.h"

#define HEAP 512 /* eight 64-byte blocks */
#define STEPS 6

int main(void) {
    static uint8_t mem[HEAP + 256];
    pr3_arena a;
    pr3_arena_init(&a, mem, sizeof mem, "cbmc");
    pr3_buddy b;
    if (pr3_buddy_init(&b, &a, HEAP, "cbmc") != PR3_OK) return 0;
    uint8_t *ptr[STEPS];
    size_t size[STEPS];
    unsigned n = 0;
    for (int i = 0; i < STEPS; i++) {
        if (nondet_bool() || n == 0) {
            size_t want = nondet_size();
            assume(want <= HEAP + 1);
            uint8_t *p = pr3_buddy_alloc(&b, want);
            if (p == NULL) continue;
            check(p >= b.base && (size_t)(p - b.base) + want <= HEAP, "blocks stay inside the region");
            check(((size_t)(p - b.base) & 63) == 0, "blocks are aligned to the smallest block");
            ptr[n] = p;
            size[n] = pr3_buddy_block_size(want);
            n++;
        } else {
            unsigned k = nondet_unsigned();
            assume(k < n);
            pr3_buddy_free(&b, ptr[k]);
            ptr[k] = ptr[n - 1];
            size[k] = size[n - 1];
            n--;
        }
        size_t used = 0;
        for (unsigned x = 0; x < n; x++) {
            used += size[x];
            for (unsigned y = x + 1; y < n; y++)
                check(ptr[x] + size[x] <= ptr[y] || ptr[y] + size[y] <= ptr[x], "live blocks never overlap");
        }
        check(b.used == used, "used is the sum of the live blocks");
    }
    return 0;
}
