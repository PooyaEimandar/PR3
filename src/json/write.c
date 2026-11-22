/* The canonical writer, straight from the tape: an explicit stack of open containers,
 * object keys sorted in byte order with the last duplicate kept. */
#include "json_internal.h"

#include <pr3/poison.h>

/* Ryu's shortest round-trip digits (third_party/ryu/ryu/ryu.h, declared here so this file
 * needs no hosted header): "[-]D[.DDD]E[-]X", returning its length. */
int d2s_buffered_n(double f, char *result);

/* ---- strings */

static const char hex_digit[16] = {'0', '1', '2', '3', '4', '5', '6', '7',
                                   '8', '9', 'a', 'b', 'c', 'd', 'e', 'f'};

/* The letter after the backslash of each short escape; the other control bytes are
 * written \u00XX. */
static const char escape_of[256] = {
    ['\b'] = 'b', ['\t'] = 't', ['\n'] = 'n', ['\f'] = 'f', ['\r'] = 'r', ['"'] = '"', ['\\'] = '\\',
};

#define ONES UINT64_C(0x0101010101010101)
#define HIGHS UINT64_C(0x8080808080808080)

/* For eight bytes read little-endian, the top bit of each that is a control byte, a quote
 * or a backslash. Bytes above the first such one may be flagged falsely (a borrow runs
 * upward), but the lowest flag is always exact, which is all the writer uses. */
PR3_WRAPPING static inline uint64_t escape_flags(uint64_t w) {
    uint64_t quote = w ^ (ONES * '"'), slash = w ^ (ONES * '\\');
    uint64_t low = (w - ONES * 0x20) & ~w;
    uint64_t q = (quote - ONES) & ~quote, s = (slash - ONES) & ~slash;
    return (low | q | s) & HIGHS;
}

static inline char *put_escaped(char *p, uint8_t c) {
    char e = escape_of[c];
    if (e == 0) {
        if (c >= 0x20) {
            *p++ = (char)c;
            return p;
        }
        e = 'u';
    }
    *p++ = '\\';
    *p++ = e;
    if (e == 'u') {
        *p++ = '0';
        *p++ = '0';
        *p++ = hex_digit[c >> 4];
        *p++ = hex_digit[c & 15];
    }
    return p;
}

char *pr3_json_escape(char *dst, pr3_str s) {
    char *p = dst;
    const uint8_t *q = (const uint8_t *)s.ptr, *const end = q + s.len;
    /* Eight bytes are copied as they are, then the cursor moves past those before the
     * first that needs an escape. The copy fits: at least 6 bytes of room remain for each
     * byte still to read, and at least 8 are still to read. */
    while (end - q >= 8) {
        uint64_t w = pr3_load_le64(q), hit = escape_flags(w);
        pr3_store_le64(p, w);
        if (hit == 0) {
            p += 8;
            q += 8;
            continue;
        }
        unsigned k = pr3_ctz64(hit) / 8;
        p += k;
        q += k;
        p = put_escaped(p, *q++);
    }
    while (q < end) p = put_escaped(p, *q++);
    return p;
}

void pr3_json_put_string_tight(pr3_buf *b, pr3_str s) {
    pr3_buf_put_char(b, '"');
    size_t run = 0; /* bytes copied as they are, flushed before each escape */
    for (size_t i = 0; i < s.len; i++) {
        uint8_t c = (uint8_t)s.ptr[i];
        if (c >= 0x20 && c != '"' && c != '\\') continue;
        pr3_buf_put_bytes(b, s.ptr + run, i - run);
        run = i + 1;
        char esc[6] = {'\\', 0, 0, 0, 0, 0};
        size_t n = 2;
        switch (c) {
        case '"': esc[1] = '"'; break;
        case '\\': esc[1] = '\\'; break;
        case '\b': esc[1] = 'b'; break;
        case '\f': esc[1] = 'f'; break;
        case '\n': esc[1] = 'n'; break;
        case '\r': esc[1] = 'r'; break;
        case '\t': esc[1] = 't'; break;
        default:
            esc[1] = 'u';
            esc[2] = '0';
            esc[3] = '0';
            esc[4] = hex_digit[c >> 4];
            esc[5] = hex_digit[c & 15];
            n = 6;
            break;
        }
        pr3_buf_put_bytes(b, esc, n);
    }
    pr3_buf_put_bytes(b, s.ptr + run, s.len - run);
    pr3_buf_put_char(b, '"');
}

