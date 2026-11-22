/* The tape parser: one pass, no recursion, an explicit stack of open containers. */
#include "json_internal.h"

#include <pr3/poison.h>

size_t pr3_json_parse_bytes(size_t in_len) {
    /* Every token but a lone top-level scalar owns at least two bytes of the input (a
     * container its brackets, a string its quotes, a number or literal its own byte and
     * the comma or bracket after it), so in_len / 2 + 1 tokens always suffice. Decoded
     * strings are never longer than their escaped form. */
    size_t tokens, bytes, total;
    if (pr3_ckd_add(&tokens, in_len / 2, 1) || pr3_ckd_mul(&bytes, tokens, sizeof(pr3_json_token)) ||
        pr3_ckd_add(&total, bytes, in_len) ||
        pr3_ckd_add(&total, total, _Alignof(pr3_json_token) + PR3_ARENA_REDZONE + 7))
        return 0;
    /* each decoded string is its own allocation, with a red zone in asan builds */
    if (PR3_ARENA_REDZONE &&
        (pr3_ckd_mul(&bytes, in_len / 2 + 1, PR3_ARENA_REDZONE + 7) || pr3_ckd_add(&total, total, bytes)))
        return 0;
    return total;
}

typedef struct parser {
    const uint8_t *p, *end, *begin;
    pr3_arena *arena;
    pr3_json_tape *tape;
    pr3_json_error *err;
} parser;

static pr3_status fail(parser *ps, const uint8_t *at, pr3_status s, pr3_str reason) {
    if (ps->err) *ps->err = (pr3_json_error){.offset = (size_t)(at - ps->begin), .reason = reason};
    return s;
}

static void skip_ws(parser *ps) {
    while (ps->p < ps->end && (*ps->p == ' ' || *ps->p == '\n' || *ps->p == '\r' || *ps->p == '\t')) ps->p++;
}

static int hex_value(uint8_t c) {
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}

/* Four hex digits at p (bounds already checked), or -1. */
static int32_t hex4(const uint8_t *p) {
    int32_t v = 0;
    for (int i = 0; i < 4; i++) {
        int h = hex_value(p[i]);
        if (h < 0) return -1;
        v = v << 4 | h;
    }
    return v;
}

/* Decodes the escaped string body [s, e) into dst; returns the decoded length. The body
 * was validated by scan_string. */
static size_t decode_string(const uint8_t *s, const uint8_t *e, uint8_t *dst) {
    size_t n = 0;
    while (s < e) {
        if (*s != '\\') {
            dst[n++] = *s++;
            continue;
        }
        s++;
        uint8_t c = *s++;
        switch (c) {
        case 'b': dst[n++] = '\b'; break;
        case 'f': dst[n++] = '\f'; break;
        case 'n': dst[n++] = '\n'; break;
        case 'r': dst[n++] = '\r'; break;
        case 't': dst[n++] = '\t'; break;
        case 'u': {
            uint32_t cp = (uint32_t)hex4(s);
            s += 4;
            if (cp >= 0xD800 && cp <= 0xDBFF) { /* a high surrogate; scan_string checked the pair */
                uint32_t lo = (uint32_t)hex4(s + 2);
                cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                s += 6;
            }
            n += pr3_utf8_encode(cp, dst + n);
            break;
        }
        default: dst[n++] = c; break; /* \" \\ \/ */
        }
    }
    return n;
}

