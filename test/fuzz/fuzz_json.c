/* Canonical JSON under libFuzzer. For any input, with an arena sized by the published
 * bounds alone:
 *   - parsing never runs out of memory and either succeeds or says where it stopped;
 *   - an accepted text's tape is well formed: extents nest, strings are valid UTF-8;
 *   - its canonical form parses again and is a fixed point of canonicalization;
 *   - the comparison form (PR3_JSON_ZERO_UNSIGNED) is a fixed point too. */
#include <pr3/json.h>

#include <stdlib.h>

#define FUZZ_REQUIRE(cond)                                                                                   \
    do {                                                                                                     \
        if (!(cond)) abort();                                                                                \
    } while (0)

enum { MAX_INPUT = 1 << 16, GROWTH = 24 }; /* a number grows at most 24x: "1e15" */

static void check_tape(const pr3_json_tape *t) {
    FUZZ_REQUIRE(t->len > 0 && t->len <= t->cap && t->depth <= PR3_JSON_MAX_DEPTH);
    for (uint32_t i = 0; i < t->len; i++) {
        const pr3_json_token *tok = &t->tokens[i];
        FUZZ_REQUIRE(tok->kind <= PR3_JSON_OBJECT);
        if (tok->kind == PR3_JSON_ARRAY || tok->kind == PR3_JSON_OBJECT) {
            FUZZ_REQUIRE(tok->v.end > i && tok->v.end <= t->len);
            uint32_t n = 0;
            for (uint32_t j = i + 1; j < tok->v.end; j = pr3_json_next(t, j)) {
                if (tok->kind == PR3_JSON_OBJECT) {
                    FUZZ_REQUIRE(t->tokens[j].kind == PR3_JSON_STRING);
                    j++;
                    FUZZ_REQUIRE(j < tok->v.end);
                }
                n++;
            }
            FUZZ_REQUIRE(n == tok->n);
        } else if (tok->kind == PR3_JSON_STRING) {
            FUZZ_REQUIRE(pr3_utf8_validate((const uint8_t *)tok->v.s, tok->n) == PR3_OK);
        }
    }
    FUZZ_REQUIRE(pr3_json_next(t, 0) == t->len);
}

static void canonical_twice(pr3_arena *a, const pr3_json_tape *t, char *out, char *again, size_t cap,
                            unsigned flags) {
    pr3_buf b = pr3_buf_make(out, cap);
    FUZZ_REQUIRE(pr3_json_write(t, 0, a, &b, flags) == PR3_OK);
    pr3_arena_mark m = pr3_arena_mark_get(a);
    pr3_json_tape t2;
    FUZZ_REQUIRE(pr3_json_parse(a, (pr3_bytes){(const uint8_t *)b.ptr, b.len}, &t2, NULL) == PR3_OK);
    pr3_buf c = pr3_buf_make(again, cap);
    FUZZ_REQUIRE(pr3_json_write(&t2, 0, a, &c, flags) == PR3_OK);
    FUZZ_REQUIRE(c.len == b.len && memcmp(c.ptr, b.ptr, b.len) == 0);
    pr3_arena_rewind(a, m);
}

/* libFuzzer's entry point, so it stays external. */
int LLVMFuzzerTestOneInput(const uint8_t *data, size_t size); // NOLINT(misc-use-internal-linkage)

int LLVMFuzzerTestOneInput(const uint8_t *data, size_t size) { // NOLINT(misc-use-internal-linkage)
    if (size > MAX_INPUT) return 0;
    size_t out_cap = size * GROWTH + 64;
    pr3_json_tape worst = {
        .tokens = NULL, .len = (uint32_t)(out_cap / 2 + 1), .cap = 0, .depth = PR3_JSON_MAX_DEPTH};
    /* room to parse the input, then its canonical form, and write either */
    size_t need = pr3_json_parse_bytes(size) + pr3_json_parse_bytes(out_cap) + pr3_json_write_bytes(&worst);
    uint8_t *mem = malloc(need);
    char *out = malloc(out_cap), *again = malloc(out_cap);
    FUZZ_REQUIRE(mem && out && again);
    pr3_arena a;
    pr3_arena_init(&a, mem, need, "fuzz");

    pr3_json_tape t;
    pr3_json_error err = {0};
    pr3_status s = pr3_json_parse(&a, (pr3_bytes){data, size}, &t, &err);
    if (s == PR3_OK) {
        check_tape(&t);
        canonical_twice(&a, &t, out, again, out_cap, 0);
        canonical_twice(&a, &t, out, again, out_cap, PR3_JSON_ZERO_UNSIGNED);
    } else {
        FUZZ_REQUIRE(s == PR3_E_INVALID || s == PR3_E_UTF8 || s == PR3_E_LIMIT);
        FUZZ_REQUIRE(err.offset <= size && err.reason.len > 0);
        FUZZ_REQUIRE(a.used == 0); /* rewound */
    }
    pr3_arena_fini(&a);
    free(again);
    free(out);
    free(mem);
    return 0;
}
