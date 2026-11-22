/* The core module's budget rows (doc/15, "Module budgets"): a plain turn, a tool turn of
 * two rounds with two calls each, and a tool turn on a full history whose tool results
 * are 64 bytes or 8 KiB. The plain and tool turns end by writing their trace as JSON, as
 * a host that stores it does. The core allocates nothing after pr3_core_init (no file under src/ may
 * name an allocator, tools/check-banned.sh), so a turn's memory is the high-water marks
 * of its regions, printed after each workload. */
#include "pr3_bench.h"

#include <pr3/core.h>

enum { TRACE_BYTES = 256 * 1024, BIG_OUTPUT = 8 * 1024, SMALL_OUTPUT = 64 };

#define CALC_PARAMS                                                                                          \
    "{\"type\":\"object\",\"properties\":{\"expression\":{\"type\":\"string\"}},\"required\":["              \
    "\"expression\"]}"
#define WEATHER_PARAMS                                                                                       \
    "{\"type\":\"object\",\"properties\":{\"city\":{\"type\":\"string\"},\"days\":{\"type\":\"integer\"},"   \
    "\"unit\":{\"type\":\"string\",\"enum\":[\"celsius\",\"fahrenheit\"]},\"include_hourly\":{\"type\":"     \
    "\"boolean\"},\"lang\":{\"type\":\"string\"}},\"required\":[\"city\"]}"

typedef struct core_case {
    pr3_core *core;
    void *mem;
    uint64_t now;
    pr3_str output; /* what every tool call returns */
    char *trace;    /* the turn's trace as JSON; NULL to leave it unwritten */
    pr3_generation_info info;
} core_case;

static bool step(core_case *c, pr3_input *in, pr3_actions *a) {
    in->at = ++c->now;
    return pr3_core_step(c->core, in, a, NULL) == PR3_OK;
}

static uint64_t request_of(const pr3_actions *a) {
    for (uint32_t i = 0; i < a->len; i++)
        if (a->items[i].kind == PR3_ACTION_INFER) return a->items[i].infer.id;
    return 0;
}

/* Writes the finished turn's trace; its length, or 0 if the turn did not end in a reply. */
static uint64_t export_trace(core_case *c, const pr3_actions *a) {
    if (a->len != 2 || a->items[0].kind != PR3_ACTION_REPLY || a->items[1].kind != PR3_ACTION_TRACE) return 0;
    if (c->trace == NULL) return a->items[1].trace->n_steps;
    pr3_buf b = pr3_buf_make(c->trace, TRACE_BYTES);
    return pr3_trace_write_json(a->items[1].trace, &b) == PR3_OK ? b.len : 0;
}

static uint64_t utter(core_case *c, pr3_str text) {
    pr3_input in = {.kind = PR3_INPUT_UTTERANCE};
    in.utterance.session = 1;
    in.utterance.content = text;
    pr3_actions a;
    return step(c, &in, &a) ? request_of(&a) : 0;
}

/* Answers request rid with calls; the ids of the calls to run, or 0 if any was refused. */
static uint32_t answer_calls(core_case *c, uint64_t rid, const pr3_proposed_call *calls, uint32_t n,
                             uint64_t *ids) {
    pr3_input in = {.kind = PR3_INPUT_MODEL_OUTPUT};
    in.model_output.request = rid;
    in.model_output.tool_calls = calls;
    in.model_output.n_tool_calls = n;
    in.model_output.info = c->info;
    pr3_actions a;
    if (!step(c, &in, &a) || a.len != n) return 0;
    for (uint32_t i = 0; i < n; i++) {
        if (a.items[i].kind != PR3_ACTION_CALL_TOOL) return 0;
        ids[i] = a.items[i].call.id;
    }
    return n;
}

/* Reports every call's result; the request the last one asks for, or 0. */
static uint64_t outcomes(core_case *c, const uint64_t *ids, uint32_t n) {
    pr3_actions a = {0};
    for (uint32_t i = 0; i < n; i++) {
        pr3_input in = {.kind = PR3_INPUT_TOOL_OUTCOME};
        in.tool_outcome.call_id = ids[i];
        in.tool_outcome.success = true;
        in.tool_outcome.output = c->output;
        if (!step(c, &in, &a)) return 0;
    }
    return request_of(&a);
}

static uint64_t reply(core_case *c, uint64_t rid, pr3_str text) {
    pr3_input in = {.kind = PR3_INPUT_MODEL_OUTPUT};
    in.model_output.request = rid;
    in.model_output.content = text;
    in.model_output.info = c->info;
    pr3_actions a;
    return rid != 0 && step(c, &in, &a) ? export_trace(c, &a) : 0;
}

/* utterance -> infer -> reply -> trace */
static uint64_t plain_turn(void *ctx) {
    core_case *c = ctx;
    return reply(c, utter(c, PR3_S("What is 2+2?")), PR3_S("2+2 is 4."));
}