/* A string starting at the opening quote. */
static pr3_status scan_string(parser *ps, pr3_json_token *tok) {
    const uint8_t *body = ++ps->p, *q = body;
    bool escapes = false;
    for (;;) {
        /* the common run: no quote, backslash or control character */
        while (q < ps->end && *q != '"' && *q != '\\' && *q >= 0x20) q++;
        if (q == ps->end) return fail(ps, body - 1, PR3_E_INVALID, PR3_S("unterminated string"));
        if (*q == '"') break;
        if (*q < 0x20) return fail(ps, q, PR3_E_INVALID, PR3_S("control character in string"));
        escapes = true; /* a backslash */
        if (ps->end - q < 2) return fail(ps, q, PR3_E_INVALID, PR3_S("unterminated string"));
        uint8_t c = q[1];
        if (c == '"' || c == '\\' || c == '/' || c == 'b' || c == 'f' || c == 'n' || c == 'r' || c == 't') {
            q += 2;
            continue;
        }
        if (c != 'u') return fail(ps, q, PR3_E_INVALID, PR3_S("invalid escape"));
        int32_t cp = ps->end - q >= 6 ? hex4(q + 2) : -1;
        if (cp < 0) return fail(ps, q, PR3_E_INVALID, PR3_S("invalid \\u escape"));
        if (cp >= 0xDC00 && cp <= 0xDFFF) return fail(ps, q, PR3_E_INVALID, PR3_S("lone low surrogate"));
        if (cp >= 0xD800 && cp <= 0xDBFF) {
            int32_t lo = ps->end - q >= 12 && q[6] == '\\' && q[7] == 'u' ? hex4(q + 8) : -1;
            if (lo < 0xDC00 || lo > 0xDFFF) return fail(ps, q, PR3_E_INVALID, PR3_S("lone high surrogate"));
            q += 12;
            continue;
        }
        q += 6;
    }
    size_t raw = (size_t)(q - body);
    if (raw > UINT32_MAX) return fail(ps, body, PR3_E_LIMIT, PR3_S("string too long"));
    if (!escapes) {
        *tok = (pr3_json_token){.kind = PR3_JSON_STRING, .n = (uint32_t)raw, .v.s = (const char *)body};
    } else {
        uint8_t *dst = pr3_arena_alloc(ps->arena, raw, 1);
        if (dst == NULL) return fail(ps, body, PR3_E_NOMEM, PR3_S("out of memory"));
        size_t n = decode_string(body, q, dst);
        *tok = (pr3_json_token){
            .kind = PR3_JSON_STRING, .flags = PR3_JSON_DECODED, .n = (uint32_t)n, .v.s = (const char *)dst};
    }
    ps->p = q + 1;
    return PR3_OK;
}

static bool literal(parser *ps, const char *word, size_t n) {
    if ((size_t)(ps->end - ps->p) < n || memcmp(ps->p, word, n) != 0) return false;
    ps->p += n;
    return true;
}

/* The tape is sized by the bound in pr3_json_parse_bytes, which holds for every JSON
 * text, so a text that runs out of tokens cannot be JSON, whatever follows. */
#define TOO_MANY PR3_S("more values than a JSON text of this length can hold")

static pr3_json_token *push(parser *ps) {
    pr3_json_tape *t = ps->tape;
    return t->len < t->cap ? &t->tokens[t->len++] : NULL;
}

