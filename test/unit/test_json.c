/* Canonical JSON's behaviour (doc/04, "Canonical JSON"; ledger: doc/20, "Canonical
 * JSON"). The golden vectors come from tools/gen-vectors/canonical_json.py. */
#include <pr3/json.h>

#include "pr3_test.h"

/* ThreadSanitizer and MemorySanitizer builds run the long randomized loops shorter. */
#if defined(__has_feature)
#if __has_feature(thread_sanitizer) || __has_feature(memory_sanitizer)
#define SLOW_BUILD 1
#endif
#endif
#if defined(__SANITIZE_THREAD__)
#define SLOW_BUILD 1
#endif
#ifdef SLOW_BUILD
enum { ROUNDS = 10000 };
#else
enum { ROUNDS = 200000 };
#endif

PR3_WRAPPING static uint64_t rng_next(uint64_t *s) {
    uint64_t z = (*s += UINT64_C(0x9E3779B97F4A7C15));
    z = (z ^ (z >> 30)) * UINT64_C(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)) * UINT64_C(0x94D049BB133111EB);
    return z ^ (z >> 31);
}

/* An arena sized for parsing and writing an input of n bytes, from the published
 * bounds alone: every test below would fail with PR3_E_NOMEM if a bound were short. */
typedef struct scratch {
    void *mem;
    pr3_arena arena;
} scratch;

static bool scratch_open(scratch *s, size_t in_len) {
    size_t parse = pr3_json_parse_bytes(in_len);
    /* a tape holds at most in_len / 2 + 1 tokens and nests at most PR3_JSON_MAX_DEPTH */
    pr3_json_tape worst = {
        .tokens = NULL, .len = (uint32_t)(in_len / 2 + 1), .cap = 0, .depth = PR3_JSON_MAX_DEPTH};
    size_t write = pr3_json_write_bytes(&worst);
    s->mem = malloc(parse + write);
    if (s->mem == NULL) return false;
    pr3_arena_init(&s->arena, s->mem, parse + write, "json-test");
    return true;
}

static void scratch_close(scratch *s) {
    pr3_arena_fini(&s->arena);
    free(s->mem);
}

static pr3_bytes bytes_of(const char *p, size_t n) { return (pr3_bytes){(const uint8_t *)p, n}; }

static pr3_bytes cstr(const char *p) { return bytes_of(p, strlen(p)); }

static bool contains(const char *p, size_t n, const char *needle, size_t m) {
    for (size_t i = 0; i + m <= n; i++)
        if (memcmp(p + i, needle, m) == 0) return true;
    return false;
}

/* Canonicalizes in, comparing with want (NULL: expect status st). */
static void expect_canonical(pr3_t *t, pr3_bytes in, const char *want, size_t want_len) {
    scratch s;
    PR3_REQUIRE(t, scratch_open(&s, in.len));
    size_t cap = in.len * 8 + 64; /* canonical text is never more than 6x plus a few bytes */
    char *out = malloc(cap);
    PR3_REQUIRE(t, out != NULL);
    pr3_buf b = pr3_buf_make(out, cap);
    pr3_json_error err = {0};
    pr3_status st = pr3_json_canonicalize(&s.arena, in, &b, &err);
    if (PR3_EXPECT(t, st == PR3_OK)) PR3_EXPECT_MEM(t, b.ptr, b.len, want, want_len);
    PR3_EXPECT_U64(t, s.arena.used, 0); /* rewound */
    free(out);
    scratch_close(&s);
}

static pr3_status parse_status(const uint8_t *in, size_t n, pr3_json_error *err) {
    scratch s;
    if (!scratch_open(&s, n)) return PR3_E_NOMEM;
    pr3_json_tape tape;
    pr3_status st = pr3_json_parse(&s.arena, (pr3_bytes){in, n}, &tape, err);
    scratch_close(&s);
    return st;
}

/* ---- golden vectors */

static void test_canonical_vectors_match(pr3_t *t) {
    pr3_test_lines l;
    if (!pr3_test_lines_open(t, &l, "json/canonical.jsonl")) return;
    const char *line;
    unsigned n = 0;
    while ((line = pr3_test_lines_next(&l)) != NULL) {
        size_t in_len = 0, out_len = 0, eq_len = 0;
        char *in = pr3_test_field_str(line, "in", &in_len);
        char *out = pr3_test_field_str(line, "out", &out_len);
        char *eq = pr3_test_field_str(line, "eq", &eq_len);
        if (PR3_EXPECT(t, in && out)) {
            expect_canonical(t, bytes_of(in, in_len), out, out_len);
            expect_canonical(t, bytes_of(out, out_len), out, out_len); /* a fixed point */

            /* the comparison form: -0.0 written as 0.0 */
            scratch s;
            PR3_REQUIRE(t, scratch_open(&s, in_len));
            pr3_json_tape tape;
            char buf[8192];
            pr3_buf b = pr3_buf_make(buf, sizeof buf);
            if (PR3_EXPECT(t, pr3_json_parse(&s.arena, bytes_of(in, in_len), &tape, NULL) == PR3_OK) &&
                PR3_EXPECT(t, pr3_json_write(&tape, 0, &s.arena, &b, PR3_JSON_ZERO_UNSIGNED) == PR3_OK)) {
                if (eq)
                    PR3_EXPECT_MEM(t, b.ptr, b.len, eq, eq_len);
                else
                    PR3_EXPECT_MEM(t, b.ptr, b.len, out, out_len);
            }
            scratch_close(&s);
        }
        free(in);
        free(out);
        free(eq);
        n++;
    }
    pr3_test_lines_close(&l);
    PR3_EXPECT(t, n >= 400);
}

