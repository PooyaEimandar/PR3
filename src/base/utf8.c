/* UTF-8 validation, decoding and encoding to the well-formed byte sequences of the
 * Unicode Standard, table 3-7. */
#include <pr3/base.h>

#include <pr3/poison.h>

size_t pr3_utf8_decode(const uint8_t *p, size_t n, uint32_t *cp) {
    if (n == 0) return 0;
    uint32_t b0 = p[0];
    if (b0 < 0x80) {
        *cp = b0;
        return 1;
    }
    if (b0 < 0xC2) return 0; /* a continuation byte, or an overlong two-byte lead */
    if (b0 < 0xE0) {
        if (n < 2 || (p[1] & 0xC0) != 0x80) return 0;
        *cp = (b0 & 0x1Fu) << 6 | (p[1] & 0x3Fu);
        return 2;
    }
    if (b0 < 0xF0) {
        if (n < 3) return 0;
        uint32_t b1 = p[1], b2 = p[2];
        uint32_t lo = b0 == 0xE0 ? 0xA0 : 0x80; /* no overlong forms */
        uint32_t hi = b0 == 0xED ? 0x9F : 0xBF; /* no surrogates */
        if (b1 < lo || b1 > hi || (b2 & 0xC0) != 0x80) return 0;
        *cp = (b0 & 0x0Fu) << 12 | (b1 & 0x3Fu) << 6 | (b2 & 0x3Fu);
        return 3;
    }
    if (b0 < 0xF5) {
        if (n < 4) return 0;
        uint32_t b1 = p[1], b2 = p[2], b3 = p[3];
        uint32_t lo = b0 == 0xF0 ? 0x90 : 0x80; /* no overlong forms */
        uint32_t hi = b0 == 0xF4 ? 0x8F : 0xBF; /* nothing above U+10FFFF */
        if (b1 < lo || b1 > hi || (b2 & 0xC0) != 0x80 || (b3 & 0xC0) != 0x80) return 0;
        *cp = (b0 & 0x07u) << 18 | (b1 & 0x3Fu) << 12 | (b2 & 0x3Fu) << 6 | (b3 & 0x3Fu);
        return 4;
    }
    return 0;
}

size_t pr3_utf8_valid_prefix(const uint8_t *p, size_t n) {
    size_t i = 0;
    while (i < n) {
        /* ASCII runs, eight bytes at a time */
        while (n - i >= 8) {
            uint64_t w;
            memcpy(&w, p + i, 8);
            if (w & UINT64_C(0x8080808080808080)) break;
            i += 8;
        }
        if (i == n) break;
        if (p[i] < 0x80) {
            i++;
            continue;
        }
        uint32_t cp;
        size_t len = pr3_utf8_decode(p + i, n - i, &cp);
        if (len == 0) return i;
        i += len;
    }
    return n;
}

size_t pr3_utf8_encode(uint32_t cp, uint8_t out[4]) {
    if (cp < 0x80) {
        out[0] = (uint8_t)cp;
        return 1;
    }
    if (cp < 0x800) {
        out[0] = (uint8_t)(0xC0 | cp >> 6);
        out[1] = (uint8_t)(0x80 | (cp & 0x3F));
        return 2;
    }
    if (cp >= 0xD800 && cp <= 0xDFFF) return 0;
    if (cp < 0x10000) {
        out[0] = (uint8_t)(0xE0 | cp >> 12);
        out[1] = (uint8_t)(0x80 | (cp >> 6 & 0x3F));
        out[2] = (uint8_t)(0x80 | (cp & 0x3F));
        return 3;
    }
    if (cp <= 0x10FFFF) {
        out[0] = (uint8_t)(0xF0 | cp >> 18);
        out[1] = (uint8_t)(0x80 | (cp >> 12 & 0x3F));
        out[2] = (uint8_t)(0x80 | (cp >> 6 & 0x3F));
        out[3] = (uint8_t)(0x80 | (cp & 0x3F));
        return 4;
    }
    return 0;
}
