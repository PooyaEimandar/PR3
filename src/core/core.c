/* The agent runtime (doc/04): its layout in the host's block, sessions and their
 * history, the turn, and the queries a host asks between steps.
 *
 * Every mutating call checks before it commits: the only allocations that can fail are
 * history-heap blocks, and all of them are taken before anything changes, so a failure
 * leaves the core as it was. The turn and step arenas are sized for the worst case the
 * configured limits allow, so running out of them is a broken invariant, not an input
 * error. A finished turn's slot is released at the start of the next mutating call, so
 * the trace the last actions point at stays valid until then. */
#include "core_internal.h"

#include <pr3/poison.h>

#define NOTES_PREFIX PR3_S(CORE_NOTES_PREFIX)
#define BRIEF_PREFIX PR3_S(CORE_BRIEF_PREFIX)
#define REASON_MAX (PR3_CORE_REASON_BYTES + CORE_MARKER_BYTES)
#define SHOWN_NAME_MAX (PR3_NAME_MAX_BYTES + CORE_MARKER_BYTES)
#define SHOWN_ARGS_MAX (PR3_CORE_REFUSED_ARGS_BYTES + CORE_MARKER_BYTES)
#define LABEL_MAX (PR3_CORE_SOURCE_BYTES + CORE_MARKER_BYTES)
#define MODEL_MAX (PR3_CORE_MODEL_NAME_BYTES + CORE_MARKER_BYTES)
#define ERROR_PREFIX PR3_S("error: ")
#define EMPTY PR3_S("")

/* The loop guard's SipHash key. Fixed: the guard compares bytes on every hash match, so
 * the key only spreads entries, it does not defend against chosen collisions. */
static const uint8_t guard_key[16] = {'p', 'r', '3', ' ', 'l', 'o', 'o', 'p',
                                      ' ', 'g', 'u', 'a', 'r', 'd', ' ', 'k'};

/* A history-heap block: one per message, holding its calls, its source labels and every
 * byte it points at. */
typedef struct core_block {
    void *mem;
    pr3_tool_call *calls;
    pr3_str *sources;
    pr3_buf text;
} core_block;

/* One considered proposal while a model message is checked. */
typedef struct staged {
    pr3_buf args;   /* canonical arguments, when valid */
    pr3_buf reason; /* why it was refused, otherwise */
    uint32_t tool;
    bool ok;
} staged;

/* ---- Configuration and sizes */

pr3_core_config pr3_core_config_default(void) {
    return (pr3_core_config){
        .max_input_bytes = PR3_DEFAULT_MAX_INPUT_BYTES,
        .max_notes_bytes = PR3_DEFAULT_MAX_NOTES_BYTES,
        .max_history = PR3_DEFAULT_MAX_HISTORY,
        .max_tool_rounds = PR3_DEFAULT_MAX_TOOL_ROUNDS,
        .max_tool_calls_per_round = PR3_DEFAULT_MAX_TOOL_CALLS_PER_ROUND,
        .max_tool_calls_per_turn = PR3_DEFAULT_MAX_TOOL_CALLS_PER_TURN,
        .max_tool_args_bytes = PR3_DEFAULT_MAX_TOOL_ARGS_BYTES,
        .max_tool_output_bytes = PR3_DEFAULT_MAX_TOOL_OUTPUT_BYTES,
        .max_sessions = PR3_DEFAULT_MAX_SESSIONS,
        .max_turns = PR3_DEFAULT_MAX_TURNS,
        .max_tools = PR3_DEFAULT_MAX_TOOLS,
        .tool_bytes = (size_t)PR3_DEFAULT_TOOL_BYTES,
        .history_heap_bytes = (size_t)PR3_DEFAULT_HISTORY_HEAP_BYTES,
    };
}

typedef struct core_sizes {
    uint32_t hist_cap, steps_cap, props_cap, pending_cap, seen_cap, actions_cap;
    size_t user_max, turn_bytes, step_bytes, q_upcoming, q_opening, q_calls, q_sources, q_labels;
} core_sizes;

/* Checked arithmetic that remembers an overflow. */
typedef struct sum {
    bool bad;
} sum;

static size_t add(sum *k, size_t a, size_t b) {
    size_t r;
    if (pr3_ckd_add(&r, a, b)) k->bad = true;
    return k->bad ? 0 : r;
}

static size_t mul(sum *k, size_t a, size_t b) {
    size_t r;
    if (pr3_ckd_mul(&r, a, b)) k->bad = true;
    return k->bad ? 0 : r;
}

static size_t max_of(size_t a, size_t b) { return a > b ? a : b; }

/* The capacities the limits imply (doc/04, "Memory layout"). */
static bool sizes_of(const pr3_core_config *c, core_sizes *z) {
    if (c->max_sessions == 0 || c->max_turns == 0 || pr3_buddy_bytes(c->history_heap_bytes) == 0)
        return false;
    sum k = {false};
    size_t R = c->max_tool_rounds, C = c->max_tool_calls_per_round, T = c->max_tool_calls_per_turn;
    size_t I = c->max_input_bytes, N = c->max_notes_bytes, A = c->max_tool_args_bytes,
           O = c->max_tool_output_bytes;
    size_t M = CORE_MARKER_BYTES, slack = CORE_SLACK;

    /* A turn's exchange: its question, per round the assistant's message, one result per
     * considered call and a note, then the answer. Trimming never splits it. */
    size_t exchange = add(&k, 2, mul(&k, R, add(&k, C, 2)));
    size_t hist = max_of(max_of(c->max_history, 1), exchange);
    size_t steps = add(&k, add(&k, R, 3), mul(&k, R, add(&k, C, 1)));
    size_t props = mul(&k, add(&k, R, 1), C);
    size_t seen = add(&k, T, 1), actions = max_of(C, 2) + 1;
    const size_t big = UINT32_MAX / 64;
    if (k.bad || hist > big || steps > big || props > big || seen > big || actions > big ||
        c->max_sessions > big || c->max_turns > big || c->max_tools > big)
        return false;

    size_t user = add(&k, add(&k, add(&k, I, NOTES_PREFIX.len), mul(&k, N, 4)), 1);
    size_t user_max = add(&k, user, M); /* restored text is clipped to `user` */

    /* Turn bytes: the query, each generation's names, text and recorded proposals, each
     * call's arguments and result (a refused one in outline), the notes, a failure's
     * reason and the result. */
    size_t gen =
        add(&k, add(&k, 2 * MODEL_MAX, add(&k, I, M)), mul(&k, C, add(&k, SHOWN_NAME_MAX, add(&k, A, M))));
    size_t valid_call = add(&k, add(&k, A, O), M + ERROR_PREFIX.len);
    size_t refused_call = SHOWN_NAME_MAX + SHOWN_ARGS_MAX + ERROR_PREFIX.len + REASON_MAX;
    size_t round = add(&k, mul(&k, C, max_of(valid_call, refused_call)), CORE_NOTE_MAX);
    size_t turn = add(&k, add(&k, I, mul(&k, add(&k, R, 1), gen)), mul(&k, R, round));
    turn = add(&k, turn, REASON_MAX + CORE_RESULT_MAX);

    /* The step arena: the actions, each considered call's arguments and refusal, the
     * validation scratch, and an inference request's copied messages, joined first
     * message and budget; or a restore's plan, or a brief; or a registration. */
    size_t validate = core_tool_scratch_bytes(A),
           registration = core_tool_scratch_bytes((size_t)PR3_TOOL_PARAMETERS_MAX_BYTES);
    if (validate == 0 || registration == 0) return false;
    size_t joined = add(&k, add(&k, BRIEF_PREFIX.len, add(&k, N, M)), add(&k, 2, user_max));
    size_t msgs = mul(&k, hist + 1, sizeof(pr3_message));
    size_t per_call = add(&k, A, REASON_MAX + sizeof(staged) + sizeof(core_block) + 2 * slack);
    size_t step = add(&k, mul(&k, actions, sizeof(pr3_action)), slack);
    step = add(&k, step, add(&k, mul(&k, C, per_call), 2 * sizeof(core_block) + 2 * slack));
    step = add(&k, step, CORE_RESULT_MAX + 64 + 2 * slack); /* why a turn stopped */
    step = add(&k, step, validate);
    step = add(&k, step, add(&k, add(&k, msgs, joined), CORE_BUDGET_MAX + 3 * slack));
    step = add(&k, step, mul(&k, hist + 1, sizeof(uint32_t) + sizeof(core_block))); /* a restore's plan */
    step = add(&k, step, add(&k, add(&k, N, M), 3 * slack));                        /* and a brief */
    step = max_of(step, registration + slack);

    *z = (core_sizes){
        .hist_cap = (uint32_t)hist,
        .steps_cap = (uint32_t)steps,
        .props_cap = (uint32_t)(props > 0 ? props : 1),
        .pending_cap = (uint32_t)max_of(C, 1),
        .seen_cap = (uint32_t)seen,
        .actions_cap = (uint32_t)actions,
        .user_max = user,
        .turn_bytes = turn,
        .step_bytes = step,
        .q_upcoming = add(&k, add(&k, msgs, joined), 2 * slack),
        .q_opening = add(&k, add(&k, BRIEF_PREFIX.len, add(&k, N, M + 2)), slack),
        .q_calls = add(&k, mul(&k, mul(&k, hist, max_of(C, 1)), sizeof(pr3_tool_call)), slack),
        .q_sources = add(&k, mul(&k, mul(&k, hist, PR3_CORE_SOURCES_PER_RESULT), sizeof(pr3_str)), slack),
        .q_labels = PR3_CORE_SOURCES_PER_RESULT * (sizeof(pr3_str) + LABEL_MAX) + 2 * slack,
    };
    return !k.bad;
}