static void test_number_vectors_match(pr3_t *t) {
    pr3_test_lines l;
    if (!pr3_test_lines_open(t, &l, "json/numbers.jsonl")) return;
    const char *line;
    unsigned n = 0;
    while ((line = pr3_test_lines_next(&l)) != NULL) {
        size_t in_len = 0, out_len = 0, bits_len = 0;
        char *in = pr3_test_field_str(line, "in", &in_len);
        char *out = pr3_test_field_str(line, "out", &out_len);
        unsigned char *bits = pr3_test_field_hex(line, "bits", &bits_len);
        pr3_json_kind kind = pr3_test_field_is(line, "kind", "\"u64\"")   ? PR3_JSON_U64
                             : pr3_test_field_is(line, "kind", "\"i64\"") ? PR3_JSON_I64
                                                                          : PR3_JSON_F64;
        if (PR3_EXPECT(t, in && out && (kind != PR3_JSON_F64 || (bits && bits_len == 8)))) {
            scratch s;
            PR3_REQUIRE(t, scratch_open(&s, in_len));
            pr3_json_tape tape;
            pr3_status st = pr3_json_parse(&s.arena, bytes_of(in, in_len), &tape, NULL);
            if (PR3_EXPECT(t, st == PR3_OK) && PR3_EXPECT_U64(t, tape.tokens[0].kind, kind)) {
                if (kind == PR3_JSON_F64) {
                    uint64_t got, want = 0;
                    memcpy(&got, &tape.tokens[0].v.f, sizeof got);
                    for (size_t i = 0; i < 8; i++) want = want << 8 | bits[i];
                    if (!PR3_EXPECT_U64(t, got, want)) pr3_test_printf("# input: %.80s\n", in);
                    char buf[64];
                    pr3_buf b = pr3_buf_make(buf, sizeof buf);
                    double v;
                    memcpy(&v, &want, sizeof v);
                    PR3_EXPECT(t, pr3_json_put_f64(&b, v) == PR3_OK);
                    PR3_EXPECT_MEM(t, b.ptr, b.len, out, out_len);
                }
                char buf[64];
                pr3_buf b = pr3_buf_make(buf, sizeof buf);
                PR3_EXPECT(t, pr3_json_write(&tape, 0, &s.arena, &b, 0) == PR3_OK);
                PR3_EXPECT_MEM(t, b.ptr, b.len, out, out_len);
            }
            scratch_close(&s);
        }
        free(in);
        free(out);
        free(bits);
        n++;
    }
    pr3_test_lines_close(&l);
    PR3_EXPECT(t, n >= 1000);
}

static void test_invalid_vectors_are_refused(pr3_t *t) {
    pr3_test_lines l;
    if (!pr3_test_lines_open(t, &l, "json/invalid.jsonl")) return;
    const char *line;
    unsigned n = 0;
    while ((line = pr3_test_lines_next(&l)) != NULL) {
        size_t len = 0;
        unsigned char *in = (unsigned char *)pr3_test_field_str(line, "in", &len);
        if (in == NULL) in = pr3_test_field_hex(line, "in_hex", &len);
        pr3_status want = pr3_test_field_is(line, "error", "\"utf8\"")    ? PR3_E_UTF8
                          : pr3_test_field_is(line, "error", "\"limit\"") ? PR3_E_LIMIT
                                                                          : PR3_E_INVALID;
        if (PR3_EXPECT(t, in != NULL)) {
            pr3_json_error err = {0};
            pr3_status got = parse_status(in, len, &err);
            if (!PR3_EXPECT_U64(t, got, want)) pr3_test_printf("# line %u\n", l.line_no);
            PR3_EXPECT(t, err.offset <= len && err.reason.len > 0);
        }
        free(in);
        n++;
    }
    pr3_test_lines_close(&l);
    PR3_EXPECT(t, n >= 80);
}

/* ---- parsing */

static void test_nesting_up_to_the_limit_is_accepted(pr3_t *t) {
    char in[2 * PR3_JSON_MAX_DEPTH];
    memset(in, '[', PR3_JSON_MAX_DEPTH);
    memset(in + PR3_JSON_MAX_DEPTH, ']', PR3_JSON_MAX_DEPTH);
    PR3_EXPECT_U64(t, parse_status((const uint8_t *)in, sizeof in, NULL), PR3_OK);
}