/* ---- doubles: Ryu's shortest digits, laid out as doc/04 says */

static void put_zeros(pr3_buf *b, size_t n) {
    for (size_t i = 0; i < n; i++) pr3_buf_put_char(b, '0');
}

static void put_exponent(pr3_buf *b, int32_t e) {
    pr3_buf_put_char(b, 'e');
    pr3_buf_put_i64(b, e);
}

pr3_status pr3_json_put_f64(pr3_buf *b, double v) {
    uint64_t bits;
    memcpy(&bits, &v, sizeof bits);
    if ((bits >> 52 & 0x7FF) == 0x7FF) return PR3_E_INVALID; /* infinity or NaN */

    /* d2s writes "[-]D[.DDD]E[-]X": the shortest round-trip digits in scientific form. */
    char sci[32];
    int len = d2s_buffered_n(v, sci);
    PR3_CHECK(len > 0 && (size_t)len < sizeof sci);
    size_t i = 0, end = (size_t)len;
    bool neg = sci[0] == '-';
    if (neg) i++;
    char digits[20];
    size_t nd = 0;
    for (; i < end && sci[i] != 'E'; i++)
        if (sci[i] != '.' && nd < sizeof digits) digits[nd++] = sci[i];
    PR3_CHECK(i < end && nd > 0);
    i++; /* 'E' */
    bool eneg = sci[i] == '-';
    if (eneg) i++;
    int32_t e = 0;
    for (; i < end; i++) e = e * 10 + (sci[i] - '0');
    if (eneg) e = -e;

    if (neg) pr3_buf_put_char(b, '-');
    if (nd == 1 && digits[0] == '0') { /* zero: d2s writes "0E0" */
        pr3_buf_put_bytes(b, "0.0", 3);
        return PR3_OK;
    }
    /* The value is 0.DIGITS * 10^kk. */
    int32_t kk = e + 1, n = (int32_t)nd;
    if (kk >= n && kk <= 16) { /* 1234e7 -> 12340000000.0 */
        pr3_buf_put_bytes(b, digits, nd);
        put_zeros(b, (size_t)(kk - n));
        pr3_buf_put_bytes(b, ".0", 2);
    } else if (kk > 0 && kk <= 16) { /* 1234e-2 -> 12.34 */
        pr3_buf_put_bytes(b, digits, (size_t)kk);
        pr3_buf_put_char(b, '.');
        pr3_buf_put_bytes(b, digits + kk, nd - (size_t)kk);
    } else if (kk > -5 && kk <= 0) { /* 1234e-6 -> 0.001234 */
        pr3_buf_put_bytes(b, "0.", 2);
        put_zeros(b, (size_t)-kk);
        pr3_buf_put_bytes(b, digits, nd);
    } else if (nd == 1) { /* 1e30 */
        pr3_buf_put_char(b, digits[0]);
        put_exponent(b, kk - 1);
    } else { /* 1234e30 -> 1.234e33 */
        pr3_buf_put_char(b, digits[0]);
        pr3_buf_put_char(b, '.');
        pr3_buf_put_bytes(b, digits + 1, nd - 1);
        put_exponent(b, kk - 1);
    }
    return PR3_OK;
}

/* ---- object keys */

static int key_cmp(const pr3_json_tape *t, uint32_t a, uint32_t b) {
    return pr3_str_cmp(pr3_json_str(t, a), pr3_json_str(t, b));
}

