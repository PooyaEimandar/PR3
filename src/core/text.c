/* Text the core reshapes: trimming, one-line notes and briefs. White space is Unicode's
 * White_Space property, so a note cannot break out of its line with a line separator. */
#include "core_internal.h"

#include <pr3/poison.h>

bool core_is_space(uint32_t cp) {
    return (cp >= 0x09 && cp <= 0x0D) || cp == 0x20 || cp == 0x85 || cp == 0xA0 || cp == 0x1680 ||
           (cp >= 0x2000 && cp <= 0x200A) || cp == 0x2028 || cp == 0x2029 || cp == 0x202F || cp == 0x205F ||
           cp == 0x3000;
}

/* The code point at s[i] and its length; s is valid UTF-8. */
static size_t at(pr3_str s, size_t i, uint32_t *cp) {
    size_t n = pr3_utf8_decode((const uint8_t *)s.ptr + i, s.len - i, cp);
    PR3_DEBUG_ASSERT(n > 0);
    if (n == 0) { /* not valid UTF-8: treat the byte as a letter */
        *cp = 0xFFFD;
        n = 1;
    }
    return n;
}

pr3_str core_trim(pr3_str s) {
    size_t lo = 0, hi = s.len;
    while (lo < hi) {
        uint32_t cp;
        size_t n = at(s, lo, &cp);
        if (!core_is_space(cp)) break;
        lo += n;
    }
    while (hi > lo) {
        size_t start = hi - 1; /* back to the lead byte of the last character */
        while (start > lo && ((uint8_t)s.ptr[start] & 0xC0) == 0x80) start--;
        uint32_t cp;
        (void)at(s, start, &cp);
        if (!core_is_space(cp)) break;
        hi = start;
    }
    return (pr3_str){s.ptr + lo, hi - lo};
}

/* The next word of s at or after *i, or an empty string when none is left. */
static pr3_str next_word(pr3_str s, size_t *i) {
    size_t p = *i;
    while (p < s.len) {
        uint32_t cp;
        size_t n = at(s, p, &cp);
        if (!core_is_space(cp)) break;
        p += n;
    }
    size_t start = p;
    while (p < s.len) {
        uint32_t cp;
        size_t n = at(s, p, &cp);
        if (core_is_space(cp)) break;
        p += n;
    }
    *i = p;
    return (pr3_str){s.ptr + start, p - start};
}

size_t core_one_line_len(pr3_str s) {
    size_t i = 0, len = 0;
    bool first = true;
    for (;;) {
        pr3_str w = next_word(s, &i);
        if (w.len == 0) return len;
        len += w.len + (first ? 0 : 1);
        first = false;
    }
}

void core_put_one_line_clip(pr3_buf *b, pr3_str s, size_t max) {
    bool cut = core_one_line_len(s) > max;
    size_t room = max, i = 0;
    bool first = true;
    for (;;) {
        pr3_str w = next_word(s, &i);
        if (w.len == 0) break;
        if (!first) {
            if (room == 0) break;
            pr3_buf_put_char(b, ' ');
            room--;
        }
        first = false;
        if (w.len > room) {
            pr3_buf_put(b, pr3_str_clip(w, room));
            break;
        }
        pr3_buf_put(b, w);
        room -= w.len;
    }
    if (cut) pr3_buf_put(b, CORE_MARKER);
}