static void test_nesting_beyond_the_limit_is_refused(pr3_t *t) {
    char in[2 * (PR3_JSON_MAX_DEPTH + 1)];
    memset(in, '[', PR3_JSON_MAX_DEPTH + 1);
    memset(in + PR3_JSON_MAX_DEPTH + 1, ']', PR3_JSON_MAX_DEPTH + 1);
    pr3_json_error err = {0};
    PR3_EXPECT_U64(t, parse_status((const uint8_t *)in, sizeof in, &err), PR3_E_LIMIT);
    PR3_EXPECT_U64(t, err.offset, PR3_JSON_MAX_DEPTH); /* the bracket one too deep */
    /* deeper still, and unclosed: refused at the same place, without reading on */
    char deep[100000];
    memset(deep, '[', sizeof deep);
    PR3_EXPECT_U64(t, parse_status((const uint8_t *)deep, sizeof deep, &err), PR3_E_LIMIT);
    PR3_EXPECT_U64(t, err.offset, PR3_JSON_MAX_DEPTH);
}

static void test_the_tape_records_kinds_counts_and_extents(pr3_t *t) {
    static const char in[] = "{\"a\":[1,-2,3.5,\"x\",null,true,false],\"b\":{}}";
    scratch s;
    PR3_REQUIRE(t, scratch_open(&s, sizeof in - 1));
    pr3_json_tape tape;
    PR3_REQUIRE(t, pr3_json_parse(&s.arena, bytes_of(in, sizeof in - 1), &tape, NULL) == PR3_OK);
    PR3_EXPECT_U64(t, tape.len, 12);
    PR3_EXPECT_U64(t, tape.depth, 2);
    PR3_EXPECT_U64(t, pr3_json_kind_of(&tape, 0), PR3_JSON_OBJECT);
    PR3_EXPECT_U64(t, tape.tokens[0].n, 2);
    PR3_EXPECT_U64(t, tape.tokens[0].v.end, 12);
    PR3_EXPECT_STR(t, pr3_json_str(&tape, 1), "a");
    PR3_EXPECT_U64(t, pr3_json_kind_of(&tape, 2), PR3_JSON_ARRAY);
    PR3_EXPECT_U64(t, tape.tokens[2].n, 7);
    PR3_EXPECT_U64(t, pr3_json_next(&tape, 2), 10);
    PR3_EXPECT_U64(t, tape.tokens[3].v.u, 1);
    PR3_EXPECT_I64(t, tape.tokens[4].v.i, -2);
    PR3_EXPECT(t, tape.tokens[5].kind == PR3_JSON_F64 && tape.tokens[5].v.f == 3.5);
    PR3_EXPECT_U64(t, pr3_json_kind_of(&tape, 7), PR3_JSON_NULL);
    PR3_EXPECT_U64(t, pr3_json_kind_of(&tape, 8), PR3_JSON_TRUE);
    PR3_EXPECT_U64(t, pr3_json_kind_of(&tape, 9), PR3_JSON_FALSE);
    PR3_EXPECT_U64(t, pr3_json_kind_of(&tape, 11), PR3_JSON_OBJECT);
    PR3_EXPECT_U64(t, tape.tokens[11].n, 0);
    scratch_close(&s);
}

static void test_strings_without_escapes_point_into_the_input(pr3_t *t) {
    static const char in[] = "[\"plain\",\"tab\\there\"]";
    scratch s;
    PR3_REQUIRE(t, scratch_open(&s, sizeof in - 1));
    pr3_json_tape tape;
    PR3_REQUIRE(t, pr3_json_parse(&s.arena, bytes_of(in, sizeof in - 1), &tape, NULL) == PR3_OK);
    PR3_EXPECT(t, tape.tokens[1].v.s == in + 2 && tape.tokens[1].flags == 0);
    PR3_EXPECT_STR(t, pr3_json_str(&tape, 2), "tab\there");
    PR3_EXPECT(t, tape.tokens[2].flags & PR3_JSON_DECODED);
    scratch_close(&s);
}

static void test_duplicate_keys_keep_the_last_value(pr3_t *t) {
    static const char in[] = "{\"b\":1,\"a\":2,\"b\":3,\"c\":4,\"a\":5}";
    scratch s;
    PR3_REQUIRE(t, scratch_open(&s, sizeof in - 1));
    pr3_json_tape tape;
    PR3_REQUIRE(t, pr3_json_parse(&s.arena, bytes_of(in, sizeof in - 1), &tape, NULL) == PR3_OK);
    PR3_EXPECT_U64(t, tape.tokens[0].n, 5);
    uint32_t a = pr3_json_member(&tape, 0, PR3_S("a")), b = pr3_json_member(&tape, 0, PR3_S("b"));
    PR3_EXPECT(t, a != PR3_JSON_NONE && tape.tokens[a].v.u == 5);
    PR3_EXPECT(t, b != PR3_JSON_NONE && tape.tokens[b].v.u == 3);
    PR3_EXPECT_U64(t, pr3_json_member(&tape, 0, PR3_S("d")), PR3_JSON_NONE);
    const uint32_t *keys;
    uint32_t n;
    PR3_REQUIRE(t, pr3_json_members(&tape, 0, &s.arena, &keys, &n) == PR3_OK);
    PR3_REQUIRE(t, n == 3);
    PR3_EXPECT_STR(t, pr3_json_str(&tape, keys[0]), "a");
    PR3_EXPECT_U64(t, tape.tokens[keys[0] + 1].v.u, 5);
    PR3_EXPECT_STR(t, pr3_json_str(&tape, keys[1]), "b");
    PR3_EXPECT_U64(t, tape.tokens[keys[1] + 1].v.u, 3);
    PR3_EXPECT_STR(t, pr3_json_str(&tape, keys[2]), "c");
    scratch_close(&s);
}