/* Carves regions from the host's block, or only adds up their cost when `from` is NULL.
 * Each region costs its size plus its worst alignment padding and red zone, so a block of
 * the summed size fits them at any alignment. */
typedef struct carver {
    pr3_arena *from;
    size_t need;
    bool bad;
} carver;

static void *carve(carver *k, size_t n, size_t size, size_t align) {
    size_t bytes, cost, pad = (align < 8 ? 8 : align) - 1 + PR3_ARENA_REDZONE;
    if (k->bad || pr3_ckd_mul(&bytes, n, size) || pr3_ckd_add(&cost, bytes, pad) ||
        pr3_ckd_add(&k->need, k->need, cost)) {
        k->bad = true;
        return NULL;
    }
    if (k->from == NULL) return NULL;
    void *p = pr3_arena_alloc(k->from, bytes, align);
    PR3_CHECK(p != NULL); /* the block was checked against the same sum */
    return p;
}

static void *carve_nm(carver *k, size_t n, size_t m, size_t size, size_t align) {
    size_t nm;
    if (pr3_ckd_mul(&nm, n, m)) {
        k->bad = true;
        return NULL;
    }
    return carve(k, nm, size, align);
}

static void sub_arena(carver *k, pr3_arena *a, size_t cap, const char *name) {
    void *p = carve(k, cap, 1, 16);
    if (a != NULL) pr3_arena_init(a, p, cap, name);
}

#define CARVE(k, T, n) ((T *)carve((k), (n), sizeof(T), _Alignof(T)))

static pr3_core *lay_out(const pr3_core_config *c, const core_sizes *z, carver *k) {
    pr3_core *core = CARVE(k, pr3_core, 1);
    bool real = core != NULL;
    size_t S = c->max_sessions, K = c->max_turns, hist = z->hist_cap;
    pr3_tool_spec *tools = CARVE(k, pr3_tool_spec, max_of(c->max_tools, 1));
    core_schema *schemas = CARVE(k, core_schema, max_of(c->max_tools, 1));
    sub_arena(k, real ? &core->statics : NULL, c->tool_bytes, "core static");
    core_session *sessions = CARVE(k, core_session, S);
    core_index *index = CARVE(k, core_index, S);
    size_t slots = 2 * hist; /* hist is at most UINT32_MAX / 64 */
    pr3_message *msgs = (pr3_message *)carve_nm(k, S, slots, sizeof(pr3_message), _Alignof(pr3_message));
    core_meta *meta = (core_meta *)carve_nm(k, S, slots, sizeof(core_meta), _Alignof(core_meta));
    core_turn *turns = CARVE(k, core_turn, K);
    pr3_trace_step *steps =
        (pr3_trace_step *)carve_nm(k, K, z->steps_cap, sizeof(pr3_trace_step), _Alignof(pr3_trace_step));
    pr3_proposed_call *props = (pr3_proposed_call *)carve_nm(k, K, z->props_cap, sizeof(pr3_proposed_call),
                                                             _Alignof(pr3_proposed_call));
    core_pending *pending =
        (core_pending *)carve_nm(k, K, z->pending_cap, sizeof(core_pending), _Alignof(core_pending));
    core_seen *seen = (core_seen *)carve_nm(k, K, z->seen_cap, sizeof(core_seen), _Alignof(core_seen));
    char *bytes = (char *)carve_nm(k, K, z->turn_bytes, 1, 16);
    size_t heap = pr3_buddy_bytes(c->history_heap_bytes);
    if (pr3_ckd_add(&k->need, k->need, heap)) k->bad = true;
    if (real)
        PR3_CHECK(pr3_buddy_init(&core->heap, k->from, c->history_heap_bytes, "history heap") == PR3_OK);
    sub_arena(k, real ? &core->step : NULL, z->step_bytes, "core step");
    sub_arena(k, real ? &core->q_upcoming : NULL, z->q_upcoming, "core upcoming");
    sub_arena(k, real ? &core->q_opening : NULL, z->q_opening, "core opening");
    sub_arena(k, real ? &core->q_calls : NULL, z->q_calls, "core calls");
    sub_arena(k, real ? &core->q_sources : NULL, z->q_sources, "core sources");
    sub_arena(k, real ? &core->q_labels : NULL, z->q_labels, "core labels");
    if (!real) return NULL;

    core->tools = tools;
    core->schemas = schemas;
    core->sessions = sessions;
    core->index = index;
    core->turns = turns;
    for (size_t i = 0; i < S; i++)
        sessions[i] = (core_session){
            .turn = CORE_NONE, .msgs = msgs + i * slots, .meta = meta + i * slots, .brief = EMPTY};
    for (size_t i = 0; i < K; i++)
        turns[i] = (core_turn){
            .session = CORE_NONE,
            .steps = steps + i * z->steps_cap,
            .proposals = props + i * z->props_cap,
            .pending = pending + i * z->pending_cap,
            .seen = seen + i * z->seen_cap,
            .bytes = pr3_buf_make(bytes + i * z->turn_bytes, z->turn_bytes),
        };
    return core;
}

size_t pr3_core_size(const pr3_core_config *cfg) {
    core_sizes z;
    if (cfg == NULL || !sizes_of(cfg, &z)) return 0;
    carver k = {NULL, 0, false};
    (void)lay_out(cfg, &z, &k);
    return k.bad ? 0 : k.need;
}

pr3_status pr3_core_init(void *mem, size_t len, const pr3_core_config *cfg, pr3_core **out) {
    core_sizes z;
    size_t need = pr3_core_size(cfg);
    if (need == 0 || !sizes_of(cfg, &z)) return PR3_E_INVALID;
    if (mem == NULL || len < need) return PR3_E_NOMEM;
    pr3_arena root;
    pr3_arena_init(&root, mem, len, "core");
    carver k = {&root, 0, false};
    pr3_core *core = lay_out(cfg, &z, &k);
    core->cfg = *cfg;
    core->root = root;
    core->size = need;
    core->hist_cap = z.hist_cap;
    core->steps_cap = z.steps_cap;
    core->props_cap = z.props_cap;
    core->pending_cap = z.pending_cap;
    core->seen_cap = z.seen_cap;
    core->actions_cap = z.actions_cap;
    core->user_max = z.user_max;
    core->turn_bytes_cap = z.turn_bytes;
    core->turn_high_water = 0;
    core->n_tools = 0;
    core->n_index = 0;
    core->retired = CORE_NONE;
    core->acts = NULL;
    core->n_acts = 0;
    core->next_request = 1;
    core->next_call = 1;
    core->next_turn = 1;
    core->uses = 1;
    *out = core;
    return PR3_OK;
}

void pr3_core_destroy(pr3_core *core) {
    if (core == NULL) return;
    pr3_arena root = core->root; /* the core lives in the block it hands back */
    pr3_arena_fini(&root);
}

void pr3_core_memory_report(const pr3_core *core, pr3_core_memory *out) {
    uint32_t sessions = 0, turns = 0;
    for (uint32_t i = 0; i < core->cfg.max_sessions; i++) sessions += core->sessions[i].live;
    for (uint32_t i = 0; i < core->cfg.max_turns; i++) turns += core->turns[i].live && i != core->retired;
    *out = (pr3_core_memory){
        .total = core->size,
        .static_used = core->statics.used,
        .static_cap = core->statics.cap,
        .heap_used = core->heap.used,
        .heap_high_water = core->heap.high_water,
        .heap_cap = core->heap.size,
        .step_high_water = core->step.high_water,
        .step_cap = core->step.cap,
        .turn_high_water = core->turn_high_water,
        .turn_cap = core->turn_bytes_cap,
        .sessions_live = sessions,
        .sessions_cap = core->cfg.max_sessions,
        .turns_live = turns,
        .turns_cap = core->cfg.max_turns,
    };
}

