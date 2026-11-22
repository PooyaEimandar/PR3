/* The json module's budget rows (doc/15, "Module budgets"): parse, and validate and
 * canonicalize, on the small, nested, number-heavy and escape-heavy workloads. */
#include "pr3_bench.h"

#include <pr3/json.h>

typedef struct json_case {
    pr3_bytes in;
    pr3_arena arena;
    char *out;
    size_t out_cap;
} json_case;

static uint64_t parse_once(void *ctx) {
    json_case *c = ctx;
    pr3_json_tape t;
    pr3_arena_reset(&c->arena);
    return pr3_json_parse(&c->arena, c->in, &t, NULL) == PR3_OK ? t.len : 0;
}

static uint64_t canonicalize_once(void *ctx) {
    json_case *c = ctx;
    pr3_buf b = pr3_buf_make(c->out, c->out_cap);
    pr3_arena_reset(&c->arena);
    return pr3_json_canonicalize(&c->arena, c->in, &b, NULL) == PR3_OK ? b.len : 0;
}

int main(int argc, char **argv) {
    pr3_bench b;
    if (!pr3_bench_init(&b, "json", argc, argv)) return 2;
    static const char *names[] = {"small", "nested", "numbers", "escapes"};
    for (size_t i = 0; i < sizeof names / sizeof names[0]; i++) {
        char rel[64], label[64];
        (void)snprintf(rel, sizeof rel, "json/%s.json", names[i]);
        size_t len = 0;
        char *text = pr3_bench_read(&b, rel, &len);
        if (!text) continue;
        json_case c = {.in = {(const uint8_t *)text, len}, .out_cap = len * 24 + 64};
        pr3_json_tape worst = {
            .tokens = NULL, .len = (uint32_t)(len / 2 + 1), .cap = 0, .depth = PR3_JSON_MAX_DEPTH};
        size_t need = pr3_json_parse_bytes(len) + pr3_json_write_bytes(&worst);
        void *mem = malloc(need);
        c.out = malloc(c.out_cap);
        if (!mem || !c.out) {
            (void)fprintf(stderr, "bench: out of memory\n");
            free(c.out);
            free(mem);
            free(text);
            return 2;
        }
        pr3_arena_init(&c.arena, mem, need, "bench");
        (void)snprintf(label, sizeof label, "parse.%s", names[i]);
        pr3_bench_run(&b, label, len, parse_once, &c);
        (void)snprintf(label, sizeof label, "canonicalize.%s", names[i]);
        pr3_bench_run(&b, label, len, canonicalize_once, &c);
        pr3_arena_fini(&c.arena);
        free(c.out);
        free(mem);
        free(text);
    }
    return pr3_bench_finish(&b);
}