static void test_large_objects_sort_stably_and_keep_the_last_duplicate(pr3_t *t) {
    enum { KEYS = 3000 };
    static char in[KEYS * 24 + 2];
    uint64_t seed = 7;
    pr3_buf b = pr3_buf_make(in, sizeof in);
    pr3_buf_put_char(&b, '{');
    for (unsigned i = 0; i < KEYS; i++) {
        if (i) pr3_buf_put_char(&b, ',');
        pr3_buf_put_bytes(&b, "\"k", 2);
        pr3_buf_put_u64(&b, rng_next(&seed) % 700); /* many duplicates */
        pr3_buf_put_bytes(&b, "\":", 2);
        pr3_buf_put_u64(&b, i);
    }
    pr3_buf_put_char(&b, '}');
    PR3_REQUIRE(t, !b.overflow);
    scratch s;
    PR3_REQUIRE(t, scratch_open(&s, b.len));
    pr3_json_tape tape;
    PR3_REQUIRE(t, pr3_json_parse(&s.arena, bytes_of(b.ptr, b.len), &tape, NULL) == PR3_OK);
    const uint32_t *keys;
    uint32_t n;
    PR3_REQUIRE(t, pr3_json_members(&tape, 0, &s.arena, &keys, &n) == PR3_OK);
    PR3_EXPECT(t, n > 600 && n <= 700);
    for (uint32_t i = 0; i < n; i++) {
        pr3_str k = pr3_json_str(&tape, keys[i]);
        if (i) PR3_EXPECT(t, pr3_str_cmp(pr3_json_str(&tape, keys[i - 1]), k) < 0);
        /* the value kept is the one pr3_json_member finds: the last */
        PR3_EXPECT_U64(t, keys[i] + 1, pr3_json_member(&tape, 0, k));
    }
    scratch_close(&s);
}

static void test_errors_say_where_and_why(pr3_t *t) {
    pr3_json_error err = {0};
    PR3_EXPECT_U64(t, parse_status((const uint8_t *)"[1,2,x]", 7, &err), PR3_E_INVALID);
    PR3_EXPECT_U64(t, err.offset, 5);
    PR3_EXPECT(t, err.reason.len > 0);
    PR3_EXPECT_U64(t, parse_status((const uint8_t *)"[1] 2", 5, &err), PR3_E_INVALID);
    PR3_EXPECT_U64(t, err.offset, 4);
    PR3_EXPECT_U64(t, parse_status((const uint8_t *)"\"ab\xff\"", 5, &err), PR3_E_UTF8);
    PR3_EXPECT_U64(t, err.offset, 3);
    PR3_EXPECT_U64(t, parse_status((const uint8_t *)"[1e999]", 7, &err), PR3_E_INVALID);
    PR3_EXPECT_U64(t, err.offset, 1);
    PR3_EXPECT_STR(t, err.reason, "number out of range");
}

static void test_a_small_arena_fails_cleanly(pr3_t *t) {
    static const char in[] = "{\"a\":[1,2,3,{\"b\":\"\\u00e9\"}],\"c\":\"d\"}";
    size_t need = pr3_json_parse_bytes(sizeof in - 1);
    uint8_t *mem = malloc(need);
    PR3_REQUIRE(t, mem != NULL);
    for (size_t cap = 0; cap < need; cap += 7) {
        pr3_arena a;
        pr3_arena_init(&a, mem, cap, "small");
        pr3_json_tape tape;
        pr3_status st = pr3_json_parse(&a, bytes_of(in, sizeof in - 1), &tape, NULL);
        PR3_EXPECT(t, st == PR3_OK || st == PR3_E_NOMEM);
        if (st != PR3_OK) PR3_EXPECT_U64(t, a.used, 0); /* rewound */
        pr3_arena_fini(&a);
    }
    free(mem);
}

static void test_empty_and_null_inputs_are_refused(pr3_t *t) {
    scratch s;
    PR3_REQUIRE(t, scratch_open(&s, 0));
    pr3_json_tape tape;
    pr3_json_error err = {0};
    PR3_EXPECT_U64(t, pr3_json_parse(&s.arena, (pr3_bytes){NULL, 0}, &tape, &err), PR3_E_INVALID);
    PR3_EXPECT_U64(t, err.offset, 0);
    PR3_EXPECT_U64(t, pr3_json_parse(&s.arena, cstr("   "), &tape, &err), PR3_E_INVALID);
    PR3_EXPECT_U64(t, err.offset, 3);
    scratch_close(&s);
}

/* Random bytes drawn from JSON's alphabet: whatever the parser says, it must not need
 * more than its bound, and what it accepts must canonicalize to a fixed point. */