/* ---- Errors */

static pr3_status fail(pr3_core_error *err, pr3_core_error_kind kind, uint64_t a, uint64_t b) {
    core_error_set(err, kind, a, b);
    switch (kind) {
    case PR3_CORE_ERROR_INPUT_TOO_LARGE: return PR3_E_LIMIT;
    case PR3_CORE_ERROR_EMPTY_INPUT: return PR3_E_INVALID;
    case PR3_CORE_ERROR_UNKNOWN_REQUEST:
    case PR3_CORE_ERROR_UNKNOWN_TOOL_CALL: return PR3_E_NOT_FOUND;
    case PR3_CORE_ERROR_TURN_IN_PROGRESS: return PR3_E_STATE;
    case PR3_CORE_ERROR_INVALID_TOOL_SPEC: return PR3_E_INVALID;
    case PR3_CORE_ERROR_OUT_OF_MEMORY: return PR3_E_NOMEM;
    case PR3_CORE_ERROR_NONE: break;
    }
    return PR3_OK;
}

static pr3_status out_of_memory(pr3_core_error *err) { return fail(err, PR3_CORE_ERROR_OUT_OF_MEMORY, 0, 0); }

void pr3_core_error_text(const pr3_core_error *e, pr3_buf *out) {
    switch (e->kind) {
    case PR3_CORE_ERROR_NONE: break;
    case PR3_CORE_ERROR_INPUT_TOO_LARGE:
        pr3_buf_put(out, PR3_S("input is "));
        pr3_buf_put_u64(out, e->a);
        pr3_buf_put(out, PR3_S(" bytes; the limit is "));
        pr3_buf_put_u64(out, e->b);
        break;
    case PR3_CORE_ERROR_EMPTY_INPUT: pr3_buf_put(out, PR3_S("input is empty")); break;
    case PR3_CORE_ERROR_UNKNOWN_REQUEST:
        pr3_buf_put(out, PR3_S("unknown or already answered request "));
        pr3_buf_put_u64(out, e->a);
        break;
    case PR3_CORE_ERROR_UNKNOWN_TOOL_CALL:
        pr3_buf_put(out, PR3_S("unknown or already answered tool call "));
        pr3_call_id_put(out, e->a);
        break;
    case PR3_CORE_ERROR_TURN_IN_PROGRESS:
        pr3_buf_put(out, PR3_S("still answering the previous message"));
        break;
    case PR3_CORE_ERROR_INVALID_TOOL_SPEC:
        pr3_buf_put(out, PR3_S("invalid tool: "));
        pr3_buf_put_bytes(out, e->why, e->why_len);
        break;
    case PR3_CORE_ERROR_OUT_OF_MEMORY: pr3_buf_put(out, PR3_S("out of memory")); break;
    }
}

/* ---- Sessions */

/* The position of id in the index, or where it would go. */
static uint32_t index_pos(const pr3_core *core, pr3_session_id id, bool *found) {
    uint32_t lo = 0, hi = core->n_index;
    while (lo < hi) {
        uint32_t mid = lo + (hi - lo) / 2;
        if (core->index[mid].id < id)
            lo = mid + 1;
        else
            hi = mid;
    }
    *found = lo < core->n_index && core->index[lo].id == id;
    return lo;
}

static core_session *session_find(const pr3_core *core, pr3_session_id id) {
    bool found;
    uint32_t at = index_pos(core, id, &found);
    return found ? &core->sessions[core->index[at].slot] : NULL;
}

/* Where a new session would go: a free record, else the least recently used session with
 * no turn, which is evicted on commit. false when every session is in a turn. */
static bool session_plan(const pr3_core *core, core_session **place) {
    core_session *victim = NULL;
    for (uint32_t i = 0; i < core->cfg.max_sessions; i++) {
        core_session *s = &core->sessions[i];
        if (!s->live) {
            *place = s;
            return true;
        }
        if (s->turn == CORE_NONE && (victim == NULL || s->last_use < victim->last_use)) victim = s;
    }
    *place = victim;
    return victim != NULL;
}

/* Empties the history and moves its start back to the first record. */
static void history_clear(core_session *s) {
    s->msgs -= s->head;
    s->meta -= s->head;
    s->head = 0;
    s->n = 0;
}

static void session_drop(pr3_core *core, core_session *s) {
    for (uint32_t i = 0; i < s->n; i++) pr3_buddy_free(&core->heap, s->meta[i].block);
    pr3_buddy_free(&core->heap, s->brief_block);
    bool found;
    uint32_t at = index_pos(core, s->id, &found);
    PR3_CHECK(found);
    memmove(core->index + at, core->index + at + 1, (core->n_index - at - 1) * sizeof *core->index);
    core->n_index--;
    s->live = false;
    history_clear(s);
    s->brief = EMPTY;
    s->brief_block = NULL;
}

/* Creates session id, which does not exist yet, in `s` (from session_plan). */
static core_session *session_open(pr3_core *core, pr3_session_id id, core_session *s) {
    if (s->live) session_drop(core, s);
    s->id = id;
    s->live = true;
    s->turn = CORE_NONE;
    history_clear(s);
    s->brief = EMPTY;
    s->brief_block = NULL;
    bool found;
    uint32_t at = index_pos(core, id, &found);
    memmove(core->index + at + 1, core->index + at, (core->n_index - at) * sizeof *core->index);
    core->index[at] = (core_index){id, (uint32_t)(s - core->sessions)};
    core->n_index++;
    return s;
}

static void touch(pr3_core *core, core_session *s) { s->last_use = core->uses++; }

/* ---- History */

/* The first user message after the first, or 0. */
static uint32_t next_question(const pr3_message *m, uint32_t n) {
    for (uint32_t i = 1; i < n; i++)
        if (m[i].role == PR3_ROLE_USER) return i;
    return 0;
}

static void drop_front(pr3_core *core, core_session *s, uint32_t k) {
    for (uint32_t i = 0; i < k; i++) pr3_buddy_free(&core->heap, s->meta[i].block);
    s->msgs += k;
    s->meta += k;
    s->head += k;
    s->n -= k;
}

/* Adds a message, first dropping whole exchanges from the front while the history is
 * full. An untrimmable history at its capacity (a restored run with no question after the
 * first message) loses its oldest message instead. */
static void history_push(pr3_core *core, core_session *s, const pr3_message *m, const core_meta *meta) {
    uint32_t max = core->cfg.max_history > 0 ? core->cfg.max_history : 1;
    while (s->n >= max) {
        uint32_t k = next_question(s->msgs, s->n);
        if (k == 0) break;
        drop_front(core, s, k);
    }
    if (s->n == core->hist_cap) drop_front(core, s, 1);
    if (s->head + s->n == 2 * core->hist_cap) { /* the end of the records: back to the first */
        memmove(s->msgs - s->head, s->msgs, s->n * sizeof *s->msgs);
        memmove(s->meta - s->head, s->meta, s->n * sizeof *s->meta);
        s->msgs -= s->head;
        s->meta -= s->head;
        s->head = 0;
    }
    s->msgs[s->n] = *m;
    s->meta[s->n] = *meta;
    s->n++;
}

/* ---- History-heap blocks */

static bool block_alloc(pr3_core *core, core_block *b, uint32_t n_calls, uint32_t n_sources, size_t text) {
    size_t calls = (size_t)n_calls * sizeof(pr3_tool_call), sources = (size_t)n_sources * sizeof(pr3_str);
    size_t total = calls + sources + text;
    *b = (core_block){0};
    if (total == 0) {
        b->text = pr3_buf_make(NULL, 0);
        return true;
    }
    void *mem = pr3_buddy_alloc(&core->heap, total);
    if (mem == NULL) return false;
    void *after_calls = (char *)mem + calls; /* a call's size keeps the labels aligned */
    b->mem = mem;
    b->calls = mem;
    b->sources = after_calls;
    b->text = pr3_buf_make((char *)after_calls + sources, text);
    return true;
}

/* Every byte the block's text was sized for was written. */
static void block_done(const core_block *b) { PR3_CHECK(!b->text.overflow && b->text.len == b->text.cap); }