/* Two rounds of two calls each, every call validated against its schema and canonicalized. */
static uint64_t tool_turn(void *ctx) {
    core_case *c = ctx;
    const pr3_proposed_call first[] = {
        {PR3_S("weather"), PR3_S("{\"unit\":\"celsius\",\"city\":\"Tehran\",\"days\":3,\"include_hourly\":"
                                 "false,\"lang\":\"fa\"}")},
        {PR3_S("calculator"), PR3_S("{ \"expression\": \"2+2\" }")},
    };
    const pr3_proposed_call second[] = {
        {PR3_S("weather"), PR3_S("{\"unit\":\"celsius\",\"city\":\"Shiraz\",\"days\":1,\"include_hourly\":"
                                 "true,\"lang\":\"fa\"}")},
        {PR3_S("calculator"), PR3_S("{\"expression\":\"6*7\"}")},
    };
    uint64_t ids[2];
    uint64_t rid = utter(c, PR3_S("The weather in Tehran and Shiraz, and 2+2 and 6*7?"));
    if (rid == 0 || answer_calls(c, rid, first, 2, ids) != 2 || (rid = outcomes(c, ids, 2)) == 0 ||
        answer_calls(c, rid, second, 2, ids) != 2 || (rid = outcomes(c, ids, 2)) == 0)
        return 0;
    return reply(c, rid, PR3_S("Sunny in both; 4 and 42."));
}

/* One call on a history kept full: each turn's four messages replace the oldest four, so
 * its two inference requests show 29 and 31 messages, 7 and 8 of them tool results. The
 * trace is not written here: the row is about what the history's size costs a turn. */
static uint64_t full_history_turn(void *ctx) {
    core_case *c = ctx;
    const pr3_proposed_call call[] = {{PR3_S("calculator"), PR3_S("{\"expression\":\"2+2\"}")}};
    uint64_t id;
    uint64_t rid = utter(c, PR3_S("And now?"));
    if (rid == 0 || answer_calls(c, rid, call, 1, &id) != 1 || (rid = outcomes(c, &id, 1)) == 0) return 0;
    return reply(c, rid, PR3_S("Still 4."));
}

static bool case_open(core_case *c, pr3_str output, char *trace) {
    memset(c, 0, sizeof *c);
    pr3_core_config cfg = pr3_core_config_default();
    size_t size = pr3_core_size(&cfg);
    c->mem = size > 0 ? malloc(size) : NULL;
    if (c->mem == NULL || pr3_core_init(c->mem, size, &cfg, &c->core) != PR3_OK) return false;
    const pr3_tool_spec tools[] = {
        {.name = PR3_S("calculator"),
         .description = PR3_S("Evaluates arithmetic."),
         .parameters = PR3_S(CALC_PARAMS)},
        {.name = PR3_S("weather"),
         .description = PR3_S("The forecast for a city."),
         .parameters = PR3_S(WEATHER_PARAMS)},
    };
    for (size_t i = 0; i < sizeof tools / sizeof tools[0]; i++)
        if (pr3_core_register_tool(c->core, &tools[i], NULL) != PR3_OK) return false;
    c->output = output;
    c->trace = trace;
    c->info = (pr3_generation_info){PR3_S("qwen3.5-2b"), PR3_S("pr3"), 120, 8};
    return true;
}

static void case_report(const core_case *c, const char *workload) {
    pr3_core_memory m;
    pr3_core_memory_report(c->core, &m);
    (void)printf("# %s memory: core %zu bytes; heap high water %zu of %zu; step %zu of %zu; turn %zu of %zu; "
                 "static %zu of %zu\n",
                 workload, m.total, m.heap_high_water, m.heap_cap, m.step_high_water, m.step_cap,
                 m.turn_high_water, m.turn_cap, m.static_used, m.static_cap);
}

static void case_close(core_case *c) {
    if (c->core != NULL) pr3_core_destroy(c->core);
    free(c->mem);
    memset(c, 0, sizeof *c);
}

int main(int argc, char **argv) {
    pr3_bench b;
    if (!pr3_bench_init(&b, "core", argc, argv)) return 2;
    char *trace = malloc(TRACE_BYTES), *big = malloc(BIG_OUTPUT);
    if (trace == NULL || big == NULL) {
        (void)fprintf(stderr, "bench: out of memory\n");
        free(trace);
        free(big);
        return 2;
    }
    static const char line[] = "Sunny, 31 degrees, light wind from the north-west; no rain expected. ";
    for (size_t i = 0; i < BIG_OUTPUT; i++) big[i] = line[i % (sizeof line - 1)];
    static const struct {
        const char *name;
        pr3_bench_fn fn;
        size_t output;
        bool trace;
    } workloads[] = {
        {"plain_turn", plain_turn, SMALL_OUTPUT, true},
        {"tool_turn", tool_turn, SMALL_OUTPUT, true},
        {"full_history.64b", full_history_turn, SMALL_OUTPUT, false},
        {"full_history.8k", full_history_turn, BIG_OUTPUT, false},
    };
    for (size_t i = 0; i < sizeof workloads / sizeof workloads[0]; i++) {
        if (b.filter && !strstr(workloads[i].name, b.filter)) continue;
        core_case c;
        if (!case_open(&c, (pr3_str){big, workloads[i].output}, workloads[i].trace ? trace : NULL)) {
            (void)printf("%-28s FAILED: the core could not be set up\n", workloads[i].name);
            b.failures++;
            case_close(&c);
            continue;
        }
        pr3_bench_run(&b, workloads[i].name, 0, workloads[i].fn, &c);
        case_report(&c, workloads[i].name);
        case_close(&c);
    }
    free(big);
    free(trace);
    return pr3_bench_finish(&b);
}