pr3_status pr3_json_members(const pr3_json_tape *t, uint32_t obj, pr3_arena *a, const uint32_t **keys,
                            uint32_t *n) {
    const pr3_json_token *o = &t->tokens[obj];
    PR3_DEBUG_ASSERT(o->kind == PR3_JSON_OBJECT);
    uint32_t count = o->n;
    pr3_arena_mark start = pr3_arena_mark_get(a);
    uint32_t *list = PR3_NEW_N(a, uint32_t, count > 0 ? count : 1);
    if (list == NULL) return PR3_E_NOMEM;
    uint32_t m = 0;
    for (uint32_t i = obj + 1; i < o->v.end; i = pr3_json_next(t, i + 1)) list[m++] = i;
    PR3_CHECK(m == count);

    if (count > 1) {
        /* A stable bottom-up merge sort, so equal keys keep their input order; insertion
         * sort for the short runs that tool arguments are. */
        pr3_arena_mark mark = pr3_arena_mark_get(a);
        uint32_t *tmp = count > 8 ? PR3_NEW_N(a, uint32_t, count) : NULL;
        if (count > 8 && tmp == NULL) {
            pr3_arena_rewind(a, start);
            return PR3_E_NOMEM;
        }
        const uint32_t run = 8;
        for (uint32_t lo = 0; lo < count; lo += run) {
            uint32_t hi = lo + run < count ? lo + run : count;
            for (uint32_t i = lo + 1; i < hi; i++) {
                uint32_t k = list[i], j = i;
                while (j > lo && key_cmp(t, list[j - 1], k) > 0) {
                    list[j] = list[j - 1];
                    j--;
                }
                list[j] = k;
            }
        }
        /* 64-bit bounds: lo + 2 * width can pass UINT32_MAX for a huge object */
        for (uint64_t width = run; width < count; width *= 2) {
            for (uint64_t lo = 0; lo < count; lo += 2 * width) {
                uint64_t mid = lo + width < count ? lo + width : count;
                uint64_t hi = lo + 2 * width < count ? lo + 2 * width : count;
                uint64_t x = lo, y = mid, k = lo;
                while (x < mid && y < hi) tmp[k++] = key_cmp(t, list[y], list[x]) < 0 ? list[y++] : list[x++];
                while (x < mid) tmp[k++] = list[x++];
                while (y < hi) tmp[k++] = list[y++];
            }
            memcpy(list, tmp, (size_t)count * sizeof *list);
        }
        pr3_arena_rewind(a, mark);

        /* Keep the last of each run of equal keys. */
        uint32_t w = 0;
        for (uint32_t i = 0; i < count; i++) {
            if (i + 1 < count && key_cmp(t, list[i], list[i + 1]) == 0) continue;
            list[w++] = list[i];
        }
        count = w;
    }
    *keys = list;
    *n = count;
    return PR3_OK;
}

/* ---- values */

typedef struct frame {
    const uint32_t *keys; /* objects: sorted keys; arrays: NULL */
    pr3_arena_mark mark;  /* objects: the arena before the keys were taken */
    uint32_t next;        /* arrays: the next element's token; objects: the next key's position */
    uint32_t left;        /* members or elements still to write */
    bool started;         /* something was written, so the next needs a comma */
} frame;