/* Writes s, or its clipped form, and returns the view of what was written. */
static pr3_str put(pr3_buf *b, pr3_str s) {
    size_t at = b->len;
    pr3_buf_put(b, s);
    return (pr3_str){b->ptr != NULL ? b->ptr + at : "", b->len - at};
}

static pr3_str put_clip(pr3_buf *b, pr3_str s, size_t max) {
    size_t at = b->len;
    pr3_buf_put_clip(b, s, max);
    return (pr3_str){b->ptr != NULL ? b->ptr + at : "", b->len - at};
}

/* ---- Turns */

static pr3_str tcopy(core_turn *t, pr3_str s) {
    pr3_str v = put(&t->bytes, s);
    PR3_CHECK(!t->bytes.overflow); /* the turn arena is sized for the worst turn */
    return v;
}

static pr3_str tclip(core_turn *t, pr3_str s, size_t max) {
    pr3_str v = put_clip(&t->bytes, s, max);
    PR3_CHECK(!t->bytes.overflow);
    return v;
}

static pr3_trace_step *step_add(pr3_core *core, core_turn *t, pr3_step_type type, pr3_timestamp since,
                                pr3_timestamp at) {
    PR3_CHECK(t->trace.n_steps < core->steps_cap);
    pr3_trace_step *s = &t->steps[t->trace.n_steps++];
    *s = (pr3_trace_step){.type = type, .at = at, .duration_ms = at > since ? at - since : 0};
    return s;
}

static void act(pr3_core *core, pr3_action a) {
    PR3_CHECK(core->n_acts < core->actions_cap);
    core->acts[core->n_acts++] = a;
}

static uint32_t turn_awaiting(const pr3_core *core, pr3_request_id request) {
    for (uint32_t i = 0; i < core->cfg.max_turns; i++) {
        const core_turn *t = &core->turns[i];
        if (t->live && t->awaiting_model && t->request == request) return i;
    }
    return CORE_NONE;
}

static void gen_step(pr3_core *core, core_turn *t, pr3_timestamp at, const pr3_generation_info *info,
                     pr3_str content, const pr3_proposed_call *calls, uint32_t n_calls,
                     pr3_str *content_copy) {
    t->trace.total_tokens += (uint64_t)info->prompt_tokens + info->completion_tokens;
    t->trace.model = tclip(t, info->model, PR3_CORE_MODEL_NAME_BYTES);
    t->trace.engine = tclip(t, info->engine, PR3_CORE_MODEL_NAME_BYTES);
    pr3_trace_step *s = step_add(core, t, PR3_STEP_GENERATE, t->since, at);
    s->generate.model = t->trace.model;
    s->generate.content = tclip(t, content, core->cfg.max_input_bytes);
    PR3_CHECK(t->n_proposals + n_calls <= core->props_cap);
    pr3_proposed_call *rec = &t->proposals[t->n_proposals];
    for (uint32_t i = 0; i < n_calls; i++) {
        rec[i].name = tclip(t, calls[i].name, PR3_NAME_MAX_BYTES);
        rec[i].arguments = tclip(t, calls[i].arguments, core->cfg.max_tool_args_bytes);
    }
    t->n_proposals += n_calls;
    s->generate.calls = rec;
    s->generate.n_calls = n_calls;
    s->generate.prompt_tokens = info->prompt_tokens;
    s->generate.completion_tokens = info->completion_tokens;
    s->generate.error = EMPTY;
    if (content_copy != NULL) *content_copy = s->generate.content;
}

/* Ends the turn: a reply or a failure, then its trace. The slot is released at the start
 * of the next mutating call. */
static void finish(pr3_core *core, uint32_t ti, pr3_outcome outcome, pr3_str result, pr3_timestamp at) {
    core_turn *t = &core->turns[ti];
    core_session *s = &core->sessions[t->session];
    /* Every call still open is answered, so no later request shows a call without its
     * result. The text is a constant: no block, so ending a turn cannot run out. The
     * exchange's room already counts one result per call, and the steps one per call. */
    const pr3_str cancelled = PR3_S("error: " PR3_CALL_CANCELLED);
    for (uint32_t i = 0; i < t->n_pending; i++) {
        core_pending *p = &t->pending[i];
        if (!p->open) continue;
        p->open = false;
        pr3_message m = {.role = PR3_ROLE_TOOL, .content = cancelled, .at = at, .tool_call_id = p->call.id};
        core_meta meta = {0};
        history_push(core, s, &m, &meta);
        pr3_trace_step *step = step_add(core, t, PR3_STEP_TOOL_CALL, p->since, at);
        step->tool_call.tool = p->call.name;
        step->tool_call.arguments = p->call.arguments;
        step->tool_call.result = cancelled;
        step->tool_call.success = false;
    }
    if (outcome == PR3_OUTCOME_SUCCESS) {
        pr3_trace_step *r = step_add(core, t, PR3_STEP_RESPOND, at, at);
        r->respond.content = result;
        r->respond.turns = t->rounds + 1;
    }
    t->trace.outcome = outcome;
    t->trace.result = result;
    t->trace.ended_at = at;
    t->awaiting_model = false;
    t->n_open = 0;
    if (t->bytes.len > core->turn_high_water) core->turn_high_water = t->bytes.len;
    s->turn = CORE_NONE;
    touch(core, s);
    core->retired = ti;
    pr3_action a = {.kind = outcome == PR3_OUTCOME_SUCCESS ? PR3_ACTION_REPLY : PR3_ACTION_FAILED};
    if (outcome == PR3_OUTCOME_SUCCESS) {
        a.reply.session = s->id;
        a.reply.content = result;
    } else {
        a.failed.session = s->id;
        a.failed.reason = result;
    }
    act(core, a);
    act(core, (pr3_action){.kind = PR3_ACTION_TRACE, .trace = &t->trace});
}

/* The session's history as the model sees it: with the brief joined to its first user
 * message, and room for `extra` messages after it. A view of the history itself when
 * nothing changes; otherwise a copy in `a`. */
static pr3_message *prompt(const core_session *s, pr3_arena *a, uint32_t extra, const pr3_message **msgs) {
    if (s->brief.len == 0 && extra == 0) {
        *msgs = s->msgs;
        return NULL;
    }
    pr3_message *m = PR3_NEW_N(a, pr3_message, (size_t)s->n + extra);
    PR3_CHECK(m != NULL); /* the arena is sized for a full history */
    if (s->n) memcpy(m, s->msgs, s->n * sizeof *m);
    for (uint32_t i = 0; s->brief.len > 0 && i < s->n; i++) {
        if (m[i].role != PR3_ROLE_USER) continue;
        pr3_buf b;
        size_t joined = BRIEF_PREFIX.len + s->brief.len + 2 + m[i].content.len;
        PR3_CHECK(pr3_buf_from_arena(a, joined, &b) == PR3_OK);
        pr3_buf_put(&b, BRIEF_PREFIX);
        pr3_buf_put(&b, s->brief);
        pr3_buf_put(&b, PR3_S("\n\n"));
        pr3_buf_put(&b, m[i].content);
        m[i].content = (pr3_str){b.ptr, b.len};
        break;
    }
    *msgs = m;
    return m;
}

/* Asks for the turn's next model step. */
static void infer(pr3_core *core, uint32_t ti, pr3_timestamp at) {
    core_turn *t = &core->turns[ti];
    const core_session *s = &core->sessions[t->session];
    pr3_request_id id = core->next_request;
    if (core->next_request != UINT64_MAX) core->next_request++;
    t->awaiting_model = true;
    t->request = id;
    t->since = at;
    uint32_t extra = t->rounds > 0 ? 1 : 0, n = s->n;
    const pr3_message *msgs;
    pr3_message *copy = prompt(s, &core->step, extra, &msgs);
    const pr3_tool_spec *tools = core->tools;
    uint32_t n_tools = core->n_tools;
    if (extra) {
        pr3_buf b;
        PR3_CHECK(pr3_buf_from_arena(&core->step, CORE_BUDGET_MAX, &b) == PR3_OK);
        (void)pr3_tool_round_budget(t->rounds, core->cfg.max_tool_rounds, &b);
        PR3_CHECK(!b.overflow);
        copy[n++] = (pr3_message){.role = PR3_ROLE_SYSTEM, .content = {b.ptr, b.len}, .at = at};
        if (t->rounds >= core->cfg.max_tool_rounds) { /* the final answer cannot run another batch */
            tools = NULL;
            n_tools = 0;
        }
    }
    act(core, (pr3_action){.kind = PR3_ACTION_INFER,
                           .infer = {.id = id,
                                     .session = s->id,
                                     .messages = msgs,
                                     .n_messages = n,
                                     .tools = tools,
                                     .n_tools = n_tools}});
}

