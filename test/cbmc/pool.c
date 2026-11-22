/* Pool: a handle resolves exactly while its slot is live, releasing twice fails, and the
 * live count matches the handles held. */
#include "cbmc.h"

#define CAP 3
#define STEPS 7

int main(void) {
    static uint8_t mem[512];
    pr3_arena a;
    pr3_arena_init(&a, mem, sizeof mem, "cbmc");
    pr3_pool p;
    if (pr3_pool_init(&p, &a, 8, 8, CAP) != PR3_OK) return 0;
    pr3_handle held[STEPS];
    bool live[STEPS];
    unsigned n = 0, n_live = 0;
    for (int i = 0; i < STEPS; i++) {
        if (nondet_bool() || n == 0) {
            pr3_handle h;
            void *slot = pr3_pool_acquire(&p, &h);
            check((slot == NULL) == (n_live == CAP), "acquire fails exactly when full");
            if (slot) {
                held[n] = h;
                live[n] = true;
                n++;
                n_live++;
            }
        } else {
            unsigned k = nondet_unsigned();
            assume(k < n);
            pr3_status s = pr3_pool_release(&p, held[k]);
            check((s == PR3_OK) == live[k], "release succeeds exactly once per acquire");
            if (s == PR3_OK) {
                live[k] = false;
                n_live--;
            }
        }
        for (unsigned k = 0; k < n; k++)
            check((pr3_pool_get(&p, held[k]) != NULL) == live[k], "a handle resolves exactly while live");
        check(p.live == n_live, "the live count is exact");
    }
    return 0;
}
