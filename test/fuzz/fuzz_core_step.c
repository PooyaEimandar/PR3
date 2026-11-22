/* The agent runtime under libFuzzer. The input is a script: its first byte picks the
 * limits, and every operation after it is a step, a registration, a session call or a
 * query, with its strings cut from the input and clipped to valid UTF-8. Two cores in
 * separate blocks run the script in lockstep. For any script:
 *   - both cores return the same status, error and actions, and answer every query the
 *     same, byte for byte;
 *   - a refusal names its error, and its status is that error's; the core is as it was:
 *     every session's history and turn, and the live sessions, turns and heap bytes;
 *   - an inference request offers its tools sorted by name, and every tool result in it
 *     that names a call answers one an earlier assistant message made, once (the note
 *     on calls over the per-message limit names none); every call in it, and in the
 *     history of a session with no turn running, has its result;
 *   - a call to run names a registered tool that is not consequential, a proposal one
 *     that is, and its arguments are a canonical JSON object within the limit;
 *   - a reply or a failure ends its session's turn and comes with a trace that writes
 *     canonical JSON; an inference request leaves the turn running;
 *   - no region passes its size, and once every turn is aborted and every session
 *     forgotten, no session, turn or heap byte is in use. */
#include <pr3/core.h>
#include <pr3/json.h>

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define FUZZ_REQUIRE(cond)                                                                                   \
    do {                                                                                                     \
        if (!(cond)) {                                                                                       \
            (void)fprintf(stderr, "%s:%d: %s\n", __FILE__, __LINE__, #cond);                                 \
            abort();                                                                                         \
        }                                                                                                    \
    } while (0)

enum {
    SESSIONS = 6,  /* ids 0..5 against three session records */
    OPS_MAX = 400, /* operations one input may run */
    IDS = 16,      /* recent request and call ids to answer from */
    TOOLS_MAX = 8, /* the four fixed tools and the input's own */
    PROPOSED = 5,  /* calls in one model message: one more than any limit here */
    LABELS = 20,   /* origin labels in one outcome: more than the 16 kept */
    RESTORED = 14, /* messages in one restore: more than any history here */
    ARGS_BYTES = 640,
    OUT_BYTES = 1 << 18, /* a trace's JSON under these limits */
    SCRATCH_BYTES = 4 << 20,
};

/* ---- The script */

typedef struct script {
    const uint8_t *p;
    size_t n;
} script;

static uint8_t u8(script *s) {
    if (s->n == 0) return 0;
    s->n--;
    return *s->p++;
}

/* A string of up to 4095 bytes from the input, cut back to its valid UTF-8 prefix. */
static pr3_str text(script *s) {
    size_t len = u8(s);
    if (len >= 0xf0) len = (len & 0x0f) << 8 | u8(s);
    if (len > s->n) len = s->n;
    const uint8_t *p = s->p;
    s->p += len;
    s->n -= len;
    return (pr3_str){(const char *)p, pr3_utf8_valid_prefix(p, len)};
}

/* ---- Digests: FNV-1a over everything a caller can see */

typedef struct digest {
    uint64_t h;
} digest;

static void mix(digest *d, const void *p, size_t n) {
    const uint8_t *b = p;
    for (size_t i = 0; i < n; i++) d->h = (d->h ^ b[i]) * 0x100000001b3u;
}

static void mix_u64(digest *d, uint64_t v) { mix(d, &v, sizeof v); }

static void mix_str(digest *d, pr3_str s) {
    mix_u64(d, s.len);
    mix(d, s.ptr, s.len);
}

static void mix_calls(digest *d, const pr3_tool_call *c, uint32_t n) {
    mix_u64(d, n);
    for (uint32_t i = 0; i < n; i++) {
        mix_u64(d, c[i].id);
        mix_str(d, c[i].name);
        mix_str(d, c[i].arguments);
    }
}

static void mix_messages(digest *d, const pr3_message *m, uint32_t n) {
    mix_u64(d, n);
    for (uint32_t i = 0; i < n; i++) {
        mix_u64(d, m[i].role);
        mix_str(d, m[i].content);
        mix_u64(d, m[i].at);
        mix_u64(d, m[i].tool_call_id);
        mix_calls(d, m[i].tool_calls, m[i].n_tool_calls);
    }
}

static void mix_tools(digest *d, const pr3_tool_spec *t, uint32_t n) {
    mix_u64(d, n);
    for (uint32_t i = 0; i < n; i++) {
        mix_str(d, t[i].name);
        mix_str(d, t[i].description);
        mix_str(d, t[i].parameters);
        mix_u64(d, (uint64_t)t[i].requires_confirmation << 1 | (uint64_t)t[i].untrusted_output);
        mix_u64(d, t[i].n_required_capabilities);
        for (uint32_t k = 0; k < t[i].n_required_capabilities; k++) mix_str(d, t[i].required_capabilities[k]);
    }
}

/* ---- The world: two cores, what the script registered, and the ids it can answer */

typedef struct known {
    char name[PR3_NAME_MAX_BYTES];
    size_t len;
    bool confirm, untrusted;
} known;

typedef struct ring {
    uint64_t v[IDS];
    uint32_t n, at;
} ring;

typedef struct world {
    pr3_core *core[2];
    pr3_core_config cfg;
    known tools[TOOLS_MAX];
    uint32_t n_tools;
    ring requests, calls;
    pr3_timestamp clock;
} world;

static uint8_t scratch_mem[SCRATCH_BYTES];
static char out_mem[OUT_BYTES], canon_mem[OUT_BYTES];
static char args_mem[PROPOSED][ARGS_BYTES], schema_mem[ARGS_BYTES];
static pr3_arena scratch;

static void ring_add(ring *r, uint64_t v) {
    r->v[r->at] = v;
    r->at = (r->at + 1) % IDS;
    if (r->n < IDS) r->n++;
}

/* One of the recent ids, newest most often, or now and then one nothing issued. */
static uint64_t ring_pick(const ring *r, script *s) {
    uint8_t k = u8(s);
    if (r->n == 0 || k >= 0xf8) return k & 7u;
    return r->v[(r->at + IDS - 1 - (k % r->n)) % IDS];
}

static const known *tool_find(const world *w, pr3_str name) {
    for (uint32_t i = 0; i < w->n_tools; i++)
        if (w->tools[i].len == name.len && memcmp(w->tools[i].name, name.ptr, name.len) == 0)
            return &w->tools[i];
    return NULL;
}

/* Canonical JSON under PR3_JSON flags 0, written to out_mem. */
static pr3_str canonical(pr3_str json, pr3_json_kind *kind) {
    pr3_arena_reset(&scratch);
    pr3_json_tape t;
    pr3_status s = pr3_json_parse(&scratch, (pr3_bytes){(const uint8_t *)json.ptr, json.len}, &t, NULL);
    FUZZ_REQUIRE(s == PR3_OK);
    if (kind != NULL) *kind = pr3_json_kind_of(&t, 0);
    pr3_buf b = pr3_buf_make(canon_mem, sizeof canon_mem);
    FUZZ_REQUIRE(pr3_json_write(&t, 0, &scratch, &b, 0) == PR3_OK);
    return (pr3_str){b.ptr, b.len};
}

static pr3_str trace_json(const pr3_trace *t) {
    pr3_buf b = pr3_buf_make(out_mem, sizeof out_mem);
    FUZZ_REQUIRE(pr3_trace_write_json(t, &b) == PR3_OK);
    return (pr3_str){b.ptr, b.len};
}

static void mix_actions(digest *d, const pr3_actions *a) {
    mix_u64(d, a->len);
    for (uint32_t i = 0; i < a->len; i++) {
        const pr3_action *x = &a->items[i];
        mix_u64(d, x->kind);
        switch (x->kind) {
        case PR3_ACTION_INFER:
            mix_u64(d, x->infer.id);
            mix_u64(d, x->infer.session);
            mix_messages(d, x->infer.messages, x->infer.n_messages);
            mix_tools(d, x->infer.tools, x->infer.n_tools);
            break;
        case PR3_ACTION_CALL_TOOL:
        case PR3_ACTION_PROPOSE: mix_calls(d, &x->call, 1); break;
        case PR3_ACTION_REPLY:
            mix_u64(d, x->reply.session);
            mix_str(d, x->reply.content);
            break;
        case PR3_ACTION_FAILED:
            mix_u64(d, x->failed.session);
            mix_str(d, x->failed.reason);
            break;
        case PR3_ACTION_TRACE: mix_str(d, trace_json(x->trace)); break;
        default: FUZZ_REQUIRE(false);
        }
    }
}

/* What a refusal must leave as it was. */
static uint64_t fingerprint(const pr3_core *core) {
    digest d = {0xcbf29ce484222325u};
    for (pr3_session_id id = 0; id < SESSIONS; id++) {
        const pr3_message *m;
        uint32_t n;
        pr3_core_history(core, id, &m, &n);
        mix_messages(&d, m, n);
        mix_u64(&d, pr3_core_in_turn(core, id));
    }
    pr3_core_memory mem;
    pr3_core_memory_report(core, &mem);
    mix_u64(&d, mem.sessions_live);
    mix_u64(&d, mem.turns_live);
    mix_u64(&d, mem.heap_used);
    mix_u64(&d, mem.static_used);
    return d.h;
}

/* ---- Checks */

static pr3_status status_of(pr3_core_error_kind k) {
    switch (k) {
    case PR3_CORE_ERROR_NONE: return PR3_OK;
    case PR3_CORE_ERROR_INPUT_TOO_LARGE: return PR3_E_LIMIT;
    case PR3_CORE_ERROR_EMPTY_INPUT:
    case PR3_CORE_ERROR_INVALID_TOOL_SPEC: return PR3_E_INVALID;
    case PR3_CORE_ERROR_UNKNOWN_REQUEST:
    case PR3_CORE_ERROR_UNKNOWN_TOOL_CALL: return PR3_E_NOT_FOUND;
    case PR3_CORE_ERROR_TURN_IN_PROGRESS: return PR3_E_STATE;
    case PR3_CORE_ERROR_OUT_OF_MEMORY: return PR3_E_NOMEM;
    }
    FUZZ_REQUIRE(false);
    return PR3_E_INVALID;
}

static void valid_text(pr3_str s) {
    FUZZ_REQUIRE(s.len == 0 || s.ptr != NULL);
    FUZZ_REQUIRE(pr3_utf8_validate((const uint8_t *)s.ptr, s.len) == PR3_OK);
}

/* Both cores answered alike, and a refusal says why and changed nothing. */
static void check_result(const world *w, const pr3_status st[2], const pr3_core_error err[2],
                         uint64_t before) {
    FUZZ_REQUIRE(st[0] == st[1] && err[0].kind == err[1].kind && err[0].a == err[1].a &&
                 err[0].b == err[1].b);
    FUZZ_REQUIRE(err[0].why_len == err[1].why_len && err[0].why_len <= PR3_CORE_WHY_BYTES);
    FUZZ_REQUIRE(memcmp(err[0].why, err[1].why, err[0].why_len) == 0);
    FUZZ_REQUIRE(status_of(err[0].kind) == st[0]);
    if (st[0] != PR3_OK) {
        char mem[2 * PR3_CORE_WHY_BYTES];
        pr3_buf b = pr3_buf_make(mem, sizeof mem);
        pr3_core_error_text(&err[0], &b);
        FUZZ_REQUIRE(!b.overflow && b.len > 0);
        valid_text((pr3_str){b.ptr, b.len});
        FUZZ_REQUIRE(fingerprint(w->core[0]) == before);
    }
    FUZZ_REQUIRE(fingerprint(w->core[0]) == fingerprint(w->core[1]));
}

/* A conversation as the model is shown it. */
static void check_messages(const pr3_message *m, uint32_t n) {
    for (uint32_t i = 0; i < n; i++) {
        valid_text(m[i].content);
        FUZZ_REQUIRE(m[i].role <= PR3_ROLE_TOOL);
        if (m[i].n_tool_calls > 0) FUZZ_REQUIRE(m[i].role == PR3_ROLE_ASSISTANT && m[i].tool_calls != NULL);
        for (uint32_t k = 0; k < m[i].n_tool_calls; k++) FUZZ_REQUIRE(m[i].tool_calls[k].id != 0);
        if (m[i].role != PR3_ROLE_TOOL || m[i].tool_call_id == 0) continue;
        uint64_t id = m[i].tool_call_id;
        bool asked = false;
        for (uint32_t j = 0; j < i && !asked; j++)
            for (uint32_t k = 0; k < m[j].n_tool_calls; k++) asked = asked || m[j].tool_calls[k].id == id;
        FUZZ_REQUIRE(asked);
        for (uint32_t j = i + 1; j < n; j++)
            FUZZ_REQUIRE(m[j].role != PR3_ROLE_TOOL || m[j].tool_call_id != id); /* answered once */
    }
}

/* Every call an assistant message makes has a result after it: none is left open. */
static void check_answered(const pr3_message *m, uint32_t n) {
    for (uint32_t i = 0; i < n; i++)
        for (uint32_t k = 0; k < m[i].n_tool_calls; k++) {
            bool answered = false;
            for (uint32_t j = i + 1; j < n && !answered; j++)
                answered = m[j].role == PR3_ROLE_TOOL && m[j].tool_call_id == m[i].tool_calls[k].id;
            FUZZ_REQUIRE(answered);
        }
}

static void check_tools(const pr3_tool_spec *t, uint32_t n) {
    for (uint32_t i = 1; i < n; i++) {
        size_t a = t[i - 1].name.len, b = t[i].name.len;
        int c = memcmp(t[i - 1].name.ptr, t[i].name.ptr, a < b ? a : b);
        FUZZ_REQUIRE(c < 0 || (c == 0 && a < b));
    }
}

static void check_call(world *w, const pr3_tool_call *c, bool propose) {
    const known *k = tool_find(w, c->name);
    FUZZ_REQUIRE(k != NULL && k->confirm == propose && c->id != 0);
    FUZZ_REQUIRE(c->arguments.len <= w->cfg.max_tool_args_bytes);
    pr3_json_kind kind;
    pr3_str canon = canonical(c->arguments, &kind);
    FUZZ_REQUIRE(kind == PR3_JSON_OBJECT && canon.len == c->arguments.len);
    FUZZ_REQUIRE(memcmp(canon.ptr, c->arguments.ptr, canon.len) == 0);
    ring_add(&w->calls, c->id);
}

static void check_trace(const pr3_trace *t, pr3_session_id session) {
    FUZZ_REQUIRE(t->session == session && t->ended_at >= t->started_at);
    pr3_str json = trace_json(t);
    pr3_json_kind kind;
    pr3_str canon = canonical(json, &kind);
    FUZZ_REQUIRE(kind == PR3_JSON_OBJECT && canon.len == json.len &&
                 memcmp(canon.ptr, json.ptr, json.len) == 0);
}

static void check_actions(world *w, const pr3_actions *a) {
    for (uint32_t i = 0; i < a->len; i++) {
        const pr3_action *x = &a->items[i];
        pr3_session_id ended;
        switch (x->kind) {
        case PR3_ACTION_INFER:
            FUZZ_REQUIRE(x->infer.n_messages > 0 && pr3_core_in_turn(w->core[0], x->infer.session));
            check_messages(x->infer.messages, x->infer.n_messages);
            check_answered(x->infer.messages, x->infer.n_messages);
            check_tools(x->infer.tools, x->infer.n_tools);
            ring_add(&w->requests, x->infer.id);
            continue;
        case PR3_ACTION_CALL_TOOL: check_call(w, &x->call, false); continue;
        case PR3_ACTION_PROPOSE: check_call(w, &x->call, true); continue;
        case PR3_ACTION_REPLY:
            ended = x->reply.session;
            valid_text(x->reply.content);
            break;
        case PR3_ACTION_FAILED:
            ended = x->failed.session;
            valid_text(x->failed.reason);
            break;
        case PR3_ACTION_TRACE: FUZZ_REQUIRE(false); continue; /* only right after its turn's end */
        default: FUZZ_REQUIRE(false); continue;
        }
        FUZZ_REQUIRE(!pr3_core_in_turn(w->core[0], ended));
        FUZZ_REQUIRE(i + 1 < a->len && a->items[i + 1].kind == PR3_ACTION_TRACE);
        check_trace(a->items[++i].trace, ended);
    }
}

/* ---- Operations */

static void step(world *w, const pr3_input *in) {
    uint64_t before = fingerprint(w->core[0]);
    pr3_actions out[2];
    pr3_core_error err[2];
    pr3_status st[2];
    for (int c = 0; c < 2; c++) st[c] = pr3_core_step(w->core[c], in, &out[c], &err[c]);
    if (st[0] == PR3_OK && st[1] == PR3_OK) {
        digest d[2] = {{0}, {0}};
        for (int c = 0; c < 2; c++) mix_actions(&d[c], &out[c]);
        FUZZ_REQUIRE(d[0].h == d[1].h);
        check_actions(w, &out[0]);
    }
    check_result(w, st, err, before);
}

/* Arguments as a model might write them: raw, or a template around a string. */
static pr3_str arguments(script *s, char *mem) {
    uint8_t shape = u8(s) % 6;
    pr3_str t = text(s);
    if (shape == 0) return t;
    pr3_buf b = pr3_buf_make(mem, ARGS_BYTES);
    switch (shape) {
    case 1: pr3_buf_put(&b, PR3_S("{\"q\":\"")); break;
    case 2: pr3_buf_put(&b, PR3_S("{\"url\":\"")); break;
    case 3: pr3_buf_put(&b, PR3_S("{\"text\":\"hi\",\"to\":\"")); break;
    case 4: pr3_buf_put(&b, PR3_S("{\"n\":7,\"expression\":\"")); break;
    default: pr3_buf_put(&b, PR3_S("{ \"expression\" : \"")); break;
    }
    pr3_buf_put_clip(&b, t, ARGS_BYTES - 64); /* room for the template and the marker */
    pr3_buf_put(&b, PR3_S("\"}"));
    FUZZ_REQUIRE(!b.overflow);
    return (pr3_str){b.ptr, pr3_utf8_valid_prefix((const uint8_t *)b.ptr, b.len)};
}

static pr3_str tool_name(const world *w, script *s) {
    uint8_t k = u8(s);
    if (k % 8 < w->n_tools) return (pr3_str){w->tools[k % 8].name, w->tools[k % 8].len};
    return text(s);
}

static void op_utterance(world *w, script *s) {
    pr3_str notes[3];
    pr3_input in = {.kind = PR3_INPUT_UTTERANCE, .at = w->clock};
    in.utterance.session = u8(s) % SESSIONS;
    in.utterance.content = text(s);
    in.utterance.n_notes = u8(s) % 4;
    for (uint32_t i = 0; i < in.utterance.n_notes; i++) notes[i] = text(s);
    in.utterance.notes = notes;
    step(w, &in);
}

static void op_model_output(world *w, script *s) {
    pr3_proposed_call calls[PROPOSED];
    pr3_input in = {.kind = PR3_INPUT_MODEL_OUTPUT, .at = w->clock};
    in.model_output.request = ring_pick(&w->requests, s);
    in.model_output.content = text(s);
    uint8_t n = u8(s);
    in.model_output.n_tool_calls = n < 96 ? 0 : (uint32_t)(n % (PROPOSED + 1));
    for (uint32_t i = 0; i < in.model_output.n_tool_calls; i++) {
        calls[i].name = tool_name(w, s);
        calls[i].arguments = arguments(s, args_mem[i]);
    }
    in.model_output.tool_calls = calls;
    in.model_output.info = (pr3_generation_info){
        .model = text(s), .engine = text(s), .prompt_tokens = u8(s), .completion_tokens = u8(s)};
    step(w, &in);
}

static void op_model_failed(world *w, script *s) {
    pr3_input in = {.kind = PR3_INPUT_MODEL_FAILED, .at = w->clock};
    in.model_failed.request = ring_pick(&w->requests, s);
    in.model_failed.reason = text(s);
    step(w, &in);
}

static void op_tool_outcome(world *w, script *s) {
    pr3_str labels[LABELS];
    pr3_input in = {.kind = PR3_INPUT_TOOL_OUTCOME, .at = w->clock};
    in.tool_outcome.call_id = ring_pick(&w->calls, s);
    uint8_t k = u8(s);
    in.tool_outcome.success = (k & 1) == 0;
    in.tool_outcome.output = text(s);
    in.tool_outcome.n_sources = k < 0xe0 ? (uint32_t)(k >> 1) % 4 : LABELS;
    for (uint32_t i = 0; i < in.tool_outcome.n_sources; i++) labels[i] = text(s);
    in.tool_outcome.sources = labels;
    step(w, &in);
}

static void op_abort(world *w, script *s) {
    pr3_input in = {.kind = PR3_INPUT_ABORT, .at = w->clock};
    in.abort_turn.session = u8(s) % SESSIONS;
    in.abort_turn.reason = text(s);
    step(w, &in);
}

/* forget, set_brief or restore, on both cores */
static void op_session(world *w, script *s) {
    uint8_t which = u8(s) % 3;
    pr3_session_id id = u8(s) % SESSIONS;
    pr3_message msgs[RESTORED];
    pr3_str brief;
    uint32_t n = 0;
    if (which == 1) {
        brief = text(s);
    } else if (which == 2) {
        n = u8(s) % (RESTORED + 1);
        for (uint32_t i = 0; i < n; i++) {
            uint8_t k = u8(s);
            msgs[i] = (pr3_message){
                .content = text(s), .at = w->clock + i, .role = (pr3_role)(k % 4), .tool_call_id = k >> 6};
        }
    }
    bool clear = which == 1 && (s->n & 1) != 0;
    uint64_t before = fingerprint(w->core[0]);
    pr3_core_error err[2];
    pr3_status st[2];
    for (int c = 0; c < 2; c++) {
        if (which == 0)
            st[c] = pr3_core_forget(w->core[c], id, &err[c]);
        else if (which == 1)
            st[c] = pr3_core_set_brief(w->core[c], id, clear ? NULL : &brief, &err[c]);
        else
            st[c] = pr3_core_restore(w->core[c], id, msgs, n, &err[c]);
    }
    check_result(w, st, err, before);
}

static void op_register(world *w, script *s) {
    pr3_str caps[2];
    uint8_t k = u8(s);
    pr3_tool_spec spec = {.name = text(s), .description = text(s)};
    pr3_str body = text(s);
    pr3_buf b = pr3_buf_make(schema_mem, sizeof schema_mem);
    switch (k % 4) {
    case 0: spec.parameters = body; break;
    case 1:
        pr3_buf_put(&b, PR3_S("{\"type\":\"object\",\"properties\":{"));
        pr3_buf_put_clip(&b, body, ARGS_BYTES - 64);
        pr3_buf_put(&b, PR3_S("}}"));
        break;
    case 2:
        pr3_buf_put(
            &b, PR3_S("{\"type\":\"object\",\"required\":[\"a\"],\"properties\":{\"a\":{\"type\":\"string\","
                      "\"enum\":["));
        pr3_buf_put_clip(&b, body, ARGS_BYTES - 128);
        pr3_buf_put(&b, PR3_S("]}}}"));
        break;
    default:
        pr3_buf_put(
            &b,
            PR3_S("{\"type\":\"object\",\"additionalProperties\":true,\"properties\":{\"x\":{\"type\":\""));
        pr3_buf_put_clip(&b, body, ARGS_BYTES - 128);
        pr3_buf_put(&b, PR3_S("\"}}}"));
        break;
    }
    FUZZ_REQUIRE(!b.overflow);
    if (k % 4 != 0) spec.parameters = (pr3_str){b.ptr, pr3_utf8_valid_prefix((const uint8_t *)b.ptr, b.len)};
    spec.requires_confirmation = (k & 0x10) != 0;
    spec.untrusted_output = (k & 0x20) != 0;
    spec.n_required_capabilities = k >> 6 < 3 ? k >> 6 : 0;
    for (uint32_t i = 0; i < spec.n_required_capabilities; i++) caps[i] = text(s);
    spec.required_capabilities = caps;

    uint64_t before = fingerprint(w->core[0]);
    pr3_core_error err[2];
    pr3_status st[2];
    for (int c = 0; c < 2; c++) st[c] = pr3_core_register_tool(w->core[c], &spec, &err[c]);
    check_result(w, st, err, before);
    if (st[0] != PR3_OK) return;
    FUZZ_REQUIRE(w->n_tools < TOOLS_MAX && spec.name.len <= PR3_NAME_MAX_BYTES &&
                 tool_find(w, spec.name) == NULL);
    known *t = &w->tools[w->n_tools++];
    memcpy(t->name, spec.name.ptr, spec.name.len);
    t->len = spec.name.len;
    t->confirm = spec.requires_confirmation;
    t->untrusted = spec.untrusted_output;
}

/* Every query, asked of both cores: the same answer, and a sound one. */
static void op_query(world *w, script *s) {
    pr3_str labels[LABELS];
    uint8_t which = u8(s) % 6;
    pr3_session_id id = u8(s) % SESSIONS;
    pr3_str arg = text(s), name = which == 4 ? tool_name(w, s) : PR3_S("");
    uint32_t n_labels = which == 4 ? u8(s) % (LABELS + 1) : 0;
    for (uint32_t i = 0; i < n_labels; i++) labels[i] = text(s);
    digest d[2] = {{0}, {0}};
    bool got[2] = {false, false};
    for (int c = 0; c < 2; c++) {
        pr3_core *core = w->core[c];
        const pr3_message *m;
        const pr3_tool_call *calls;
        const pr3_str *strs;
        const pr3_tool_spec *tools;
        pr3_str opening;
        uint32_t n, n_tools;
        switch (which) {
        case 0:
            pr3_core_history(core, id, &m, &n);
            check_messages(m, n);
            if (!pr3_core_in_turn(core, id)) check_answered(m, n);
            mix_messages(&d[c], m, n);
            break;
        case 1:
            pr3_core_untrusted_calls(core, id, &calls, &n);
            for (uint32_t i = 0; i < n; i++) {
                const known *k = tool_find(w, calls[i].name);
                FUZZ_REQUIRE(k != NULL && k->untrusted);
            }
            mix_calls(&d[c], calls, n);
            break;
        case 2:
            pr3_core_untrusted_sources(core, id, &strs, &n);
            for (uint32_t i = 0; i < n; i++) mix_str(&d[c], strs[i]);
            break;
        case 3:
            got[c] = pr3_core_upcoming(core, id, &m, &n, &tools, &n_tools);
            if (!got[c]) break;
            FUZZ_REQUIRE(!pr3_core_in_turn(core, id));
            check_messages(m, n);
            check_answered(m, n);
            check_tools(tools, n_tools);
            mix_messages(&d[c], m, n);
            mix_tools(&d[c], tools, n_tools);
            break;
        case 4:
            got[c] = pr3_core_tool_sources(core, name, labels, n_labels, &strs, &n);
            if (!got[c]) break;
            FUZZ_REQUIRE(n <= 16);
            for (uint32_t i = 0; i < n; i++) {
                FUZZ_REQUIRE(strs[i].len > 0 && strs[i].len <= 256 + sizeof " …[truncated]");
                valid_text(strs[i]);
                mix_str(&d[c], strs[i]);
            }
            break;
        default:
            got[c] = pr3_core_opening(core, arg, &opening, &tools, &n_tools);
            if (!got[c]) break;
            valid_text(opening);
            check_tools(tools, n_tools);
            mix_str(&d[c], opening);
            mix_tools(&d[c], tools, n_tools);
            break;
        }
    }
    FUZZ_REQUIRE(got[0] == got[1] && d[0].h == d[1].h);
}

/* ---- The tools every script starts with */

typedef struct fixed_tool {
    const char *name, *parameters;
    bool confirm, untrusted;
} fixed_tool;

static const fixed_tool FIXED[] = {
    {"calc",
     "{\"type\":\"object\",\"properties\":{\"expression\":{\"type\":\"string\"},\"n\":{\"type\":\"integer\"}}"
     ","
     "\"required\":[\"expression\"]}",
     false, false},
    {"search", "{\"type\":\"object\",\"properties\":{\"q\":{\"type\":\"string\"}},\"required\":[\"q\"]}",
     false, false},
    {"send_message",
     "{\"type\":\"object\",\"properties\":{\"text\":{\"type\":\"string\"},\"to\":{\"type\":\"string\"}},"
     "\"required\":[\"to\",\"text\"]}",
     true, false},
    {"web_fetch",
     "{\"type\":\"object\",\"properties\":{\"url\":{\"type\":\"string\"}},\"required\":[\"url\"]}", false,
     true},
};

/* Small limits, so that pools fill, histories trim and the heap runs out. */
static pr3_core_config config_of(uint8_t k) {
    pr3_core_config c = pr3_core_config_default();
    c.max_input_bytes = 80;
    c.max_notes_bytes = 48;
    c.max_history = 3u + (k & 7u);
    c.max_tool_rounds = (k >> 3) & 3u;
    c.max_tool_calls_per_round = (k & 0x20) != 0 ? 3 : 1;
    c.max_tool_calls_per_turn = 4;
    c.max_tool_args_bytes = 96;
    c.max_tool_output_bytes = 64;
    c.max_sessions = 3;
    c.max_turns = 2;
    c.max_tools = 6;
    c.history_heap_bytes = (size_t)1 << (12u + (k >> 6));
    return c;
}

/* libFuzzer's entry point, so it stays external. */
int LLVMFuzzerTestOneInput(const uint8_t *data, size_t size); // NOLINT(misc-use-internal-linkage)

int LLVMFuzzerTestOneInput(const uint8_t *data, size_t size) { // NOLINT(misc-use-internal-linkage)
    script s = {data, size};
    world w = {.cfg = config_of(u8(&s))};
    pr3_arena_init(&scratch, scratch_mem, sizeof scratch_mem, "fuzz");
    size_t need = pr3_core_size(&w.cfg);
    FUZZ_REQUIRE(need > 0);
    void *mem[2] = {malloc(need), malloc(need + 1)};
    FUZZ_REQUIRE(mem[0] != NULL && mem[1] != NULL);
    FUZZ_REQUIRE(pr3_core_init(mem[0], need, &w.cfg, &w.core[0]) == PR3_OK);
    FUZZ_REQUIRE(pr3_core_init((char *)mem[1] + 1, need, &w.cfg, &w.core[1]) == PR3_OK); /* any alignment */
    for (size_t i = 0; i < PR3_ARRAY_LEN(FIXED); i++) {
        pr3_tool_spec spec = {.name = {FIXED[i].name, strlen(FIXED[i].name)},
                              .description = PR3_S("a tool"),
                              .parameters = {FIXED[i].parameters, strlen(FIXED[i].parameters)},
                              .requires_confirmation = FIXED[i].confirm,
                              .untrusted_output = FIXED[i].untrusted};
        for (int c = 0; c < 2; c++) FUZZ_REQUIRE(pr3_core_register_tool(w.core[c], &spec, NULL) == PR3_OK);
        known *t = &w.tools[w.n_tools++];
        memcpy(t->name, spec.name.ptr, spec.name.len);
        t->len = spec.name.len;
        t->confirm = FIXED[i].confirm;
        t->untrusted = FIXED[i].untrusted;
    }

    for (uint32_t op = 0; op < OPS_MAX && s.n > 0; op++) {
        w.clock += u8(&s);
        switch (u8(&s) % 12) {
        case 0:
        case 1: op_utterance(&w, &s); break;
        case 2:
        case 3:
        case 4: op_model_output(&w, &s); break;
        case 5: op_model_failed(&w, &s); break;
        case 6:
        case 7: op_tool_outcome(&w, &s); break;
        case 8: op_abort(&w, &s); break;
        case 9: op_session(&w, &s); break;
        case 10: op_register(&w, &s); break;
        default: op_query(&w, &s); break;
        }
    }

    /* End every turn and forget every session: nothing may stay behind. */
    for (pr3_session_id id = 0; id < SESSIONS; id++) {
        w.clock++;
        if (pr3_core_in_turn(w.core[0], id)) {
            pr3_input in = {.kind = PR3_INPUT_ABORT, .at = w.clock, .abort_turn = {.session = id}};
            step(&w, &in);
        }
        for (int c = 0; c < 2; c++) FUZZ_REQUIRE(pr3_core_forget(w.core[c], id, NULL) == PR3_OK);
    }
    for (int c = 0; c < 2; c++) {
        pr3_core_memory m;
        pr3_core_memory_report(w.core[c], &m);
        FUZZ_REQUIRE(m.sessions_live == 0 && m.turns_live == 0 && m.heap_used == 0);
        FUZZ_REQUIRE(m.static_used <= m.static_cap && m.heap_high_water <= m.heap_cap);
        FUZZ_REQUIRE(m.step_high_water <= m.step_cap && m.turn_high_water <= m.turn_cap);
        pr3_core_destroy(w.core[c]);
    }
    free(mem[0]);
    free(mem[1]);
    return 0;
}