static void test_random_inputs_stay_within_the_bounds(pr3_t *t) {
    static const char alphabet[] = "[]{}:,\"\\ 0123456789-+.eEtrufalsn\x01\xc3\xa9u";
    uint64_t seed = 11;
    char in[96], out[1024], again[1024];
    unsigned accepted = 0;
    for (unsigned iter = 0; iter < ROUNDS; iter++) {
        size_t n = (size_t)(rng_next(&seed) % sizeof in);
        for (size_t i = 0; i < n; i++) in[i] = alphabet[rng_next(&seed) % (sizeof alphabet - 1)];
        scratch s;
        PR3_REQUIRE(t, scratch_open(&s, n));
        pr3_buf b = pr3_buf_make(out, sizeof out);
        pr3_status st = pr3_json_canonicalize(&s.arena, bytes_of(in, n), &b, NULL);
        if (!PR3_EXPECT(t, st != PR3_E_NOMEM)) pr3_test_print_quoted(in, n);
        if (st == PR3_OK) {
            accepted++;
            pr3_buf c = pr3_buf_make(again, sizeof again);
            PR3_EXPECT(t, pr3_json_canonicalize(&s.arena, bytes_of(b.ptr, b.len), &c, NULL) == PR3_OK);
            PR3_EXPECT_MEM(t, c.ptr, c.len, b.ptr, b.len);
        }
        scratch_close(&s);
    }
    PR3_EXPECT(t, accepted > ROUNDS / 2000);
}

/* Numbers longer than any exponent cap: the digit counts and the exponent combine
 * exactly, however many digits there are. */
static void test_long_numbers_scale_by_their_whole_exponent(pr3_t *t) {
    enum { ZEROS = 200000 };
    static const struct {
        const char *head, *tail;
        double want;
    } cases[] = {
        {"0.", "1e200005", 1e4}, /* 0.000...01e200005 */
        {"1", "e-200000", 1.0},  /* 1000...0e-200000 */
        {"0.", "1e200001", 1.0}, /* a long run of fraction zeros, scaled back */
        {"0.", "1e199000", 0.0}, /* still below the smallest double */
        {"0.", "1e-99999999999999999999", 0.0},
    };
    static char in[ZEROS + 64];
    for (size_t i = 0; i < PR3_ARRAY_LEN(cases); i++) {
        size_t h = strlen(cases[i].head), tl = strlen(cases[i].tail);
        memcpy(in, cases[i].head, h);
        memset(in + h, '0', ZEROS);
        memcpy(in + h + ZEROS, cases[i].tail, tl);
        size_t n = h + ZEROS + tl;
        scratch s;
        PR3_REQUIRE(t, scratch_open(&s, n));
        pr3_json_tape tape;
        if (PR3_EXPECT(t, pr3_json_parse(&s.arena, bytes_of(in, n), &tape, NULL) == PR3_OK)) {
            PR3_EXPECT_U64(t, tape.tokens[0].kind, PR3_JSON_F64);
            if (!PR3_EXPECT(t, tape.tokens[0].v.f == cases[i].want)) pr3_test_printf("# case %zu\n", i);
        }
        scratch_close(&s);
    }
    /* the same zeros the other way: far beyond the largest double, so refused */
    in[0] = '1';
    memset(in + 1, '0', ZEROS);
    memcpy(in + 1 + ZEROS, "e-199000", 8);
    PR3_EXPECT_U64(t, parse_status((const uint8_t *)in, 1 + ZEROS + 8, NULL), PR3_E_INVALID);
}

/* ---- writing */

static void test_a_small_buffer_is_reported(pr3_t *t) {
    static const char in[] = "{\"b\":[1,2,3],\"a\":\"text\"}";
    scratch s;
    PR3_REQUIRE(t, scratch_open(&s, sizeof in - 1));
    for (size_t cap = 0; cap < 25; cap++) {
        char out[32];
        pr3_buf b = pr3_buf_make(out, cap);
        pr3_status st = pr3_json_canonicalize(&s.arena, bytes_of(in, sizeof in - 1), &b, NULL);
        PR3_EXPECT_U64(t, st, cap < 24 ? PR3_E_LIMIT : PR3_OK);
        PR3_EXPECT_U64(t, s.arena.used, 0);
    }
    scratch_close(&s);
}

static void test_strings_escape_exactly_the_canonical_set(pr3_t *t) {
    char raw[128];
    for (int c = 0; c < 128; c++) raw[c] = (char)c;
    char out[1024];
    pr3_buf b = pr3_buf_make(out, sizeof out);
    pr3_json_put_string(&b, (pr3_str){raw, sizeof raw});
    PR3_REQUIRE(t, !b.overflow);
    static const char want_head[] = "\"\\u0000\\u0001\\u0002\\u0003\\u0004\\u0005\\u0006\\u0007\\b\\t\\n"
                                    "\\u000b\\f\\r\\u000e\\u000f\\u0010\\u0011\\u0012\\u0013\\u0014\\u0015"
                                    "\\u0016\\u0017\\u0018\\u0019\\u001a\\u001b\\u001c\\u001d\\u001e\\u001f"
                                    " !\\\"#$%&'()*+,-./";
    PR3_EXPECT_MEM(t, b.ptr, sizeof want_head - 1, want_head, sizeof want_head - 1);
    static const char want_tail[] = "XYZ[\\\\]^_`abc";
    PR3_EXPECT(t, contains(b.ptr, b.len, want_tail, sizeof want_tail - 1));
    PR3_EXPECT(t, b.ptr[b.len - 2] == 0x7F && b.ptr[b.len - 1] == '"');
}

