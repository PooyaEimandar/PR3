/* JSON numbers. Integers that fit uint64_t or int64_t stay integers; -0 and every other
 * number become the correctly rounded double (round half to even), so the result never
 * depends on the compiler or the C library.
 *
 * Most doubles take Clinger's fast path: at most 19 significant digits with a value up
 * to 2^53, scaled by an exact power of ten up to 10^22, which one IEEE operation rounds
 * correctly. The rest go through an exact decimal: up to 800 significant digits, shifted
 * by powers of two until the binary exponent and 53 bits of mantissa can be read off,
 * then rounded; digits past the 800th only matter at an exact halfway point, which the
 * `trunc` flag settles. 800 digits cover the 767 a double can ever need. */
#include "json_internal.h"

#include <float.h>

#include <pr3/poison.h>

#define DEC_DIGITS 800u
#define DEC_SLACK 20u /* room for a left shift's new digits before truncation */
#define MAX_SHIFT 60u /* 9 * 2^60 plus a carry below 2^60 fits in 64 bits */
/* An exponent stops accumulating at 10^15: no input has that many digits, so a larger
 * exponent overflows or underflows whatever the digits are. */
#define EXP_CAP INT64_C(1000000000000000)
#define DP_CLAMP 400 /* decimal_to_double gives infinity above 310 and zero below -330 */

typedef struct decimal {
    uint8_t d[DEC_DIGITS + DEC_SLACK]; /* digit values, most significant first */
    size_t nd;                         /* digits in use, at most DEC_DIGITS */
    int32_t dp;                        /* the value is 0.d[0]d[1]... * 10^dp */
    bool trunc;                        /* non-zero digits were dropped after the last */
} decimal;

static bool is_digit(uint8_t c) { return c >= '0' && c <= '9'; }

static void trim(decimal *a) {
    while (a->nd > 0 && a->d[a->nd - 1] == 0) a->nd--;
    if (a->nd == 0) a->dp = 0;
}

/* a *= 2^-k, k <= MAX_SHIFT */
static void right_shift(decimal *a, unsigned k) {
    size_t r = 0, w = 0;
    uint64_t n = 0;
    for (; (n >> k) == 0; r++) {
        if (r >= a->nd) {
            if (n == 0) {
                a->nd = 0;
                a->dp = 0;
                return;
            }
            while ((n >> k) == 0) {
                n *= 10;
                r++;
            }
            break;
        }
        n = n * 10 + a->d[r];
    }
    a->dp -= (int32_t)r - 1;
    uint64_t mask = ((uint64_t)1 << k) - 1;
    for (; r < a->nd; r++) { /* w < r throughout: digits are written behind the reader */
        uint64_t dig = n >> k;
        n &= mask;
        a->d[w++] = (uint8_t)dig;
        n = n * 10 + a->d[r];
    }
    while (n > 0) {
        uint64_t dig = n >> k;
        n &= mask;
        if (w < DEC_DIGITS)
            a->d[w++] = (uint8_t)dig;
        else if (dig > 0)
            a->trunc = true;
        n = n * 10;
    }
    a->nd = w;
    trim(a);
}

/* a *= 2^k, k <= MAX_SHIFT. Digits are produced from the least significant end into the
 * slack after the number, 19 places ahead of the reader, then moved to the front. */
static void left_shift(decimal *a, unsigned k) {
    size_t r = a->nd, end = a->nd + 19, w = end;
    uint64_t n = 0;
    while (r > 0) {
        r--;
        n += (uint64_t)a->d[r] << k;
        uint64_t quo = n / 10;
        a->d[--w] = (uint8_t)(n - 10 * quo);
        n = quo;
    }
    while (n > 0) { /* the carry is below 2^60: at most 19 more digits */
        uint64_t quo = n / 10;
        a->d[--w] = (uint8_t)(n - 10 * quo);
        n = quo;
    }
    size_t nd = end - w;
    a->dp += (int32_t)(nd - a->nd);
    memmove(a->d, a->d + w, nd);
    if (nd > DEC_DIGITS) {
        for (size_t i = DEC_DIGITS; i < nd; i++)
            if (a->d[i] != 0) a->trunc = true;
        nd = DEC_DIGITS;
    }
    a->nd = nd;
    trim(a);
}

static void shift(decimal *a, int k) {
    if (a->nd == 0) return;
    while (k > (int)MAX_SHIFT) {
        left_shift(a, MAX_SHIFT);
        k -= (int)MAX_SHIFT;
    }
    while (k < -(int)MAX_SHIFT) {
        right_shift(a, MAX_SHIFT);
        k += (int)MAX_SHIFT;
    }
    if (k > 0) left_shift(a, (unsigned)k);
    if (k < 0) right_shift(a, (unsigned)-k);
}