/* ---- Steps */

static void core_begin(pr3_core *core) {
    if (core->retired != CORE_NONE) {
        core_turn *t = &core->turns[core->retired];
        t->live = false;
        t->session = CORE_NONE;
        t->bytes.len = 0;
        t->bytes.overflow = false;
        core->retired = CORE_NONE;
    }
    pr3_arena_reset(&core->step);
    core->acts = NULL;
    core->n_acts = 0;
}

/* The host's notes that fit max_notes_bytes, in order, blank ones left out, up to the
 * first that does not fit: how many, where the fitting stopped, and the bytes of their
 * lines ("- " note "\n"). */
static void fit_notes(const pr3_core *core, const pr3_str *notes, uint32_t n, uint32_t *upto, uint32_t *count,
                      size_t *lines) {
    size_t room = core->cfg.max_notes_bytes;
    *count = 0;
    *lines = 0;
    uint32_t i = 0;
    for (; i < n; i++) {
        pr3_str t = core_trim(notes[i]);
        if (t.len == 0) continue;
        if (t.len > room) break;
        room -= t.len;
        (*count)++;
        *lines += 3 + core_one_line_len(t);
    }
    *upto = i;
}

static pr3_status on_utterance(pr3_core *core, const pr3_input *in, pr3_core_error *err) {
    pr3_session_id id = in->utterance.session;
    pr3_str content = in->utterance.content;
    if (content.len > core->cfg.max_input_bytes)
        return fail(err, PR3_CORE_ERROR_INPUT_TOO_LARGE, content.len, core->cfg.max_input_bytes);
    if (core_trim(content).len == 0) return fail(err, PR3_CORE_ERROR_EMPTY_INPUT, 0, 0);
    core_session *s = session_find(core, id);
    if (s != NULL && s->turn != CORE_NONE) return fail(err, PR3_CORE_ERROR_TURN_IN_PROGRESS, id, 0);
    uint32_t ti = CORE_NONE;
    core_session *place = NULL;
    for (uint32_t i = 0; i < core->cfg.max_turns && ti == CORE_NONE; i++)
        if (!core->turns[i].live) ti = i;
    if (ti == CORE_NONE || (s == NULL && !session_plan(core, &place))) return out_of_memory(err);

    uint32_t upto, count;
    size_t lines;
    fit_notes(core, in->utterance.notes, in->utterance.n_notes, &upto, &count, &lines);
    size_t len = count > 0 ? NOTES_PREFIX.len + lines + 1 + content.len : content.len;
    core_block b;
    if (!block_alloc(core, &b, 0, 0, len)) return out_of_memory(err);
    if (count > 0) {
        pr3_buf_put(&b.text, NOTES_PREFIX);
        for (uint32_t i = 0; i < upto; i++) {
            pr3_str note = core_trim(in->utterance.notes[i]);
            if (note.len == 0) continue;
            pr3_buf_put(&b.text, PR3_S("- "));
            core_put_one_line_clip(&b.text, note, SIZE_MAX); /* one line per note */
            pr3_buf_put_char(&b.text, '\n');
        }
        pr3_buf_put_char(&b.text, '\n');
    }
    pr3_buf_put(&b.text, content);
    block_done(&b);

    if (s == NULL) s = session_open(core, id, place);
    pr3_message m = {
        .role = PR3_ROLE_USER, .content = {b.text.ptr ? b.text.ptr : "", b.text.len}, .at = in->at};
    core_meta meta = {.block = b.mem};
    history_push(core, s, &m, &meta);
    touch(core, s);

    core_turn *t = &core->turns[ti];
    t->live = true;
    t->read_untrusted = false;
    t->awaiting_model = false;
    t->session = (uint32_t)(s - core->sessions);
    t->rounds = 0;
    t->request = 0;
    t->since = in->at;
    t->n_pending = t->n_open = 0;
    t->n_seen = t->calls = 0;
    t->n_proposals = 0;
    t->bytes.len = 0;
    t->bytes.overflow = false;
    t->trace = (pr3_trace){
        .session = id,
        .turn = core->next_turn,
        .model = EMPTY,
        .engine = EMPTY,
        .steps = t->steps,
        .result = EMPTY,
        .outcome = PR3_OUTCOME_FAILURE,
        .started_at = in->at,
        .ended_at = in->at,
    };
    if (core->next_turn != UINT64_MAX) core->next_turn++;
    t->trace.query = tcopy(t, content);
    s->turn = ti;
    if (count > 0) {
        pr3_trace_step *r = step_add(core, t, PR3_STEP_RETRIEVE, in->at, in->at);
        r->retrieve.query = t->trace.query;
        r->retrieve.num_results = count;
    }
    infer(core, ti, in->at);
    return PR3_OK;
}

static uint64_t guard_hash(pr3_str name, pr3_str args) {
    pr3_siphash h;
    pr3_siphash_init(&h, pr3_sipkey_from_bytes(guard_key), 1, 3);
    pr3_siphash_update(&h, name.ptr, name.len);
    pr3_siphash_update(&h, "", 1); /* a name never holds a NUL, so the split is unambiguous */
    pr3_siphash_update(&h, args.ptr, args.len);
    return pr3_siphash_final(&h);
}

/* Runs the message's valid calls through the loop guard, in order, as tentative entries
 * after the committed ones. false, with why, if the turn must stop. */
static bool guard_check(pr3_core *core, core_turn *t, const staged *st, uint32_t n, uint32_t *seen_out,
                        uint32_t *calls_out, pr3_buf *why) {
    uint32_t seen = t->n_seen, calls = t->calls, budget = core->cfg.max_tool_calls_per_turn;
    for (uint32_t i = 0; i < n; i++) {
        if (!st[i].ok) continue;
        pr3_str name = core->tools[st[i].tool].name, args = {st[i].args.ptr, st[i].args.len};
        uint64_t h = guard_hash(name, args);
        for (uint32_t j = 0; j < seen; j++) {
            const core_seen *e = &t->seen[j];
            if (e->hash == h && pr3_str_eq(e->name, name) && pr3_str_eq(e->arguments, args)) {
                pr3_buf_put(why, PR3_S("loop detected: identical call to '"));
                pr3_buf_put(why, name);
                pr3_buf_put(why, PR3_S("' with the same arguments"));
                return false;
            }
        }
        PR3_CHECK(seen < core->seen_cap);
        t->seen[seen++] = (core_seen){h, name, args};
        if (++calls > budget) {
            pr3_buf_put(why, PR3_S("tool budget exceeded: "));
            pr3_buf_put_u64(why, calls);
            pr3_buf_put(why, PR3_S(" calls (budget "));
            pr3_buf_put_u64(why, budget);
            pr3_buf_put_char(why, ')');
            return false;
        }
    }
    *seen_out = seen;
    *calls_out = calls;
    return true;
}