/* The canonical escape of one byte, as doc/04 lists them; its length. */
static size_t escape_one(uint8_t c, char *out) {
    static const char hex[] = "0123456789abcdef";
    const char *short_form = c == '"'    ? "\\\""
                             : c == '\\' ? "\\\\"
                             : c == '\b' ? "\\b"
                             : c == '\f' ? "\\f"
                             : c == '\n' ? "\\n"
                             : c == '\r' ? "\\r"
                             : c == '\t' ? "\\t"
                                         : NULL;
    if (short_form != NULL) {
        memcpy(out, short_form, 2);
        return 2;
    }
    if (c >= 0x20) {
        out[0] = (char)c;
        return 1;
    }
    memcpy(out, "\\u00", 4);
    out[4] = hex[c >> 4];
    out[5] = hex[c & 15];
    return 6;
}

/* Whether the n bytes at p still hold 0xFF, a byte no UTF-8 text contains. */
static bool guard_intact(const char *p, size_t n) {
    for (size_t i = 0; i < n; i++)
        if ((uint8_t)p[i] != 0xFF) return false;
    return true;
}

/* The writer copies eight bytes at a time up to the first that needs an escape, and
 * writes byte by byte when the builder may not hold the worst case. Random text at every
 * offset in the builder and every capacity around its length is the canonical text when
 * it fits and an overflow when it does not, and the bytes just past the capacity are
 * never written. */
static void test_escapes_are_exact_at_every_offset_and_capacity(pr3_t *t) {
#define PIECE(lit) {"" lit, sizeof(lit) - 1}
    static const struct {
        const char *s;
        size_t n;
    } pieces[] = {PIECE("\x00"),     PIECE("\x01"),     PIECE("\b"),           PIECE("\t"),
                  PIECE("\n"),       PIECE("\f"),       PIECE("\r"),           PIECE("\x1f"),
                  PIECE(" "),        PIECE("\""),       PIECE("\\"),           PIECE("/"),
                  PIECE("\x7f"),     PIECE("\xc3\xa9"), PIECE("\xe2\x82\xac"), PIECE("\xf0\x9f\x98\x80"),
                  PIECE("abcdefgh"), PIECE("xy")};
#undef PIECE
    uint64_t seed = 11;
    enum { GUARD = 16 }; /* bytes past the capacity that must stay as they were */
    char raw[48], want[48 * 6 + 2], out[8 + 48 * 6 + 2 + 8 + GUARD];
    for (unsigned iter = 0; iter < ROUNDS / 20; iter++) {
        size_t len = 0, target = (size_t)(rng_next(&seed) % 41);
        while (len < target) { /* escapes, multibyte characters and runs of letters */
            size_t k = (size_t)(rng_next(&seed) % PR3_ARRAY_LEN(pieces));
            memcpy(raw + len, pieces[k].s, pieces[k].n);
            len += pieces[k].n;
        }
        size_t n = 0;
        want[n++] = '"';
        for (size_t i = 0; i < len; i++) n += escape_one((uint8_t)raw[i], want + n);
        want[n++] = '"';
        for (size_t pre = 0; pre < 8; pre++) {
            for (size_t cap = pre; cap <= pre + n + 8; cap++) {
                memset(out + cap, 0xFF, GUARD);
                pr3_buf b = pr3_buf_make(out, cap);
                pr3_buf_put_bytes(&b, "........", pre);
                pr3_json_put_string(&b, (pr3_str){raw, len});
                bool fits = cap >= pre + n;
                if (!PR3_EXPECT(t, b.overflow == !fits && b.len <= cap) ||
                    !PR3_EXPECT(t, guard_intact(out + cap, GUARD)) ||
                    (fits && !PR3_EXPECT_MEM(t, b.ptr + pre, b.len - pre, want, n)))
                    return;
            }
        }
    }
}

static void test_doubles_use_the_canonical_layout(pr3_t *t) {
    static const struct {
        double v;
        const char *text;
    } cases[] = {
        {0.0, "0.0"},       {-0.0, "-0.0"},           {1.0, "1.0"},
        {100.0, "100.0"},   {0.0001, "0.0001"},       {1e-5, "0.00001"},
        {1e-6, "1e-6"},     {1e16, "1e16"},           {1e15, "1000000000000000.0"},
        {1.5e-7, "1.5e-7"}, {-2.5, "-2.5"},           {123.456, "123.456"},
        {1e300, "1e300"},   {5e-324, "5e-324"},       {1.7976931348623157e308, "1.7976931348623157e308"},
        {0.1, "0.1"},       {1.2345e16, "1.2345e16"}, {9007199254740993.0, "9007199254740992.0"},
    };
    for (size_t i = 0; i < PR3_ARRAY_LEN(cases); i++) {
        char out[64];
        pr3_buf b = pr3_buf_make(out, sizeof out);
        PR3_EXPECT(t, pr3_json_put_f64(&b, cases[i].v) == PR3_OK);
        PR3_EXPECT_MEM(t, b.ptr, b.len, cases[i].text, strlen(cases[i].text));
    }
}

