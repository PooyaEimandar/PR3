/* Slices, decimal parsing and builders. */
#include <pr3/base.h>

#include <pr3/poison.h>

int pr3_str_cmp_rest(pr3_str a, pr3_str b) {
    size_t n = a.len < b.len ? a.len : b.len;
    int c = n ? memcmp(a.ptr, b.ptr, n) : 0;
    if (c != 0) return c;
    return a.len < b.len ? -1 : a.len > b.len ? 1 : 0;
}

pr3_status pr3_str_from_bytes(pr3_bytes b, pr3_str *out) {
    if (b.len && pr3_utf8_valid_prefix(b.ptr, b.len) != b.len) return PR3_E_UTF8;
    *out = (pr3_str){(const char *)b.ptr, b.len};
    return PR3_OK;
}

pr3_str pr3_str_clip(pr3_str s, size_t max) {
    if (s.len <= max) return s;
    size_t n = max;
    /* s is valid UTF-8, so stepping back over continuation bytes reaches a lead byte
     * within three steps */
    while (n > 0 && ((uint8_t)s.ptr[n] & 0xC0) == 0x80) n--;
    return (pr3_str){s.ptr, n};
}

pr3_status pr3_parse_u64(pr3_str s, uint64_t *out) {
    if (s.len == 0 || (s.len > 1 && s.ptr[0] == '0')) return PR3_E_INVALID;
    uint64_t v = 0;
    bool over = false;
    for (size_t i = 0; i < s.len; i++) {
        uint8_t c = (uint8_t)s.ptr[i];
        if (c < '0' || c > '9') return PR3_E_INVALID;
        unsigned d = (unsigned)(c - '0');
        if (!over && (pr3_ckd_mul_u64(&v, v, 10) || pr3_ckd_add_u64(&v, v, d))) over = true;
    }
    if (over) return PR3_E_LIMIT;
    *out = v;
    return PR3_OK;
}

pr3_status pr3_buf_from_arena(pr3_arena *a, size_t cap, pr3_buf *out) {
    char *mem = pr3_arena_alloc(a, cap, 1);
    if (mem == NULL) return PR3_E_NOMEM;
    *out = pr3_buf_make(mem, cap);
    return PR3_OK;
}

void pr3_buf_put_hex(pr3_buf *b, pr3_bytes bytes) {
    static const char digits[16] = {'0', '1', '2', '3', '4', '5', '6', '7',
                                    '8', '9', 'a', 'b', 'c', 'd', 'e', 'f'};
    size_t n;
    if (bytes.len == 0) return;
    if (pr3_ckd_mul(&n, bytes.len, 2)) {
        b->overflow = true;
        return;
    }
    char *dst = pr3_buf_reserve(b, n);
    if (dst == NULL) return;
    for (size_t i = 0; i < bytes.len; i++) {
        dst[2 * i] = digits[bytes.ptr[i] >> 4];
        dst[2 * i + 1] = digits[bytes.ptr[i] & 15];
    }
}

void pr3_buf_put_clip(pr3_buf *b, pr3_str s, size_t max) {
    pr3_str head = pr3_str_clip(s, max);
    pr3_buf_put(b, head);
    if (head.len < s.len) pr3_buf_put(b, PR3_S(PR3_TRUNCATED_MARKER));
}
