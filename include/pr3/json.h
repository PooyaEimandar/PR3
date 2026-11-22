/* Canonical JSON (doc/04, "Canonical JSON"; doc/19 D-08): a bounded tape parser, the
 * canonical writer and the primitives the schema subset is checked with.
 *
 * The parser turns one JSON text into a tape of tokens in an arena: no tree, no
 * allocation per value, no recursion. Strings point into the input unless they
 * contained escapes, in which case they are decoded once into the arena. Nesting is at
 * most PR3_JSON_MAX_DEPTH levels; a duplicate key keeps its last value; numbers that fit
 * uint64_t or int64_t stay integers, and -0 and every other number become the correctly
 * rounded double. The writer emits the canonical form straight from the tape: keys
 * sorted in byte order, no whitespace, a fixed escape set, and doubles in the shortest
 * form that reads back to the same value. */
#ifndef PR3_JSON_H
#define PR3_JSON_H

#include <pr3/base.h>

typedef enum pr3_json_kind {
    PR3_JSON_NULL,
    PR3_JSON_FALSE,
    PR3_JSON_TRUE,
    PR3_JSON_U64,    /* an integer from 0 to UINT64_MAX */
    PR3_JSON_I64,    /* a negative integer from INT64_MIN */
    PR3_JSON_F64,    /* every other number, -0 included */
    PR3_JSON_STRING, /* valid UTF-8, escapes decoded */
    PR3_JSON_ARRAY,
    PR3_JSON_OBJECT,
} pr3_json_kind;

/* One value. An array's elements, or an object's key and value tokens in pairs, follow
 * it on the tape up to (not including) v.end. */
typedef struct pr3_json_token {
    uint8_t kind;  /* pr3_json_kind */
    uint8_t flags; /* PR3_JSON_DECODED */
    uint16_t reserved;
    uint32_t n; /* string: bytes; array: elements; object: members, duplicates included */
    union {
        uint64_t u;
        int64_t i;
        double f;
        const char *s; /* string: its bytes, in the input or (decoded) in the arena */
        uint32_t end;  /* array and object: the index one past their last token */
    } v;
} pr3_json_token;

#define PR3_JSON_DECODED 0x1u    /* the string had escapes and was decoded into the arena */
#define PR3_JSON_NONE UINT32_MAX /* "no such token" */

typedef struct pr3_json_tape {
    pr3_json_token *tokens;
    uint32_t len;
    uint32_t cap;
    uint32_t depth; /* the deepest nesting seen, 0 for a scalar */
} pr3_json_tape;

/* Where and why a text was refused; written only when parsing fails. */
typedef struct pr3_json_error {
    size_t offset; /* byte offset of the problem in the input */
    pr3_str reason;
} pr3_json_error;

/* Arena bytes pr3_json_parse may take for an input of in_len bytes: the tape at its
 * worst case and room for every string to need decoding. 0 on overflow. */
size_t pr3_json_parse_bytes(size_t in_len);

/* Parses one JSON text, surrounded by optional whitespace, into a tape in `a`.
 * PR3_E_UTF8 if the input is not valid UTF-8; PR3_E_LIMIT if it nests deeper than
 * PR3_JSON_MAX_DEPTH; PR3_E_INVALID if it is not JSON or a number is out of the range of
 * a double; PR3_E_NOMEM if `a` is too small, which never happens when `a` has
 * pr3_json_parse_bytes(in.len) bytes free. UTF-8 is checked first; after that the first
 * fault met from the left is the one reported. On failure the arena is rewound and err
 * (if not NULL) says where. */
PR3_NODISCARD pr3_status pr3_json_parse(pr3_arena *a, pr3_bytes in, pr3_json_tape *out, pr3_json_error *err);

/* The token after the value at i, its subtree included. */
PR3_INLINE uint32_t pr3_json_next(const pr3_json_tape *t, uint32_t i) {
    const pr3_json_token *tok = &t->tokens[i];
    return tok->kind == PR3_JSON_ARRAY || tok->kind == PR3_JSON_OBJECT ? tok->v.end : i + 1;
}

PR3_INLINE pr3_json_kind pr3_json_kind_of(const pr3_json_tape *t, uint32_t i) {
    return (pr3_json_kind)t->tokens[i].kind;
}

/* The bytes of the string at i. */
PR3_INLINE pr3_str pr3_json_str(const pr3_json_tape *t, uint32_t i) {
    PR3_DEBUG_ASSERT(t->tokens[i].kind == PR3_JSON_STRING);
    return (pr3_str){t->tokens[i].v.s, t->tokens[i].n};
}