static void test_infinities_and_nans_are_refused(pr3_t *t) {
    static const uint64_t bits[] = {UINT64_C(0x7FF0000000000000), UINT64_C(0xFFF0000000000000),
                                    UINT64_C(0x7FF8000000000000), UINT64_C(0x7FF0000000000001),
                                    UINT64_C(0xFFFFFFFFFFFFFFFF)};
    for (size_t i = 0; i < PR3_ARRAY_LEN(bits); i++) {
        double v;
        memcpy(&v, &bits[i], sizeof v);
        char out[64];
        pr3_buf b = pr3_buf_make(out, sizeof out);
        PR3_EXPECT_U64(t, pr3_json_put_f64(&b, v), PR3_E_INVALID);
        PR3_EXPECT_U64(t, b.len, 0);
    }
}

/* Every finite double written canonically reads back as the same bits. */
static void test_random_doubles_round_trip(pr3_t *t) {
    uint64_t seed = 3;
    static uint8_t mem[4096];
    for (unsigned iter = 0; iter < ROUNDS; iter++) {
        uint64_t bits = rng_next(&seed);
        if (iter % 4 == 1) bits &= UINT64_C(0x800FFFFFFFFFFFFF); /* subnormals */
        if (iter % 4 == 2)
            bits = (bits & UINT64_C(0x800FFFFFFFFFFFFF)) | UINT64_C(0x4330000000000000); /* ~2^52 */
        double v;
        memcpy(&v, &bits, sizeof v);
        char text[64];
        pr3_buf b = pr3_buf_make(text, sizeof text);
        if (pr3_json_put_f64(&b, v) != PR3_OK) continue; /* infinity or NaN */
        pr3_arena a;
        pr3_arena_init(&a, mem, pr3_json_parse_bytes(b.len), "one");
        pr3_json_tape tape;
        if (!PR3_EXPECT(t, pr3_json_parse(&a, bytes_of(b.ptr, b.len), &tape, NULL) == PR3_OK)) break;
        uint64_t back;
        memcpy(&back, &tape.tokens[0].v.f, sizeof back);
        if (!PR3_EXPECT_U64(t, back, bits)) {
            pr3_test_printf("# text: %.*s\n", (int)b.len, b.ptr);
            break;
        }
        pr3_arena_fini(&a);
    }
}

static void test_write_needs_no_more_than_its_bound(pr3_t *t) {
    /* a deep chain of objects with many keys each, so every level holds sorted keys */
    char in[60000];
    pr3_buf b = pr3_buf_make(in, sizeof in);
    for (unsigned d = 0; d < PR3_JSON_MAX_DEPTH - 1; d++) {
        pr3_buf_put_char(&b, '{');
        for (unsigned k = 0; k < 12; k++) {
            pr3_buf_put_bytes(&b, "\"k", 2);
            pr3_buf_put_u64(&b, 11 - k);
            pr3_buf_put_bytes(&b, "\":0,", 4);
        }
        pr3_buf_put_bytes(&b, "\"z\":", 4);
    }
    pr3_buf_put_bytes(&b, "[]", 2);
    for (unsigned d = 0; d < PR3_JSON_MAX_DEPTH - 1; d++) pr3_buf_put_char(&b, '}');
    PR3_REQUIRE(t, !b.overflow);

    size_t parse = pr3_json_parse_bytes(b.len);
    uint8_t *mem = malloc(parse);
    PR3_REQUIRE(t, mem != NULL);
    pr3_arena pa;
    pr3_arena_init(&pa, mem, parse, "tape");
    pr3_json_tape tape;
    PR3_REQUIRE(t, pr3_json_parse(&pa, bytes_of(b.ptr, b.len), &tape, NULL) == PR3_OK);
    size_t write = pr3_json_write_bytes(&tape);
    uint8_t *wmem = malloc(write);
    PR3_REQUIRE(t, wmem != NULL);
    pr3_arena wa;
    pr3_arena_init(&wa, wmem, write, "write");
    static char out[60000];
    pr3_buf o = pr3_buf_make(out, sizeof out);
    PR3_EXPECT_U64(t, pr3_json_write(&tape, 0, &wa, &o, 0), PR3_OK);
    PR3_EXPECT_U64(t, wa.used, 0);
    PR3_EXPECT(t, wa.high_water <= write);
    pr3_arena_fini(&wa);
    pr3_arena_fini(&pa);
    free(wmem);
    free(mem);
}

/* ---- the schema subset's primitives */

static void test_type_names_parse(pr3_t *t) {
    static const char *names[] = {"string", "number", "integer", "boolean", "object", "array"};
    for (size_t i = 0; i < PR3_ARRAY_LEN(names); i++) {
        pr3_json_type ty;
        PR3_EXPECT(t, pr3_json_type_parse((pr3_str){names[i], strlen(names[i])}, &ty) == PR3_OK);
        PR3_EXPECT_U64(t, ty, i);
    }
    pr3_json_type ty;
    PR3_EXPECT_U64(t, pr3_json_type_parse(PR3_S("null"), &ty), PR3_E_NOT_FOUND);
    PR3_EXPECT_U64(t, pr3_json_type_parse(PR3_S("String"), &ty), PR3_E_NOT_FOUND);
}

