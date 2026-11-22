/* PR3's test harness (doc/19 D-16): one header, no dependencies beyond the C library.
 * The same runner works on hosts, on phones over adb and, with PR3_TEST_NO_FILES and a
 * PR3_TEST_WRITE of the board's UART, on boards.
 *
 *   static void test_something_holds(pr3_t *t) { PR3_EXPECT(t, 1 + 1 == 2); }
 *   PR3_TEST_MAIN(PR3_TEST_CASE(test_something_holds))
 *
 * Output is TAP: "1..N", then "ok"/"not ok" per test with "#" diagnostics. Arguments:
 * --list prints the test names; --only NAME runs one test; --vectors DIR overrides the
 * golden vector directory; any other argument is a substring filter. The exit code is
 * 0 when every selected test passed. */
#ifndef PR3_TEST_H
#define PR3_TEST_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifndef PR3_TEST_WRITE
#define PR3_TEST_WRITE(p, n) ((void)fwrite((p), 1, (n), stdout))
#endif

#ifndef PR3_VECTORS_DIR
#define PR3_VECTORS_DIR "test/vectors"
#endif

typedef struct pr3_t {
    const char *name;
    unsigned checks;
    unsigned failures;
    bool skipped;
} pr3_t;

typedef void (*pr3_test_fn)(pr3_t *t);

typedef struct pr3_test_case {
    const char *name;
    pr3_test_fn fn;
} pr3_test_case;

#define PR3_TEST_CASE(fn) {#fn, fn}

static const char *pr3_test_vectors_dir = PR3_VECTORS_DIR;

static inline void pr3_test_print(const char *s) { PR3_TEST_WRITE(s, strlen(s)); }

#if defined(__GNUC__) || defined(__clang__)
static inline void pr3_test_printf(const char *fmt, ...) __attribute__((format(printf, 1, 2)));
#endif

static inline void pr3_test_printf(const char *fmt, ...) {
    char line[1024];
    va_list ap;
    va_start(ap, fmt);
    int n = vsnprintf(line, sizeof line, fmt, ap);
    va_end(ap);
    if (n < 0) return;
    size_t len = (size_t)n < sizeof line ? (size_t)n : sizeof line - 1;
    PR3_TEST_WRITE(line, len);
}

static inline bool pr3_test_check(pr3_t *t, bool ok, const char *what, const char *file, int line) {
    t->checks++;
    if (!ok) {
        t->failures++;
        pr3_test_printf("# %s:%d: %s: failed: %s\n", file, line, t->name, what);
    }
    return ok;
}

/* Prints a byte string with C escapes, for failure messages. */
static inline void pr3_test_print_quoted(const char *p, size_t n) {
    char out[8];
    pr3_test_print("\"");
    for (size_t i = 0; i < n && i < 400; i++) {
        unsigned char c = (unsigned char)p[i];
        if (c == '"' || c == '\\') {
            out[0] = '\\';
            out[1] = (char)c;
            PR3_TEST_WRITE(out, 2);
        } else if (c >= 0x20 && c < 0x7f) {
            PR3_TEST_WRITE(p + i, 1);
        } else {
            int k = snprintf(out, sizeof out, "\\x%02x", c);
            if (k > 0) PR3_TEST_WRITE(out, (size_t)k);
        }
    }
    if (n > 400) pr3_test_print("...");
    pr3_test_print("\"");
}

static inline bool pr3_test_check_bytes(pr3_t *t, const char *a, size_t an, const char *b, size_t bn,
                                        const char *what, const char *file, int line) {
    bool ok = an == bn && (an == 0 || memcmp(a, b, an) == 0);
    if (!pr3_test_check(t, ok, what, file, line)) {
        pr3_test_print("#   got:      ");
        pr3_test_print_quoted(a, an);
        pr3_test_print("\n#   expected: ");
        pr3_test_print_quoted(b, bn);
        pr3_test_print("\n");
    }
    return ok;
}

static inline bool pr3_test_check_u64(pr3_t *t, uint64_t a, uint64_t b, const char *what, const char *file,
                                      int line) {
    bool ok = a == b;
    if (!pr3_test_check(t, ok, what, file, line))
        pr3_test_printf("#   got %llu (0x%llx), expected %llu (0x%llx)\n", (unsigned long long)a,
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)b);
    return ok;
}