/* The value of the member of object obj named key, the last one if the key repeats, or
 * PR3_JSON_NONE. */
uint32_t pr3_json_member(const pr3_json_tape *t, uint32_t obj, pr3_str key);

/* The key tokens of object obj sorted in byte order, each key once (the last occurrence;
 * its value is the token after it), in an array taken from `a`. */
PR3_NODISCARD pr3_status pr3_json_members(const pr3_json_tape *t, uint32_t obj, pr3_arena *a,
                                          const uint32_t **keys, uint32_t *n);

/* ---- The canonical writer */

/* Writes -0.0 as 0.0, for comparing values: the schema subset treats them as equal. */
#define PR3_JSON_ZERO_UNSIGNED 0x1u

/* Arena bytes pr3_json_write may take for any value of tape t: 4 bytes per token and
 * one frame per nesting level. 0 on overflow. */
size_t pr3_json_write_bytes(const pr3_json_tape *t);

/* Writes the value at i in canonical form. Scratch for sorting keys comes from `a` and
 * is released before returning. PR3_E_LIMIT if `out` is too small; PR3_E_NOMEM if `a`
 * is. */
PR3_NODISCARD pr3_status pr3_json_write(const pr3_json_tape *t, uint32_t i, pr3_arena *a, pr3_buf *out,
                                        unsigned flags);

/* Parses `in` and writes its canonical form into `out`; the arena is rewound before
 * returning. The statuses of pr3_json_parse and pr3_json_write. */
PR3_NODISCARD pr3_status pr3_json_canonicalize(pr3_arena *a, pr3_bytes in, pr3_buf *out, pr3_json_error *err);

/* A string, quoted and escaped canonically: \" \\ \b \f \n \r \t, \u00xx for the other
 * control characters, everything else as raw UTF-8. */
PR3_INLINE void pr3_json_put_string(pr3_buf *b, pr3_str s);

/* The halves of pr3_json_put_string. pr3_json_escape writes s's escaped bytes, without
 * quotes, at dst, which has room for 6 * s.len of them, and returns their end;
 * pr3_json_put_string_tight writes s to a builder that may not hold that worst case. */
char *pr3_json_escape(char *dst, pr3_str s);
void pr3_json_put_string_tight(pr3_buf *b, pr3_str s);

PR3_INLINE void pr3_json_put_string(pr3_buf *b, pr3_str s) {
    size_t room = b->cap - b->len;
    if (PR3_LIKELY(!b->overflow && room >= 2 && s.len <= (room - 2) / 6)) {
        char *p = b->ptr + b->len;
        *p = '"';
        p = pr3_json_escape(p + 1, s);
        *p = '"';
        b->len = (size_t)(p + 1 - b->ptr);
        return;
    }
    /* Through a copy, so a caller's builder kept in registers never has its address taken. */
    pr3_buf tight = {b->ptr, b->len, b->cap, b->overflow};
    pr3_json_put_string_tight(&tight, s);
    b->len = tight.len;
    b->overflow = tight.overflow;
}

/* A finite double in PR3's canonical layout: the shortest digits that read back to the
 * same value (Ryu), in decimal notation from 1e-5 up to below 1e16 (100.0, 0.0001) and
 * scientific notation otherwise (1e16, 1e-7, 1.5e-7). PR3_E_INVALID, writing nothing,
 * for an infinity or a NaN. */
PR3_NODISCARD pr3_status pr3_json_put_f64(pr3_buf *b, double v);

/* ---- Schema-subset primitives (doc/04, "Tool registry and the schema subset") */

typedef enum pr3_json_type {
    PR3_JSON_TYPE_STRING,
    PR3_JSON_TYPE_NUMBER,
    PR3_JSON_TYPE_INTEGER,
    PR3_JSON_TYPE_BOOLEAN,
    PR3_JSON_TYPE_OBJECT,
    PR3_JSON_TYPE_ARRAY,
} pr3_json_type;

/* "string", "number", "integer", "boolean", "object" or "array". */
PR3_NODISCARD pr3_status pr3_json_type_parse(pr3_str name, pr3_json_type *out);

/* Whether the value at i has the type: an integer is a number that stayed an integer
 * (U64 or I64), so 1.0 is a number but not an integer. */
bool pr3_json_is_type(const pr3_json_tape *t, uint32_t i, pr3_json_type type);

/* Tool and property names: 1 to PR3_NAME_MAX_BYTES bytes of [a-z0-9_]. */
bool pr3_json_name_valid(pr3_str name);

#endif /* PR3_JSON_H */