/* Whether rounding a to its first nd digits goes up: half to even, and up at an exact
 * half when digits were dropped (the true value is then above the half). */
static bool should_round_up(const decimal *a, int32_t nd) {
    if (nd < 0 || (size_t)nd >= a->nd) return false;
    if (a->d[nd] == 5 && (size_t)nd + 1 == a->nd) {
        if (a->trunc) return true;
        return nd > 0 && a->d[nd - 1] % 2 == 1;
    }
    return a->d[nd] >= 5;
}

static uint64_t rounded_integer(const decimal *a) {
    if (a->dp > 20) return UINT64_MAX;
    uint64_t n = 0;
    int32_t i = 0;
    for (; i < a->dp && (size_t)i < a->nd; i++) n = n * 10 + a->d[i];
    for (; i < a->dp; i++) n *= 10;
    if (should_round_up(a, a->dp)) n++;
    return n;
}

/* The double nearest a (whose sign is applied by the caller); false on overflow. */
static bool decimal_to_double(decimal *a, double *out) {
    static const uint8_t powtab[] = {1, 3, 6, 9, 13, 16, 19, 23, 26};
    const int bias = -1023;
    int exp = 0;
    uint64_t mant = 0;
    if (a->nd == 0 || a->dp < -330) {
        exp = bias; /* zero */
    } else {
        if (a->dp > 310) return false;
        while (a->dp > 0) { /* scale down into [0.5, 1) */
            int n = a->dp >= (int32_t)sizeof powtab ? 27 : powtab[a->dp];
            shift(a, -n);
            exp += n;
        }
        while (a->dp < 0 || (a->dp == 0 && a->d[0] < 5)) {
            int n = -a->dp >= (int32_t)sizeof powtab ? 27 : powtab[-a->dp];
            shift(a, n);
            exp -= n;
        }
        exp--;                /* [0.5, 1) is [1, 2) times 2^-1 */
        if (exp < bias + 1) { /* below the normal range: denormalize */
            int n = bias + 1 - exp;
            shift(a, -n);
            exp += n;
        }
        if (exp - bias >= 0x7FF) return false;
        shift(a, 53); /* one integer bit and 52 fraction bits */
        mant = rounded_integer(a);
        if (mant == (uint64_t)2 << 52) { /* rounding carried into a new bit */
            mant >>= 1;
            exp++;
            if (exp - bias >= 0x7FF) return false;
        }
        if ((mant & ((uint64_t)1 << 52)) == 0) exp = bias; /* subnormal */
    }
    uint64_t bits = (mant & (((uint64_t)1 << 52) - 1)) | (uint64_t)((exp - bias) & 0x7FF) << 52;
    memcpy(out, &bits, sizeof bits);
    return true;
}

/* The exact decimal of the number text [p, end), already known to be well formed. */
static void decimal_from_text(decimal *a, const uint8_t *p, const uint8_t *end) {
    a->nd = 0;
    a->dp = 0;
    a->trunc = false;
    bool frac = false;
    int64_t dp = 0, exp = 0; /* 64 bits: an input can have more than 2^31 digits */
    if (p < end && *p == '-') p++;
    for (; p < end; p++) {
        uint8_t c = *p;
        if (c == '.') {
            frac = true;
            continue;
        }
        if (!is_digit(c)) break; /* the exponent */
        uint8_t v = (uint8_t)(c - '0');
        if (a->nd == 0 && v == 0) { /* leading zeros */
            if (frac) dp--;
            continue;
        }
        if (!frac) dp++;
        if (a->nd < DEC_DIGITS)
            a->d[a->nd++] = v;
        else if (v != 0)
            a->trunc = true;
    }
    if (p < end) { /* e or E, an optional sign, digits */
        p++;
        bool neg = false;
        if (*p == '+' || *p == '-') neg = *p++ == '-';
        for (; p < end; p++)
            if (exp < EXP_CAP) exp = exp * 10 + (*p - '0');
        dp += neg ? -exp : exp;
    }
    a->dp = dp > DP_CLAMP ? DP_CLAMP : dp < -DP_CLAMP ? -DP_CLAMP : (int32_t)dp;
    trim(a);
}