PR3_INLINE void put_scalar(pr3_buf *b, const pr3_json_token *tok, unsigned flags) {
    switch ((pr3_json_kind)tok->kind) {
    case PR3_JSON_NULL: pr3_buf_put_bytes(b, "null", 4); break;
    case PR3_JSON_FALSE: pr3_buf_put_bytes(b, "false", 5); break;
    case PR3_JSON_TRUE: pr3_buf_put_bytes(b, "true", 4); break;
    case PR3_JSON_U64: pr3_buf_put_u64(b, tok->v.u); break;
    case PR3_JSON_I64: pr3_buf_put_i64(b, tok->v.i); break;
    case PR3_JSON_F64: {
        double v = tok->v.f;
        if ((flags & PR3_JSON_ZERO_UNSIGNED) && v == 0.0) v = 0.0;
        pr3_buf f = {b->ptr, b->len, b->cap, b->overflow}; /* a copy, as pr3_json_put_string uses */
        pr3_status s = pr3_json_put_f64(&f, v);
        b->len = f.len;
        b->overflow = f.overflow;
        PR3_CHECK(s == PR3_OK); /* the parser never makes an infinity or a NaN */
        break;
    }
    case PR3_JSON_STRING: pr3_json_put_string(b, (pr3_str){tok->v.s, tok->n}); break;
    case PR3_JSON_ARRAY:
    case PR3_JSON_OBJECT: PR3_CHECK(false); break;
    }
}

size_t pr3_json_write_bytes(const pr3_json_tape *t) {
    /* Live at once: the frames, the sorted keys of each object on the open path (their
     * members together at most len / 2) and one merge buffer (at most len / 2 again), each
     * an allocation with up to 7 bytes of padding and a red zone in asan builds. */
    const size_t slack = 7 + PR3_ARENA_REDZONE;
    size_t frames, keys, allocs, total;
    size_t levels = (size_t)t->depth + 1;
    if (pr3_ckd_mul(&frames, levels, sizeof(frame)) || pr3_ckd_mul(&keys, (size_t)t->len, sizeof(uint32_t)) ||
        pr3_ckd_mul(&allocs, levels + 2, slack) || pr3_ckd_add(&total, frames, keys) ||
        pr3_ckd_add(&total, total, allocs))
        return 0;
    return total;
}

pr3_status pr3_json_write(const pr3_json_tape *t, uint32_t i, pr3_arena *a, pr3_buf *dst, unsigned flags) {
    PR3_CHECK(i < t->len);
    /* Written through a copy whose address never leaves this function, so the compiler
     * keeps it in registers rather than reloading it after every byte stored. */
    pr3_buf copy = {dst->ptr, dst->len, dst->cap, dst->overflow}, *out = &copy;
    if (t->tokens[i].kind != PR3_JSON_ARRAY && t->tokens[i].kind != PR3_JSON_OBJECT) {
        put_scalar(out, &t->tokens[i], flags);
        dst->len = copy.len;
        dst->overflow = copy.overflow;
        return copy.overflow ? PR3_E_LIMIT : PR3_OK;
    }
    pr3_arena_mark mark = pr3_arena_mark_get(a);
    frame *stack = PR3_NEW_N(a, frame, t->depth > 0 ? t->depth : 1);
    if (stack == NULL) return PR3_E_NOMEM;
    uint32_t depth = 0;
    pr3_status s = PR3_OK;
    uint32_t cur = i;
    for (;;) {
        const pr3_json_token *tok = &t->tokens[cur];
        if (tok->kind == PR3_JSON_ARRAY && tok->n > 0) {
            PR3_CHECK(depth < t->depth);
            pr3_buf_put_char(out, '[');
            stack[depth++] =
                (frame){.keys = NULL, .mark = {0}, .next = cur + 1, .left = tok->n, .started = false};
        } else if (tok->kind == PR3_JSON_OBJECT && tok->n > 0) {
            PR3_CHECK(depth < t->depth);
            pr3_arena_mark before = pr3_arena_mark_get(a);
            const uint32_t *keys;
            uint32_t n;
            s = pr3_json_members(t, cur, a, &keys, &n);
            if (s != PR3_OK) break;
            pr3_buf_put_char(out, '{');
            stack[depth++] = (frame){.keys = keys, .mark = before, .next = 0, .left = n, .started = false};
        } else if (tok->kind == PR3_JSON_ARRAY) {
            pr3_buf_put_bytes(out, "[]", 2);
        } else if (tok->kind == PR3_JSON_OBJECT) {
            pr3_buf_put_bytes(out, "{}", 2);
        } else {
            put_scalar(out, tok, flags);
        }

        /* Pick the next value to write, closing every container that is finished. */
        for (;;) {
            if (depth == 0) goto done;
            frame *f = &stack[depth - 1];
            if (f->left == 0) {
                pr3_buf_put_char(out, f->keys ? '}' : ']');
                if (f->keys) pr3_arena_rewind(a, f->mark); /* the keys are the newest allocation */
                depth--;
                continue;
            }
            if (f->started) pr3_buf_put_char(out, ',');
            f->started = true;
            f->left--;
            if (f->keys) {
                uint32_t key = f->keys[f->next++];
                pr3_json_put_string(out, pr3_json_str(t, key));
                pr3_buf_put_char(out, ':');
                cur = key + 1;
            } else {
                cur = f->next;
                f->next = pr3_json_next(t, cur);
            }
            break;
        }
    }
done:
    dst->len = copy.len;
    dst->overflow = copy.overflow;
    pr3_arena_rewind(a, mark);
    if (s != PR3_OK) return s;
    return copy.overflow ? PR3_E_LIMIT : PR3_OK;
}