static void test_an_integer_is_a_number_that_stayed_an_integer(pr3_t *t) {
    static const char in[] = "[1,-1,1.0,1e0,-0,\"1\",true,null,{},[]]";
    scratch s;
    PR3_REQUIRE(t, scratch_open(&s, sizeof in - 1));
    pr3_json_tape tape;
    PR3_REQUIRE(t, pr3_json_parse(&s.arena, bytes_of(in, sizeof in - 1), &tape, NULL) == PR3_OK);
    static const bool integer[] = {true, true, false, false, false, false, false, false, false, false};
    static const bool number[] = {true, true, true, true, true, false, false, false, false, false};
    uint32_t i = 1;
    for (size_t k = 0; k < PR3_ARRAY_LEN(integer); k++, i = pr3_json_next(&tape, i)) {
        PR3_EXPECT(t, pr3_json_is_type(&tape, i, PR3_JSON_TYPE_INTEGER) == integer[k]);
        PR3_EXPECT(t, pr3_json_is_type(&tape, i, PR3_JSON_TYPE_NUMBER) == number[k]);
    }
    PR3_EXPECT(t, pr3_json_is_type(&tape, 6, PR3_JSON_TYPE_STRING));
    PR3_EXPECT(t, pr3_json_is_type(&tape, 7, PR3_JSON_TYPE_BOOLEAN));
    PR3_EXPECT(t, pr3_json_is_type(&tape, 9, PR3_JSON_TYPE_OBJECT));
    PR3_EXPECT(t, pr3_json_is_type(&tape, 10, PR3_JSON_TYPE_ARRAY));
    PR3_EXPECT(t, !pr3_json_is_type(&tape, 8, PR3_JSON_TYPE_BOOLEAN));
    scratch_close(&s);
}

static void test_names_are_short_lowercase_words(pr3_t *t) {
    PR3_EXPECT(t, pr3_json_name_valid(PR3_S("get_weather")));
    PR3_EXPECT(t, pr3_json_name_valid(PR3_S("a")));
    PR3_EXPECT(t, pr3_json_name_valid(PR3_S("v2_search_0")));
    PR3_EXPECT(t, !pr3_json_name_valid(PR3_S("")));
    PR3_EXPECT(t, !pr3_json_name_valid(PR3_S("Get")));
    PR3_EXPECT(t, !pr3_json_name_valid(PR3_S("get-weather")));
    PR3_EXPECT(t, !pr3_json_name_valid(PR3_S("caf\xc3\xa9")));
    char name[PR3_NAME_MAX_BYTES + 1];
    memset(name, 'a', sizeof name);
    PR3_EXPECT(t, pr3_json_name_valid((pr3_str){name, PR3_NAME_MAX_BYTES}));
    PR3_EXPECT(t, !pr3_json_name_valid((pr3_str){name, PR3_NAME_MAX_BYTES + 1}));
}

PR3_TEST_MAIN(PR3_TEST_CASE(test_canonical_vectors_match), PR3_TEST_CASE(test_number_vectors_match),
              PR3_TEST_CASE(test_invalid_vectors_are_refused),
              PR3_TEST_CASE(test_nesting_up_to_the_limit_is_accepted),
              PR3_TEST_CASE(test_nesting_beyond_the_limit_is_refused),
              PR3_TEST_CASE(test_the_tape_records_kinds_counts_and_extents),
              PR3_TEST_CASE(test_strings_without_escapes_point_into_the_input),
              PR3_TEST_CASE(test_duplicate_keys_keep_the_last_value),
              PR3_TEST_CASE(test_large_objects_sort_stably_and_keep_the_last_duplicate),
              PR3_TEST_CASE(test_errors_say_where_and_why), PR3_TEST_CASE(test_a_small_arena_fails_cleanly),
              PR3_TEST_CASE(test_empty_and_null_inputs_are_refused),
              PR3_TEST_CASE(test_random_inputs_stay_within_the_bounds),
              PR3_TEST_CASE(test_long_numbers_scale_by_their_whole_exponent),
              PR3_TEST_CASE(test_a_small_buffer_is_reported),
              PR3_TEST_CASE(test_strings_escape_exactly_the_canonical_set),
              PR3_TEST_CASE(test_escapes_are_exact_at_every_offset_and_capacity),
              PR3_TEST_CASE(test_doubles_use_the_canonical_layout),
              PR3_TEST_CASE(test_infinities_and_nans_are_refused),
              PR3_TEST_CASE(test_random_doubles_round_trip),
              PR3_TEST_CASE(test_write_needs_no_more_than_its_bound), PR3_TEST_CASE(test_type_names_parse),
              PR3_TEST_CASE(test_an_integer_is_a_number_that_stayed_an_integer),
              PR3_TEST_CASE(test_names_are_short_lowercase_words))