pr3_status pr3_json_parse(pr3_arena *a, pr3_bytes in, pr3_json_tape *out, pr3_json_error *err) {
    if (in.ptr == NULL) { /* no arithmetic on NULL */
        PR3_CHECK(in.len == 0);
        in.ptr = (const uint8_t *)"";
    }
    pr3_arena_mark mark = pr3_arena_mark_get(a);
    pr3_json_tape tape = {0};
    parser ps = {.p = in.ptr, .end = in.ptr + in.len, .begin = in.ptr, .arena = a, .tape = &tape, .err = err};
    pr3_status s = PR3_OK;

    size_t valid = in.len ? pr3_utf8_valid_prefix(in.ptr, in.len) : 0;
    if (valid != in.len) {
        s = fail(&ps, in.ptr + valid, PR3_E_UTF8, PR3_S("invalid UTF-8"));
        goto out;
    }
    size_t cap = in.len / 2 + 1;
    if (cap > UINT32_MAX - 1) {
        s = fail(&ps, in.ptr, PR3_E_LIMIT, PR3_S("input too large"));
        goto out;
    }
    tape.tokens = PR3_NEW_N(a, pr3_json_token, cap);
    if (tape.tokens == NULL) {
        s = fail(&ps, in.ptr, PR3_E_NOMEM, PR3_S("out of memory"));
        goto out;
    }
    tape.cap = (uint32_t)cap;

    uint32_t stack[PR3_JSON_MAX_DEPTH]; /* open containers, innermost last */
    uint32_t depth = 0;
    for (;;) {
        /* ---- a value */
        skip_ws(&ps);
        if (ps.p == ps.end) {
            s = fail(&ps, ps.p, PR3_E_INVALID, PR3_S("expected a value"));
            goto out;
        }
        pr3_json_token *tok = push(&ps);
        if (tok == NULL) {
            s = fail(&ps, ps.p, PR3_E_INVALID, TOO_MANY);
            goto out;
        }
        uint8_t c = *ps.p;
        if (c == '{' || c == '[') {
            if (depth == PR3_JSON_MAX_DEPTH) {
                s = fail(&ps, ps.p, PR3_E_LIMIT, PR3_S("nesting too deep"));
                goto out;
            }
            *tok = (pr3_json_token){.kind = c == '{' ? PR3_JSON_OBJECT : PR3_JSON_ARRAY};
            stack[depth++] = tape.len - 1;
            if (depth > tape.depth) tape.depth = depth;
            ps.p++;
            skip_ws(&ps);
            if (ps.p < ps.end && *ps.p == (c == '{' ? '}' : ']')) {
                ps.p++;
                tok->v.end = tape.len;
                depth--;
                goto value_done;
            }
            if (c == '[') continue; /* the first element */
            goto member;
        } else if (c == '"') {
            s = scan_string(&ps, tok);
        } else if (c == '-' || (c >= '0' && c <= '9')) {
            const uint8_t *after;
            pr3_str reason;
            s = pr3_json_scan_number(ps.p, ps.end, &after, tok, &reason);
            if (s != PR3_OK) s = fail(&ps, after, s, reason);
            ps.p = after;
        } else if (literal(&ps, "true", 4)) {
            *tok = (pr3_json_token){.kind = PR3_JSON_TRUE};
        } else if (literal(&ps, "false", 5)) {
            *tok = (pr3_json_token){.kind = PR3_JSON_FALSE};
        } else if (literal(&ps, "null", 4)) {
            *tok = (pr3_json_token){.kind = PR3_JSON_NULL};
        } else {
            s = fail(&ps, ps.p, PR3_E_INVALID, PR3_S("expected a value"));
        }
        if (s != PR3_OK) goto out;

    value_done:
        /* ---- after a value: the end, a comma, or a closing bracket */
        skip_ws(&ps);
        if (depth == 0) {
            if (ps.p != ps.end) {
                s = fail(&ps, ps.p, PR3_E_INVALID, PR3_S("trailing characters"));
                goto out;
            }
            break;
        }
        pr3_json_token *open = &tape.tokens[stack[depth - 1]];
        open->n++;
        if (ps.p == ps.end) {
            s = fail(&ps, ps.p, PR3_E_INVALID, PR3_S("unterminated container"));
            goto out;
        }
        c = *ps.p++;
        bool is_object = open->kind == PR3_JSON_OBJECT;
        if (c == ',') {
            if (!is_object) continue;
            skip_ws(&ps);
            goto member;
        }
        if (c == (is_object ? '}' : ']')) {
            open->v.end = tape.len;
            depth--;
            goto value_done;
        }
        s = fail(&ps, ps.p - 1, PR3_E_INVALID,
                 is_object ? PR3_S("expected ',' or '}'") : PR3_S("expected ',' or ']'"));
        goto out;

    member:
        /* ---- an object member: a key string, a colon, then its value */
        if (ps.p == ps.end || *ps.p != '"') {
            s = fail(&ps, ps.p, PR3_E_INVALID, PR3_S("expected a string key"));
            goto out;
        }
        tok = push(&ps);
        if (tok == NULL) {
            s = fail(&ps, ps.p, PR3_E_INVALID, TOO_MANY);
            goto out;
        }
        s = scan_string(&ps, tok);
        if (s != PR3_OK) goto out;
        skip_ws(&ps);
        if (ps.p == ps.end || *ps.p != ':') {
            s = fail(&ps, ps.p, PR3_E_INVALID, PR3_S("expected ':'"));
            goto out;
        }
        ps.p++;
    }

out:
    if (s != PR3_OK) {
        pr3_arena_rewind(a, mark);
        return s;
    }
    *out = tape;
    return PR3_OK;
}

uint32_t pr3_json_member(const pr3_json_tape *t, uint32_t obj, pr3_str key) {
    PR3_DEBUG_ASSERT(t->tokens[obj].kind == PR3_JSON_OBJECT);
    uint32_t found = PR3_JSON_NONE;
    for (uint32_t i = obj + 1; i < t->tokens[obj].v.end; i = pr3_json_next(t, i + 1))
        if (pr3_str_eq(pr3_json_str(t, i), key)) found = i + 1;
    return found;
}
