/* Builder: the length never passes the capacity, overflow is sticky, and what was
 * written before the overflow is exactly the bytes that fit. */
#include "cbmc.h"

#define CAP 8
#define STEPS 4

int main(void) {
    char mem[CAP];
    size_t cap = nondet_size();
    assume(cap <= CAP);
    pr3_buf b = pr3_buf_make(mem, cap);
    char expect[CAP * STEPS];
    size_t total = 0;
    bool overflowed = false;
    for (int i = 0; i < STEPS; i++) {
        char piece[3] = {(char)('a' + i), (char)('A' + i), (char)('0' + i)};
        size_t n = nondet_size();
        assume(n <= 3);
        size_t before = b.len;
        pr3_buf_put_bytes(&b, piece, n);
        if (!overflowed && total + n <= cap) {
            for (size_t k = 0; k < n; k++) expect[total + k] = piece[k];
            total += n;
        } else if (n > 0) {
            overflowed = true;
        }
        check(b.len <= b.cap, "the length never passes the capacity");
        check(b.overflow == overflowed, "overflow is set exactly when a write did not fit, and stays set");
        check(!overflowed || b.len == before || b.len == total, "nothing is written after an overflow");
    }
    check(b.len == total, "the length is what fitted");
    for (size_t k = 0; k < total; k++) check(mem[k] == expect[k], "the bytes are what was written");
    pr3_str out;
    check((pr3_buf_take(&b, &out) == PR3_OK) == !overflowed, "take reports the overflow");
    return 0;
}