static pr3_status on_tool_calls(pr3_core *core, uint32_t ti, const pr3_input *in, pr3_core_error *err) {
    core_turn *t = &core->turns[ti];
    core_session *s = &core->sessions[t->session];
    const pr3_proposed_call *proposed = in->model_output.tool_calls;
    pr3_str content = in->model_output.content;
    uint32_t total = in->model_output.n_tool_calls, max_calls = core->cfg.max_tool_calls_per_round;
    uint32_t considered = total < max_calls ? total : max_calls, ignored = total - considered;
    const pr3_generation_info *info = &in->model_output.info;
    pr3_timestamp at = in->at;

    if (t->rounds >= core->cfg.max_tool_rounds) {
        gen_step(core, t, at, info, content, proposed, considered, NULL);
        t->rounds++;
        pr3_buf why;
        PR3_CHECK(pr3_buf_from_arena(&core->step, 64, &why) == PR3_OK);
        pr3_buf_put(&why, PR3_S("stopped: more than "));
        pr3_buf_put_u64(&why, core->cfg.max_tool_rounds);
        pr3_buf_put(&why, PR3_S(" rounds of tool calls"));
        finish(core, ti, PR3_OUTCOME_FAILURE, tcopy(t, (pr3_str){why.ptr, why.len}), at);
        return PR3_OK;
    }

    /* Validate every considered proposal before issuing any. A call beside a read in the
     * same message is in a turn that reads untrusted text too: a message's calls reach the
     * host at once. */
    bool read_untrusted = t->read_untrusted;
    for (uint32_t i = 0; i < considered && !read_untrusted; i++) {
        uint32_t k = core_tool_find(core, proposed[i].name);
        read_untrusted = k != CORE_NONE && core->tools[k].untrusted_output;
    }
    staged *st = PR3_NEW_N(&core->step, staged, considered > 0 ? considered : 1);
    PR3_CHECK(st != NULL);
    uint32_t n_valid = 0;
    for (uint32_t i = 0; i < considered; i++) {
        PR3_CHECK(pr3_buf_from_arena(&core->step, core->cfg.max_tool_args_bytes, &st[i].args) == PR3_OK);
        PR3_CHECK(pr3_buf_from_arena(&core->step, REASON_MAX, &st[i].reason) == PR3_OK);
        st[i].ok = core_tool_validate(core, &proposed[i], &st[i].args, &st[i].reason, &st[i].tool);
        if (st[i].ok && read_untrusted && core->tools[st[i].tool].requires_confirmation) {
            st[i].ok = false;
            st[i].reason.len = 0;
            pr3_buf_put(&st[i].reason, PR3_S(PR3_AFTER_UNTRUSTED));
        }
        n_valid += st[i].ok;
    }

    pr3_buf why;
    PR3_CHECK(pr3_buf_from_arena(&core->step, CORE_RESULT_MAX, &why) == PR3_OK);
    pr3_buf_put(&why, PR3_S("stopped: "));
    uint32_t seen = 0, calls = 0;
    if (!guard_check(core, t, st, considered, &seen, &calls, &why)) {
        gen_step(core, t, at, info, content, proposed, considered, NULL);
        t->rounds++;
        core->next_call =
            core->next_call > UINT64_MAX - considered ? UINT64_MAX : core->next_call + considered;
        finish(core, ti, PR3_OUTCOME_FAILURE, tcopy(t, (pr3_str){why.ptr, why.len}), at);
        return PR3_OK;
    }

    /* Stage the blocks: the assistant's message with every shown call, a result per
     * refused call and the note for calls over the limit. */
    size_t text = core_clip_len(content, core->cfg.max_input_bytes);
    for (uint32_t i = 0; i < considered; i++)
        text += st[i].ok ? proposed[i].name.len + st[i].args.len
                         : core_clip_len(proposed[i].name, PR3_NAME_MAX_BYTES) +
                               core_clip_len(proposed[i].arguments, PR3_CORE_REFUSED_ARGS_BYTES);
    core_block *blocks = PR3_NEW_N(&core->step, core_block, (size_t)considered + 2);
    PR3_CHECK(blocks != NULL);
    uint32_t n_blocks = 0;
    char note_mem[CORE_NOTE_MAX];
    pr3_buf note = pr3_buf_make(note_mem, sizeof note_mem);
    if (ignored > 0) {
        pr3_buf_put(&note, PR3_S("error: at most "));
        pr3_buf_put_u64(&note, max_calls);
        pr3_buf_put(&note, PR3_S(" tool calls per message; "));
        pr3_buf_put_u64(&note, ignored);
        pr3_buf_put(&note, PR3_S(" more were not run"));
        PR3_CHECK(!note.overflow);
    }
    bool ok = block_alloc(core, &blocks[n_blocks++], considered, 0, text);
    for (uint32_t i = 0; ok && i < considered; i++)
        if (!st[i].ok) ok = block_alloc(core, &blocks[n_blocks++], 0, 0, ERROR_PREFIX.len + st[i].reason.len);
    if (ok && ignored > 0) ok = block_alloc(core, &blocks[n_blocks++], 0, 0, note.len);
    if (!ok) {
        for (uint32_t i = 0; i + 1 < n_blocks; i++) pr3_buddy_free(&core->heap, blocks[i].mem);
        return out_of_memory(err);
    }

    /* Commit. */
    pr3_str shown_content;
    gen_step(core, t, at, info, content, proposed, considered, &shown_content);
    t->rounds++;
    uint64_t base = core->next_call;
    core->next_call = core->next_call > UINT64_MAX - considered ? UINT64_MAX : core->next_call + considered;

    core_block *ab = &blocks[0];
    pr3_str msg_content = put_clip(&ab->text, content, core->cfg.max_input_bytes);
    for (uint32_t i = 0; i < considered; i++) {
        pr3_tool_call *c = &ab->calls[i];
        c->id = base + i;
        if (st[i].ok) {
            c->name = put(&ab->text, proposed[i].name);
            c->arguments = put(&ab->text, (pr3_str){st[i].args.ptr, st[i].args.len});
        } else {
            c->name = put_clip(&ab->text, proposed[i].name, PR3_NAME_MAX_BYTES);
            c->arguments = put_clip(&ab->text, proposed[i].arguments, PR3_CORE_REFUSED_ARGS_BYTES);
        }
    }
    block_done(ab);
    pr3_message m = {.role = PR3_ROLE_ASSISTANT,
                     .content = msg_content,
                     .at = at,
                     .tool_calls = considered > 0 ? ab->calls : NULL,
                     .n_tool_calls = considered};
    core_meta meta = {.block = ab->mem};
    history_push(core, s, &m, &meta);

    uint32_t bi = 1;
    for (uint32_t i = 0; i < considered; i++) {
        if (st[i].ok) continue;
        core_block *rb = &blocks[bi++];
        pr3_str result = put(&rb->text, ERROR_PREFIX);
        result.len += put(&rb->text, (pr3_str){st[i].reason.ptr, st[i].reason.len}).len;
        block_done(rb);
        pr3_message r = {.role = PR3_ROLE_TOOL, .content = result, .at = at, .tool_call_id = base + i};
        core_meta rm = {.block = rb->mem};
        history_push(core, s, &r, &rm);
        pr3_trace_step *step = step_add(core, t, PR3_STEP_TOOL_CALL, at, at);
        step->tool_call.tool = tcopy(t, ab->calls[i].name);
        step->tool_call.arguments = tcopy(t, ab->calls[i].arguments);
        step->tool_call.result = tcopy(t, result);
        step->tool_call.success = false;
    }
    if (ignored > 0) {
        core_block *nb = &blocks[bi++];
        pr3_str text_note = put(&nb->text, (pr3_str){note_mem, note.len});
        block_done(nb);
        pr3_message r = {.role = PR3_ROLE_TOOL, .content = text_note, .at = at};
        core_meta rm = {.block = nb->mem};
        history_push(core, s, &r, &rm);
        pr3_trace_step *step = step_add(core, t, PR3_STEP_TOOL_CALL, at, at);
        step->tool_call.tool = PR3_S("(calls over the limit)");
        step->tool_call.arguments = EMPTY;
        step->tool_call.result = tcopy(t, text_note);
        step->tool_call.success = false;
    }
    (void)shown_content;

    if (n_valid == 0) { /* nothing runnable: the model sees why and may try again */
        infer(core, ti, at);
        return PR3_OK;
    }
    /* The guard's entries now point at the turn's own copies of the arguments. */
    uint32_t e = t->n_seen, k = 0;
    t->awaiting_model = false;
    for (uint32_t i = 0; i < considered; i++) {
        if (!st[i].ok) continue;
        const pr3_tool_spec *spec = &core->tools[st[i].tool];
        core_pending *p = &t->pending[k++];
        p->call = (pr3_tool_call){base + i, spec->name, tcopy(t, (pr3_str){st[i].args.ptr, st[i].args.len})};
        p->since = at;
        p->open = true;
        t->seen[e++].arguments = p->call.arguments;
        act(core,
            (pr3_action){.kind = spec->requires_confirmation ? PR3_ACTION_PROPOSE : PR3_ACTION_CALL_TOOL,
                         .call = p->call});
    }
    PR3_CHECK(e == seen);
    t->n_seen = seen;
    t->calls = calls;
    t->n_pending = t->n_open = k;
    return PR3_OK;
}

static pr3_status on_model_output(pr3_core *core, const pr3_input *in, pr3_core_error *err) {
    uint32_t ti = turn_awaiting(core, in->model_output.request);
    if (ti == CORE_NONE) return fail(err, PR3_CORE_ERROR_UNKNOWN_REQUEST, in->model_output.request, 0);
    if (in->model_output.n_tool_calls > 0) return on_tool_calls(core, ti, in, err);
    core_turn *t = &core->turns[ti];
    core_session *s = &core->sessions[t->session];
    pr3_str content = in->model_output.content;
    core_block b;
    if (!block_alloc(core, &b, 0, 0, core_clip_len(content, core->cfg.max_input_bytes)))
        return out_of_memory(err);
    pr3_str text = put_clip(&b.text, content, core->cfg.max_input_bytes);
    block_done(&b);
    pr3_str reply;
    gen_step(core, t, in->at, &in->model_output.info, content, NULL, 0, &reply);
    pr3_message m = {.role = PR3_ROLE_ASSISTANT, .content = text, .at = in->at};
    core_meta meta = {.block = b.mem};
    history_push(core, s, &m, &meta);
    finish(core, ti, PR3_OUTCOME_SUCCESS, reply, in->at);
    return PR3_OK;
}