static inline bool pr3_test_check_i64(pr3_t *t, int64_t a, int64_t b, const char *what, const char *file,
                                      int line) {
    bool ok = a == b;
    if (!pr3_test_check(t, ok, what, file, line))
        pr3_test_printf("#   got %lld, expected %lld\n", (long long)a, (long long)b);
    return ok;
}

#define PR3_EXPECT(t, cond) pr3_test_check((t), (cond) ? true : false, #cond, __FILE__, __LINE__)
#define PR3_EXPECT_U64(t, a, b)                                                                              \
    pr3_test_check_u64((t), (uint64_t)(a), (uint64_t)(b), #a " == " #b, __FILE__, __LINE__)
#define PR3_EXPECT_I64(t, a, b)                                                                              \
    pr3_test_check_i64((t), (int64_t)(a), (int64_t)(b), #a " == " #b, __FILE__, __LINE__)
#define PR3_EXPECT_MEM(t, p, n, q, m)                                                                        \
    pr3_test_check_bytes((t), (const char *)(p), (n), (const char *)(q), (m), #p " == " #q, __FILE__,        \
                         __LINE__)
/* A pr3_str (or any struct with ptr and len) against a string literal. */
#define PR3_EXPECT_STR(t, s, lit) PR3_EXPECT_MEM((t), (s).ptr, (s).len, "" lit, sizeof(lit) - 1)
#define PR3_REQUIRE(t, cond)                                                                                 \
    do {                                                                                                     \
        if (!PR3_EXPECT((t), cond)) return;                                                                  \
    } while (0)
#define PR3_SKIP(t, why)                                                                                     \
    do {                                                                                                     \
        (t)->skipped = true;                                                                                 \
        pr3_test_printf("# %s: skipped: %s\n", (t)->name, why);                                              \
        return;                                                                                              \
    } while (0)

/* ---- Golden vectors: JSON Lines files under test/vectors/, read with this small,
 * independent reader so a parser under test never reads its own expectations. A vector
 * line is one flat object whose values are strings, integers, booleans or null. */

#ifndef PR3_TEST_NO_FILES

typedef struct pr3_test_lines {
    char *data;
    size_t len;
    size_t pos;
    unsigned line_no;
} pr3_test_lines;

/* Opens test/vectors/<rel>; false (after a failed check) when it cannot be read. */
static inline bool pr3_test_lines_open(pr3_t *t, pr3_test_lines *l, const char *rel) {
    char path[1024];
    (void)snprintf(path, sizeof path, "%s/%s", pr3_test_vectors_dir, rel);
    memset(l, 0, sizeof *l);
    FILE *f = fopen(path, "rb");
    if (!pr3_test_check(t, f != NULL, path, __FILE__, __LINE__)) return false;
    long size = fseek(f, 0, SEEK_END) == 0 ? ftell(f) : -1;
    bool ok = size >= 0 && fseek(f, 0, SEEK_SET) == 0;
    if (ok) l->data = (char *)malloc((size_t)size + 1); /* one spare byte for the last NUL */
    ok = ok && l->data && fread(l->data, 1, (size_t)size, f) == (size_t)size;
    (void)fclose(f);
    if (!ok) {
        free(l->data);
        l->data = NULL;
    } else {
        l->len = (size_t)size;
    }
    return pr3_test_check(t, ok, "vector file read", __FILE__, __LINE__);
}

static inline void pr3_test_lines_close(pr3_test_lines *l) {
    free(l->data);
    memset(l, 0, sizeof *l);
}

static inline const char *pr3_test_field_raw(const char *line, const char *key, size_t *raw_len);

/* The next vector: a non-empty line that is neither a "#" comment nor the file's header
 * (the line with "generated_by"); NUL-terminated in place. */
/* The analyzer treats file bytes as tainted offsets; every index here is bounded by len,
 * and data holds len + 1 bytes. */
// NOLINTBEGIN(clang-analyzer-security.ArrayBound)
static inline const char *pr3_test_lines_next(pr3_test_lines *l) {
    while (l->pos < l->len) {
        char *start = l->data + l->pos;
        char *end = memchr(start, '\n', l->len - l->pos);
        size_t n = end ? (size_t)(end - start) : l->len - l->pos;
        l->pos += n + (end ? 1 : 0);
        l->line_no++;
        if (n == 0 || start[0] == '#') continue;
        /* overwrites the newline, or the spare byte at the end */
        start[n] = '\0';
        size_t raw_len;
        if (pr3_test_field_raw(start, "generated_by", &raw_len)) continue;
        return start;
    }
    return NULL;
}
// NOLINTEND(clang-analyzer-security.ArrayBound)

/* Skips one JSON value starting at p; returns the byte after it, or NULL. */
static inline const char *pr3_test_json_skip(const char *p) {
    if (*p == '"') {
        for (p++; *p && *p != '"'; p++)
            if (*p == '\\' && p[1]) p++;
        return *p == '"' ? p + 1 : NULL;
    }
    while (*p && *p != ',' && *p != '}') p++;
    return p;
}

/* The raw value of a top-level key in a flat object line, or NULL. */
static inline const char *pr3_test_field_raw(const char *line, const char *key, size_t *raw_len) {
    size_t klen = strlen(key);
    const char *p = line;
    while (*p == ' ') p++;
    if (*p++ != '{') return NULL;
    for (;;) {
        while (*p == ' ' || *p == ',') p++;
        if (*p != '"') return NULL;
        const char *k = p + 1;
        const char *kend = pr3_test_json_skip(p);
        if (!kend) return NULL;
        p = kend;
        while (*p == ' ') p++;
        if (*p++ != ':') return NULL;
        while (*p == ' ') p++;
        const char *v = p;
        const char *vend = pr3_test_json_skip(p);
        if (!vend) return NULL;
        if ((size_t)(kend - 1 - k) == klen && memcmp(k, key, klen) == 0) {
            *raw_len = (size_t)(vend - v);
            return v;
        }
        p = vend;
    }
}

static inline int pr3_test_hexval(char c) {
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}

static inline void pr3_test_put_utf8(char *out, size_t *n, uint32_t cp) {
    if (cp < 0x80) {
        out[(*n)++] = (char)cp;
    } else if (cp < 0x800) {
        out[(*n)++] = (char)(0xC0 | (cp >> 6));
        out[(*n)++] = (char)(0x80 | (cp & 0x3F));
    } else if (cp < 0x10000) {
        out[(*n)++] = (char)(0xE0 | (cp >> 12));
        out[(*n)++] = (char)(0x80 | ((cp >> 6) & 0x3F));
        out[(*n)++] = (char)(0x80 | (cp & 0x3F));
    } else {
        out[(*n)++] = (char)(0xF0 | (cp >> 18));
        out[(*n)++] = (char)(0x80 | ((cp >> 12) & 0x3F));
        out[(*n)++] = (char)(0x80 | ((cp >> 6) & 0x3F));
        out[(*n)++] = (char)(0x80 | (cp & 0x3F));
    }
}

/* A string field, unescaped into a heap buffer the caller frees; NULL if absent. Lone
 * surrogate escapes (\ud800) decode to their three-byte form, so a vector can describe
 * input that is not valid UTF-8. */
static inline char *pr3_test_field_str(const char *line, const char *key, size_t *len) {
    size_t raw_len;
    const char *v = pr3_test_field_raw(line, key, &raw_len);
    if (!v || *v != '"') return NULL;
    char *out = (char *)malloc(raw_len + 1);
    size_t n = 0;
    if (!out) return NULL;
    for (const char *p = v + 1; p < v + raw_len - 1; p++) {
        if (*p != '\\') {
            out[n++] = *p;
            continue;
        }
        p++;
        switch (*p) {
        case 'n': out[n++] = '\n'; break;
        case 't': out[n++] = '\t'; break;
        case 'r': out[n++] = '\r'; break;
        case 'b': out[n++] = '\b'; break;
        case 'f': out[n++] = '\f'; break;
        case 'u': {
            uint32_t cp = 0;
            for (int i = 1; i <= 4; i++) cp = cp << 4 | (uint32_t)pr3_test_hexval(p[i]);
            p += 4;
            if (cp >= 0xD800 && cp < 0xDC00 && p[1] == '\\' && p[2] == 'u') {
                uint32_t lo = 0;
                for (int i = 3; i <= 6; i++) lo = lo << 4 | (uint32_t)pr3_test_hexval(p[i]);
                if (lo >= 0xDC00 && lo < 0xE000) {
                    cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                    p += 6;
                }
            }
            pr3_test_put_utf8(out, &n, cp);
            break;
        }
        default: out[n++] = *p; break; /* \" \\ \/ */
        }
    }
    out[n] = '\0';
    *len = n;
    return out;
}

/* Bytes from a hex string field into a heap buffer the caller frees; NULL if absent. */
static inline unsigned char *pr3_test_field_hex(const char *line, const char *key, size_t *len) {
    size_t n;
    char *s = pr3_test_field_str(line, key, &n);
    if (!s || n % 2) {
        free(s);
        return NULL;
    }
    unsigned char *out = (unsigned char *)malloc(n / 2 + 1);
    for (size_t i = 0; out && i < n / 2; i++)
        out[i] = (unsigned char)(pr3_test_hexval(s[2 * i]) << 4 | pr3_test_hexval(s[2 * i + 1]));
    free(s);
    *len = n / 2;
    return out;
}

static inline bool pr3_test_field_i64(const char *line, const char *key, int64_t *out) {
    size_t raw_len;
    const char *v = pr3_test_field_raw(line, key, &raw_len);
    if (!v || raw_len == 0) return false;
    char tmp[32];
    if (raw_len >= sizeof tmp) return false;
    memcpy(tmp, v, raw_len);
    tmp[raw_len] = '\0';
    char *end;
    long long x = strtoll(tmp, &end, 10);
    if (*end) return false;
    *out = (int64_t)x;
    return true;
}

static inline bool pr3_test_field_is(const char *line, const char *key, const char *raw) {
    size_t raw_len;
    const char *v = pr3_test_field_raw(line, key, &raw_len);
    return v && raw_len == strlen(raw) && memcmp(v, raw, raw_len) == 0;
}

#endif /* PR3_TEST_NO_FILES */

/* ---- The runner */

static inline int pr3_test_run(int argc, char **argv, const pr3_test_case *cases, size_t n_cases) {
    const char *only = NULL;
    const char *filters[16];
    size_t n_filters = 0;
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--list") == 0) {
            for (size_t c = 0; c < n_cases; c++) pr3_test_printf("%s\n", cases[c].name);
            return 0;
        } else if (strcmp(argv[i], "--only") == 0 && i + 1 < argc) {
            only = argv[++i];
        } else if (strcmp(argv[i], "--vectors") == 0 && i + 1 < argc) {
            pr3_test_vectors_dir = argv[++i];
        } else if (n_filters < 16) {
            filters[n_filters++] = argv[i];
        }
    }
    size_t selected = 0;
    for (size_t c = 0; c < n_cases; c++) {
        bool take = only ? strcmp(cases[c].name, only) == 0 : n_filters == 0;
        for (size_t f = 0; !only && f < n_filters; f++) take = take || strstr(cases[c].name, filters[f]);
        selected += take;
    }
    if (selected == 0) {
        pr3_test_print("1..0 # no test selected\n");
        return only ? 1 : 0;
    }
    pr3_test_printf("1..%zu\n", selected);
    size_t index = 0, failed = 0;
    for (size_t c = 0; c < n_cases; c++) {
        bool take = only ? strcmp(cases[c].name, only) == 0 : n_filters == 0;
        for (size_t f = 0; !only && f < n_filters; f++) take = take || strstr(cases[c].name, filters[f]);
        if (!take) continue;
        pr3_t t = {cases[c].name, 0, 0, false};
        cases[c].fn(&t);
        index++;
        if (t.failures) {
            failed++;
            pr3_test_printf("not ok %zu - %s (%u of %u checks failed)\n", index, t.name, t.failures,
                            t.checks);
        } else if (t.skipped) {
            pr3_test_printf("ok %zu - %s # SKIP\n", index, t.name);
        } else {
            pr3_test_printf("ok %zu - %s\n", index, t.name);
        }
    }
    if (failed) pr3_test_printf("# %zu of %zu tests failed\n", failed, selected);
    (void)fflush(stdout);
    return failed ? 1 : 0;
}

#define PR3_TEST_MAIN(...)                                                                                   \
    static const pr3_test_case pr3_test_cases[] = {__VA_ARGS__};                                             \
    int main(int argc, char **argv) {                                                                        \
        return pr3_test_run(argc, argv, pr3_test_cases, sizeof pr3_test_cases / sizeof pr3_test_cases[0]);   \
    }

#endif /* PR3_TEST_H */
