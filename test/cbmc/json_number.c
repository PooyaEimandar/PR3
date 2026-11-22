/* The JSON number path: for every input of up to six bytes from the number alphabet,
 * the scanner stays inside the input, reports where it stopped, keeps integers that fit
 * as integers with their exact value, and reads -0 as a negative zero.
 *
 * Inputs whose first exponent is between 10 and 399 are left out: they reach the exact
 * decimal, whose digit loops run hundreds of times and are beyond bounded model checking
 * at any useful depth. That path is covered by the number vectors, the random round
 * trips in test_json.c and fuzz_json. What stays in covers the integers, Clinger's fast
 * path, overflow to "out of range" and underflow to zero; tools/run-cbmc.sh bounds the
 * decimal's loops at zero, so the run fails if any input left in reaches them. */
#include "cbmc.h"

#include "../../src/json/json_internal.h"

static const char alphabet[] = "-0123456789.eE+";

int main(void) {
    uint8_t s[6];
    size_t n = nondet_size();
    assume(n >= 1 && n <= sizeof s);
    for (size_t i = 0; i < sizeof s; i++) {
        unsigned k = nondet_unsigned();
        assume(k < sizeof alphabet - 1);
        s[i] = (uint8_t)alphabet[k];
    }
    assume(s[0] == '-' || (s[0] >= '0' && s[0] <= '9'));
    size_t e = 0;
    while (e < n && s[e] != 'e' && s[e] != 'E') e++;
    if (e < n) {
        size_t i = e + 1;
        if (i < n && (s[i] == '+' || s[i] == '-')) i++;
        unsigned ev = 0;
        for (; i < n && s[i] >= '0' && s[i] <= '9'; i++) ev = ev * 10 + (unsigned)(s[i] - '0');
        assume(ev < 10 || ev >= 400);
    }

    const uint8_t *after = NULL;
    pr3_json_token tok;
    pr3_str reason = {0};
    pr3_status st = pr3_json_scan_number(s, s + n, &after, &tok, &reason);
    check(after >= s && after <= s + n, "the scanner stops inside the input");
    check(st == PR3_OK || st == PR3_E_INVALID, "a number is accepted or refused as invalid");
    if (st != PR3_OK) {
        check(reason.len > 0, "a refusal says why");
        return 0;
    }
    check(after > s, "an accepted number has at least one byte");
    check(tok.kind == PR3_JSON_U64 || tok.kind == PR3_JSON_I64 || tok.kind == PR3_JSON_F64, "a number kind");

    /* The bytes scanned: a plain integer stays an integer with its exact value. */
    size_t m = (size_t)(after - s);
    bool neg = s[0] == '-', plain = true;
    uint64_t v = 0;
    for (size_t i = neg ? 1 : 0; i < m; i++) {
        if (s[i] < '0' || s[i] > '9') {
            plain = false;
            break;
        }
        v = v * 10 + (uint64_t)(s[i] - '0');
    }
    if (plain && !neg) check(tok.kind == PR3_JSON_U64 && tok.v.u == v, "a non-negative integer is exact");
    if (plain && neg && v > 0)
        check(tok.kind == PR3_JSON_I64 && tok.v.i == -(int64_t)v, "a negative integer is exact");
    if (plain && neg && v == 0)
        check(tok.kind == PR3_JSON_F64 && tok.v.f == 0.0 && 1.0 / tok.v.f < 0, "-0 is a negative zero");
    if (!plain) check(tok.kind == PR3_JSON_F64, "a fraction or exponent makes a double");
    return 0;
}