static pr3_status on_model_failed(pr3_core *core, const pr3_input *in, pr3_core_error *err) {
    uint32_t ti = turn_awaiting(core, in->model_failed.request);
    if (ti == CORE_NONE) return fail(err, PR3_CORE_ERROR_UNKNOWN_REQUEST, in->model_failed.request, 0);
    core_turn *t = &core->turns[ti];
    pr3_trace_step *g = step_add(core, t, PR3_STEP_GENERATE, t->since, in->at);
    g->generate.model = t->trace.model;
    g->generate.content = EMPTY;
    g->generate.failed = true;
    g->generate.error = tclip(t, in->model_failed.reason, PR3_CORE_REASON_BYTES);
    finish(core, ti, PR3_OUTCOME_FAILURE, g->generate.error, in->at);
    return PR3_OK;
}

/* How many origin labels a result keeps, and their bytes. */
static uint32_t labels_of(const pr3_str *sources, uint32_t n, size_t *bytes) {
    uint32_t count = 0;
    *bytes = 0;
    for (uint32_t i = 0; i < n && i < PR3_CORE_SOURCES_PER_RESULT; i++) {
        pr3_str t = core_trim(sources[i]);
        if (t.len == 0) continue;
        count++;
        *bytes += core_clip_len(t, PR3_CORE_SOURCE_BYTES);
    }
    return count;
}

static uint32_t labels_put(const pr3_str *sources, uint32_t n, pr3_buf *b, pr3_str *out) {
    uint32_t count = 0;
    for (uint32_t i = 0; i < n && i < PR3_CORE_SOURCES_PER_RESULT; i++) {
        pr3_str t = core_trim(sources[i]);
        if (t.len > 0) out[count++] = put_clip(b, t, PR3_CORE_SOURCE_BYTES);
    }
    return count;
}

static pr3_status on_tool_outcome(pr3_core *core, const pr3_input *in, pr3_core_error *err) {
    uint64_t id = in->tool_outcome.call_id;
    uint32_t ti = CORE_NONE, k = 0;
    for (uint32_t i = 0; i < core->cfg.max_turns && ti == CORE_NONE; i++) {
        const core_turn *t = &core->turns[i];
        if (!t->live || t->awaiting_model) continue;
        for (uint32_t j = 0; j < t->n_pending; j++)
            if (t->pending[j].open && t->pending[j].call.id == id) {
                ti = i;
                k = j;
                break;
            }
    }
    if (ti == CORE_NONE) return fail(err, PR3_CORE_ERROR_UNKNOWN_TOOL_CALL, id, 0);
    core_turn *t = &core->turns[ti];
    core_session *s = &core->sessions[t->session];
    core_pending *p = &t->pending[k];
    uint32_t tool = core_tool_find(core, p->call.name);
    PR3_CHECK(tool != CORE_NONE);
    bool untrusted = core->tools[tool].untrusted_output, success = in->tool_outcome.success;
    pr3_str output = in->tool_outcome.output;
    size_t label_bytes = 0;
    uint32_t n_labels =
        untrusted ? labels_of(in->tool_outcome.sources, in->tool_outcome.n_sources, &label_bytes) : 0;
    size_t len = (success ? 0 : ERROR_PREFIX.len) + core_clip_len(output, core->cfg.max_tool_output_bytes);
    core_block b;
    if (!block_alloc(core, &b, 0, n_labels, len + label_bytes)) return out_of_memory(err);
    pr3_str content = success ? put(&b.text, EMPTY) : put(&b.text, ERROR_PREFIX);
    content.len += put_clip(&b.text, output, core->cfg.max_tool_output_bytes).len;
    if (untrusted) (void)labels_put(in->tool_outcome.sources, in->tool_outcome.n_sources, &b.text, b.sources);
    block_done(&b);

    p->open = false;
    t->n_open--;
    pr3_message m = {.role = PR3_ROLE_TOOL, .content = content, .at = in->at, .tool_call_id = id};
    core_meta meta = {.block = b.mem,
                      .sources = n_labels > 0 ? b.sources : NULL,
                      .n_sources = n_labels,
                      .untrusted = untrusted};
    history_push(core, s, &m, &meta);
    /* a failure may carry remote text too: success says nothing about trust */
    if (untrusted) t->read_untrusted = true;
    pr3_trace_step *step = step_add(core, t, PR3_STEP_TOOL_CALL, p->since, in->at);
    step->tool_call.tool = p->call.name;
    step->tool_call.arguments = p->call.arguments;
    step->tool_call.result = tcopy(t, content);
    step->tool_call.success = success;
    if (t->n_open == 0) infer(core, ti, in->at);
    return PR3_OK;
}

static pr3_status on_abort(pr3_core *core, const pr3_input *in) {
    core_session *s = session_find(core, in->abort_turn.session);
    if (s == NULL || s->turn == CORE_NONE) return PR3_OK;
    uint32_t ti = s->turn;
    core_turn *t = &core->turns[ti];
    finish(core, ti, PR3_OUTCOME_FAILURE, tclip(t, in->abort_turn.reason, PR3_CORE_REASON_BYTES), in->at);
    return PR3_OK;
}

pr3_status pr3_core_step(pr3_core *core, const pr3_input *in, pr3_actions *out, pr3_core_error *err) {
    core_begin(core);
    core_error_set(err, PR3_CORE_ERROR_NONE, 0, 0);
    core->acts = PR3_NEW_N(&core->step, pr3_action, core->actions_cap);
    PR3_CHECK(core->acts != NULL);
    pr3_status s = PR3_E_INVALID;
    switch (in->kind) {
    case PR3_INPUT_UTTERANCE: s = on_utterance(core, in, err); break;
    case PR3_INPUT_ABORT: s = on_abort(core, in); break;
    case PR3_INPUT_MODEL_OUTPUT: s = on_model_output(core, in, err); break;
    case PR3_INPUT_MODEL_FAILED: s = on_model_failed(core, in, err); break;
    case PR3_INPUT_TOOL_OUTCOME: s = on_tool_outcome(core, in, err); break;
    }
    out->items = core->acts;
    out->len = s == PR3_OK ? core->n_acts : 0;
    return s;
}

pr3_status pr3_core_register_tool(pr3_core *core, const pr3_tool_spec *spec, pr3_core_error *err) {
    core_begin(core);
    core_error_set(err, PR3_CORE_ERROR_NONE, 0, 0);
    return core_tool_register(core, spec, err);
}

/* ---- Session upkeep */

pr3_status pr3_core_forget(pr3_core *core, pr3_session_id session, pr3_core_error *err) {
    core_begin(core);
    core_error_set(err, PR3_CORE_ERROR_NONE, 0, 0);
    core_session *s = session_find(core, session);
    if (s == NULL) return PR3_OK;
    if (s->turn != CORE_NONE) return fail(err, PR3_CORE_ERROR_TURN_IN_PROGRESS, session, 0);
    session_drop(core, s);
    return PR3_OK;
}

pr3_status pr3_core_set_brief(pr3_core *core, pr3_session_id session, const pr3_str *brief,
                              pr3_core_error *err) {
    core_begin(core);
    core_error_set(err, PR3_CORE_ERROR_NONE, 0, 0);
    core_session *s = session_find(core, session);
    if (s != NULL && s->turn != CORE_NONE) return fail(err, PR3_CORE_ERROR_TURN_IN_PROGRESS, session, 0);
    pr3_buf fit = pr3_buf_make(NULL, 0);
    if (brief != NULL) {
        size_t room = (size_t)core->cfg.max_notes_bytes + CORE_MARKER_BYTES;
        PR3_CHECK(pr3_buf_from_arena(&core->step, room, &fit) == PR3_OK);
        core_put_one_line_clip(&fit, *brief, core->cfg.max_notes_bytes);
        PR3_CHECK(!fit.overflow);
    }
    if (fit.len == 0) {
        if (s != NULL) {
            pr3_buddy_free(&core->heap, s->brief_block);
            s->brief_block = NULL;
            s->brief = EMPTY;
            touch(core, s);
        }
        return PR3_OK;
    }
    core_session *place = NULL;
    if (s == NULL && !session_plan(core, &place)) return out_of_memory(err);
    char *mem = (char *)pr3_buddy_alloc(&core->heap, fit.len);
    if (mem == NULL) return out_of_memory(err);
    if (s == NULL) s = session_open(core, session, place);
    pr3_buddy_free(&core->heap, s->brief_block);
    memcpy(mem, fit.ptr, fit.len);
    s->brief_block = mem;
    s->brief = (pr3_str){mem, fit.len};
    touch(core, s);
    return PR3_OK;
}

