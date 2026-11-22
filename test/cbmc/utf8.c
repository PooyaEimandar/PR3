/* UTF-8: decoding accepts exactly the well-formed sequences of the Unicode Standard,
 * table 3-7, and encoding a decoded value gives back the same bytes. */
#include "cbmc.h"

static bool cont(uint8_t b) { return b >= 0x80 && b <= 0xBF; }

/* Table 3-7, written as ranges. */
static size_t well_formed(const uint8_t *s, size_t n) {
    if (n >= 1 && s[0] <= 0x7F) return 1;
    if (n >= 2 && s[0] >= 0xC2 && s[0] <= 0xDF && cont(s[1])) return 2;
    if (n >= 3 && s[0] == 0xE0 && s[1] >= 0xA0 && s[1] <= 0xBF && cont(s[2])) return 3;
    if (n >= 3 && ((s[0] >= 0xE1 && s[0] <= 0xEC) || s[0] == 0xEE || s[0] == 0xEF) && cont(s[1]) &&
        cont(s[2]))
        return 3;
    if (n >= 3 && s[0] == 0xED && s[1] >= 0x80 && s[1] <= 0x9F && cont(s[2])) return 3;
    if (n >= 4 && s[0] == 0xF0 && s[1] >= 0x90 && s[1] <= 0xBF && cont(s[2]) && cont(s[3])) return 4;
    if (n >= 4 && s[0] >= 0xF1 && s[0] <= 0xF3 && cont(s[1]) && cont(s[2]) && cont(s[3])) return 4;
    if (n >= 4 && s[0] == 0xF4 && s[1] >= 0x80 && s[1] <= 0x8F && cont(s[2]) && cont(s[3])) return 4;
    return 0;
}

int main(void) {
    uint8_t s[4] = {nondet_u8(), nondet_u8(), nondet_u8(), nondet_u8()};
    size_t n = nondet_size();
    assume(n >= 1 && n <= 4);
    uint32_t cp = 0;
    size_t len = pr3_utf8_decode(s, n, &cp);
    check(len == well_formed(s, n), "decode accepts exactly table 3-7");
    if (len) {
        uint8_t back[4];
        size_t m = pr3_utf8_encode(cp, back);
        check(m == len, "encode gives the same length");
        for (size_t k = 0; k < m; k++) check(back[k] == s[k], "encode gives the same bytes");
    }
    check(pr3_utf8_valid_prefix(s, n) <= n, "the valid prefix is within the input");
    return 0;
}
