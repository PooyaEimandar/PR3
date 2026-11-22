/* Ring, one thread: first in, first out; reserve fails exactly when full and peek exactly
 * when empty. (Cross-thread ordering is TSan's job, test_ring_transfers_across_threads.) */
#include "cbmc.h"

#define CAP 4
#define STEPS 10

int main(void) {
    static uint8_t mem[256];
    pr3_arena a;
    pr3_arena_init(&a, mem, sizeof mem, "cbmc");
    pr3_ring r;
    if (pr3_ring_init(&r, &a, sizeof(unsigned), _Alignof(unsigned), CAP) != PR3_OK) return 0;
    unsigned next_in = 0, next_out = 0;
    for (int i = 0; i < STEPS; i++) {
        if (nondet_bool()) {
            unsigned *slot = pr3_ring_reserve(&r);
            check((slot == NULL) == (next_in - next_out == CAP), "reserve fails exactly when full");
            if (slot) {
                *slot = next_in++;
                pr3_ring_publish(&r);
            }
        } else {
            unsigned *slot = pr3_ring_peek(&r);
            check((slot == NULL) == (next_in == next_out), "peek fails exactly when empty");
            if (slot) {
                check(*slot == next_out, "first in, first out");
                next_out++;
                pr3_ring_consume(&r);
            }
        }
    }
    return 0;
}