static bool said(const pr3_message *m) {
    return (m->role == PR3_ROLE_USER || m->role == PR3_ROLE_ASSISTANT) && m->n_tool_calls == 0 &&
           m->tool_call_id == 0;
}

pr3_status pr3_core_restore(pr3_core *core, pr3_session_id session, const pr3_message *messages, uint32_t n,
                            pr3_core_error *err) {
    core_begin(core);
    core_error_set(err, PR3_CORE_ERROR_NONE, 0, 0);
    core_session *s = session_find(core, session);
    if (s != NULL && s->turn != CORE_NONE) return fail(err, PR3_CORE_ERROR_TURN_IN_PROGRESS, session, 0);
    if (s != NULL && s->n > 0) return PR3_OK;

    /* Which messages survive the history's trimming, pushed one by one. */
    uint32_t *keep = PR3_NEW_N(&core->step, uint32_t, (size_t)core->hist_cap + 1);
    PR3_CHECK(keep != NULL);
    uint32_t k = 0, max = core->cfg.max_history > 0 ? core->cfg.max_history : 1;
    for (uint32_t i = 0; i < n; i++) {
        if (!said(&messages[i])) continue;
        while (k >= max) {
            uint32_t q = 0;
            for (uint32_t j = 1; j < k && q == 0; j++)
                if (messages[keep[j]].role == PR3_ROLE_USER) q = j;
            if (q == 0) break;
            memmove(keep, keep + q, (k - q) * sizeof *keep);
            k -= q;
        }
        if (k == core->hist_cap) {
            memmove(keep, keep + 1, (k - 1) * sizeof *keep);
            k--;
        }
        keep[k++] = i;
    }
    if (k == 0) return PR3_OK;
    core_session *place = NULL;
    if (s == NULL && !session_plan(core, &place)) return out_of_memory(err);
    core_block *blocks = PR3_NEW_N(&core->step, core_block, k);
    PR3_CHECK(blocks != NULL);
    for (uint32_t j = 0; j < k; j++) {
        if (!block_alloc(core, &blocks[j], 0, 0, core_clip_len(messages[keep[j]].content, core->user_max))) {
            for (uint32_t i = 0; i < j; i++) pr3_buddy_free(&core->heap, blocks[i].mem);
            return out_of_memory(err);
        }
    }
    if (s == NULL) s = session_open(core, session, place);
    history_clear(s);
    for (uint32_t j = 0; j < k; j++) {
        const pr3_message *m = &messages[keep[j]];
        pr3_str text = put_clip(&blocks[j].text, m->content, core->user_max);
        block_done(&blocks[j]);
        s->msgs[j] = (pr3_message){.role = m->role, .content = text, .at = m->at};
        s->meta[j] = (core_meta){.block = blocks[j].mem};
    }
    s->n = k;
    touch(core, s);
    return PR3_OK;
}

/* ---- Queries */

void pr3_core_history(const pr3_core *core, pr3_session_id session, const pr3_message **messages,
                      uint32_t *n) {
    const core_session *s = session_find(core, session);
    *messages = s != NULL ? s->msgs : NULL;
    *n = s != NULL ? s->n : 0;
}

bool pr3_core_in_turn(const pr3_core *core, pr3_session_id session) {
    const core_session *s = session_find(core, session);
    return s != NULL && s->turn != CORE_NONE;
}

/* Whether call id's result is in the history and carried untrusted text. */
static bool result_untrusted(const core_session *s, uint64_t id) {
    for (uint32_t i = 0; i < s->n; i++)
        if (s->msgs[i].role == PR3_ROLE_TOOL && s->msgs[i].tool_call_id == id) return s->meta[i].untrusted;
    return false;
}

void pr3_core_untrusted_calls(pr3_core *core, pr3_session_id session, const pr3_tool_call **calls,
                              uint32_t *n) {
    pr3_arena_reset(&core->q_calls);
    *calls = NULL;
    *n = 0;
    const core_session *s = session_find(core, session);
    if (s == NULL) return;
    pr3_tool_call *list = PR3_NEW_N(&core->q_calls, pr3_tool_call,
                                    (size_t)core->hist_cap * max_of(core->cfg.max_tool_calls_per_round, 1));
    PR3_CHECK(list != NULL);
    uint32_t k = 0;
    for (uint32_t i = 0; i < s->n; i++)
        for (uint32_t j = 0; j < s->msgs[i].n_tool_calls; j++)
            if (result_untrusted(s, s->msgs[i].tool_calls[j].id)) list[k++] = s->msgs[i].tool_calls[j];
    *calls = list;
    *n = k;
}

void pr3_core_untrusted_sources(pr3_core *core, pr3_session_id session, const pr3_str **sources,
                                uint32_t *n) {
    pr3_arena_reset(&core->q_sources);
    *sources = NULL;
    *n = 0;
    const core_session *s = session_find(core, session);
    if (s == NULL) return;
    pr3_str *list =
        PR3_NEW_N(&core->q_sources, pr3_str, (size_t)core->hist_cap * PR3_CORE_SOURCES_PER_RESULT);
    PR3_CHECK(list != NULL);
    uint32_t k = 0;
    for (uint32_t i = 0; i < s->n; i++) {
        if (!s->meta[i].untrusted) continue;
        for (uint32_t j = 0; j < s->meta[i].n_sources; j++) {
            pr3_str src = s->meta[i].sources[j];
            bool dup = false;
            for (uint32_t x = 0; x < k && !dup; x++) dup = pr3_str_eq(list[x], src);
            if (!dup) list[k++] = src;
        }
    }
    *sources = list;
    *n = k;
}

bool pr3_core_tool_sources(pr3_core *core, pr3_str tool, const pr3_str *sources, uint32_t n_sources,
                           const pr3_str **out, uint32_t *n_out) {
    pr3_arena_reset(&core->q_labels);
    *out = NULL;
    *n_out = 0;
    uint32_t idx = core_tool_find(core, tool);
    if (idx == CORE_NONE || !core->tools[idx].untrusted_output) return false;
    pr3_str *list = PR3_NEW_N(&core->q_labels, pr3_str, PR3_CORE_SOURCES_PER_RESULT);
    pr3_buf b;
    size_t room = (size_t)PR3_CORE_SOURCES_PER_RESULT * LABEL_MAX;
    PR3_CHECK(list != NULL && pr3_buf_from_arena(&core->q_labels, room, &b) == PR3_OK);
    *n_out = labels_put(sources, n_sources, &b, list);
    *out = list;
    return true;
}

bool pr3_core_upcoming(pr3_core *core, pr3_session_id session, const pr3_message **messages, uint32_t *n,
                       const pr3_tool_spec **tools, uint32_t *n_tools) {
    pr3_arena_reset(&core->q_upcoming);
    const core_session *s = session_find(core, session);
    if (s == NULL || s->turn != CORE_NONE || s->n == 0) return false;
    (void)prompt(s, &core->q_upcoming, 0, messages);
    *n = s->n;
    *tools = core->tools;
    *n_tools = core->n_tools;
    return true;
}

bool pr3_core_opening(pr3_core *core, pr3_str brief, pr3_str *text, const pr3_tool_spec **tools,
                      uint32_t *n_tools) {
    pr3_arena_reset(&core->q_opening);
    if (core_one_line_len(brief) == 0) return false;
    pr3_buf b;
    size_t room = BRIEF_PREFIX.len + core->cfg.max_notes_bytes + CORE_MARKER_BYTES + 2;
    PR3_CHECK(pr3_buf_from_arena(&core->q_opening, room, &b) == PR3_OK);
    pr3_buf_put(&b, BRIEF_PREFIX);
    core_put_one_line_clip(&b, brief, core->cfg.max_notes_bytes);
    pr3_buf_put(&b, PR3_S("\n\n"));
    PR3_CHECK(!b.overflow);
    *text = (pr3_str){b.ptr, b.len};
    *tools = core->tools;
    *n_tools = core->n_tools;
    return true;
}