pr3_status pr3_json_canonicalize(pr3_arena *a, pr3_bytes in, pr3_buf *out, pr3_json_error *err) {
    pr3_arena_mark mark = pr3_arena_mark_get(a);
    pr3_json_tape t;
    pr3_status s = pr3_json_parse(a, in, &t, err);
    if (s == PR3_OK) s = pr3_json_write(&t, 0, a, out, 0);
    pr3_arena_rewind(a, mark);
    return s;
}

/* ---- schema-subset primitives */

pr3_status pr3_json_type_parse(pr3_str name, pr3_json_type *out) {
    static const struct {
        pr3_str name;
        pr3_json_type type;
    } types[] = {
        {PR3_S("string"), PR3_JSON_TYPE_STRING},   {PR3_S("number"), PR3_JSON_TYPE_NUMBER},
        {PR3_S("integer"), PR3_JSON_TYPE_INTEGER}, {PR3_S("boolean"), PR3_JSON_TYPE_BOOLEAN},
        {PR3_S("object"), PR3_JSON_TYPE_OBJECT},   {PR3_S("array"), PR3_JSON_TYPE_ARRAY},
    };
    for (size_t k = 0; k < PR3_ARRAY_LEN(types); k++) {
        if (pr3_str_eq(name, types[k].name)) {
            *out = types[k].type;
            return PR3_OK;
        }
    }
    return PR3_E_NOT_FOUND;
}

bool pr3_json_is_type(const pr3_json_tape *t, uint32_t i, pr3_json_type type) {
    pr3_json_kind k = (pr3_json_kind)t->tokens[i].kind;
    switch (type) {
    case PR3_JSON_TYPE_STRING: return k == PR3_JSON_STRING;
    case PR3_JSON_TYPE_NUMBER: return k == PR3_JSON_U64 || k == PR3_JSON_I64 || k == PR3_JSON_F64;
    case PR3_JSON_TYPE_INTEGER: return k == PR3_JSON_U64 || k == PR3_JSON_I64;
    case PR3_JSON_TYPE_BOOLEAN: return k == PR3_JSON_TRUE || k == PR3_JSON_FALSE;
    case PR3_JSON_TYPE_OBJECT: return k == PR3_JSON_OBJECT;
    case PR3_JSON_TYPE_ARRAY: return k == PR3_JSON_ARRAY;
    }
    return false;
}

bool pr3_json_name_valid(pr3_str name) {
    if (name.len == 0 || name.len > PR3_NAME_MAX_BYTES) return false;
    for (size_t i = 0; i < name.len; i++) {
        char c = name.ptr[i];
        if (!((c >= 'a' && c <= 'z') || (c >= '0' && c <= '9') || c == '_')) return false;
    }
    return true;
}