pr3_status pr3_json_scan_number(const uint8_t *p, const uint8_t *end, const uint8_t **after,
                                pr3_json_token *tok, pr3_str *reason) {
    static const double pow10[] = {1e0,  1e1,  1e2,  1e3,  1e4,  1e5,  1e6,  1e7,  1e8,  1e9,  1e10, 1e11,
                                   1e12, 1e13, 1e14, 1e15, 1e16, 1e17, 1e18, 1e19, 1e20, 1e21, 1e22};
    const uint8_t *start = p;
    bool neg = *p == '-';
    if (neg) p++;
    if (p == end || !is_digit(*p)) {
        *after = p;
        *reason = PR3_S("expected a digit");
        return PR3_E_INVALID;
    }

    /* Grammar, the significant digits as an integer w (the first 19), how many there
     * are, and the count of fraction digits. */
    uint64_t w = 0;
    unsigned nsig = 0;
    bool int_overflow = false; /* the integer part alone does not fit uint64_t */
    uint64_t ival = 0;
    if (*p == '0') {
        p++;
        if (p < end && is_digit(*p)) {
            *after = p;
            *reason = PR3_S("leading zero");
            return PR3_E_INVALID;
        }
    } else {
        for (; p < end && is_digit(*p); p++) {
            unsigned v = (unsigned)(*p - '0');
            if (!int_overflow && (pr3_ckd_mul_u64(&ival, ival, 10) || pr3_ckd_add_u64(&ival, ival, v)))
                int_overflow = true;
            if (nsig < 19) w = w * 10 + v;
            if (nsig < 20) nsig++; /* only "more than 19" matters; never wraps */
        }
    }
    bool is_int = true;
    int64_t frac_digits = 0;
    if (p < end && *p == '.') {
        is_int = false;
        p++;
        if (p == end || !is_digit(*p)) {
            *after = p;
            *reason = PR3_S("expected a digit after the decimal point");
            return PR3_E_INVALID;
        }
        for (; p < end && is_digit(*p); p++) {
            unsigned v = (unsigned)(*p - '0');
            frac_digits++;
            if (nsig == 0 && v == 0) continue; /* leading zeros are not significant */
            if (nsig < 19) w = w * 10 + v;
            if (nsig < 20) nsig++; /* only "more than 19" matters; never wraps */
        }
    }
    int64_t exp = 0;
    if (p < end && (*p == 'e' || *p == 'E')) {
        is_int = false;
        p++;
        bool eneg = false;
        if (p < end && (*p == '+' || *p == '-')) eneg = *p++ == '-';
        if (p == end || !is_digit(*p)) {
            *after = p;
            *reason = PR3_S("expected a digit in the exponent");
            return PR3_E_INVALID;
        }
        for (; p < end && is_digit(*p); p++)
            if (exp < EXP_CAP) exp = exp * 10 + (*p - '0');
        if (eneg) exp = -exp;
    }
    *after = p;

    if (is_int && !int_overflow) {
        if (!neg) {
            *tok = (pr3_json_token){.kind = PR3_JSON_U64, .v.u = ival};
            return PR3_OK;
        }
        if (ival != 0 && ival <= (uint64_t)INT64_MAX + 1) {
            int64_t v = ival == (uint64_t)INT64_MAX + 1 ? INT64_MIN : -(int64_t)ival;
            *tok = (pr3_json_token){.kind = PR3_JSON_I64, .v.i = v};
            return PR3_OK;
        }
        /* -0 and integers below INT64_MIN become doubles */
    }

    double d;
    int64_t e10 = exp - frac_digits; /* the value is W * 10^e10 */
    if (nsig == 0) {
        d = 0.0;
#if FLT_EVAL_METHOD == 0
    } else if (nsig <= 19 && w <= ((uint64_t)1 << 53) && e10 >= -22 && e10 <= 22) {
        d = (double)w; /* exact */
        d = e10 < 0 ? d / pow10[-e10] : d * pow10[e10];
    } else if (nsig <= 19 && e10 > 22 && e10 <= 22 + 15 && w <= ((uint64_t)1 << 53) &&
               w <= (((uint64_t)1 << 53) / (uint64_t)pow10[e10 - 22])) {
        d = (double)(w * (uint64_t)pow10[e10 - 22]) * 1e22; /* the product is exact and at most 2^53 */
#endif
    } else {
        decimal dec;
        decimal_from_text(&dec, start, p);
        if (!decimal_to_double(&dec, &d)) {
            *after = start;
            *reason = PR3_S("number out of range");
            return PR3_E_INVALID;
        }
    }
    *tok = (pr3_json_token){.kind = PR3_JSON_F64, .v.f = neg ? -d : d};
    return PR3_OK;
}
