/* Traces as canonical JSON (doc/04, "Traces"), call ids as text and the execution-budget
 * message. A trace is kept as structs that point at bytes the turn already copied; the
 * JSON is written only when the host exports it, with every object's keys in byte order
 * so the output is canonical without sorting. It is written through a copy of the
 * builder whose address never leaves this file, with every helper inline, so the compiler
 * keeps the copy in registers rather than reloading it after each byte it stores. */
#include "core_internal.h"

#include <pr3/poison.h>

#define KEY(b, lit, first)                                                                                   \
    ((first) ? pr3_buf_put_bytes((b), "\"" lit "\":", sizeof("\"" lit "\":") - 1)                            \
             : pr3_buf_put_bytes((b), ",\"" lit "\":", sizeof(",\"" lit "\":") - 1))

PR3_INLINE void put_calls(pr3_buf *b, const pr3_proposed_call *calls, uint32_t n) {
    pr3_buf_put_char(b, '[');
    for (uint32_t i = 0; i < n; i++) {
        if (i) pr3_buf_put_char(b, ',');
        pr3_buf_put_char(b, '{');
        KEY(b, "arguments", true);
        pr3_json_put_string(b, calls[i].arguments);
        KEY(b, "name", false);
        pr3_json_put_string(b, calls[i].name);
        pr3_buf_put_char(b, '}');
    }
    pr3_buf_put_char(b, ']');
}

PR3_INLINE void put_step(pr3_buf *b, const pr3_trace_step *s) {
    pr3_buf_put_char(b, '{');
    KEY(b, "at", true);
    pr3_buf_put_u64(b, s->at);
    KEY(b, "duration_ms", false);
    pr3_buf_put_u64(b, s->duration_ms);
    KEY(b, "input", false);
    switch (s->type) {
    case PR3_STEP_RETRIEVE:
        pr3_buf_put_char(b, '{');
        KEY(b, "query", true);
        pr3_json_put_string(b, s->retrieve.query);
        pr3_buf_put_char(b, '}');
        KEY(b, "output", false);
        pr3_buf_put_char(b, '{');
        KEY(b, "num_results", true);
        pr3_buf_put_u64(b, s->retrieve.num_results);
        pr3_buf_put_char(b, '}');
        KEY(b, "step_type", false);
        pr3_json_put_string(b, PR3_S("retrieve"));
        break;
    case PR3_STEP_GENERATE: {
        pr3_buf_put_char(b, '{');
        KEY(b, "model", true);
        pr3_json_put_string(b, s->generate.model);
        pr3_buf_put_char(b, '}');
        KEY(b, "output", false);
        pr3_buf_put_char(b, '{');
        if (s->generate.failed) {
            KEY(b, "content", true);
            pr3_json_put_string(b, PR3_S(""));
            KEY(b, "error", false);
            pr3_json_put_string(b, s->generate.error);
            KEY(b, "finish_reason", false);
            pr3_json_put_string(b, PR3_S("error"));
            KEY(b, "tokens", false);
            pr3_buf_put_u64(b, 0);
        } else {
            uint64_t total = (uint64_t)s->generate.prompt_tokens + s->generate.completion_tokens;
            KEY(b, "completion_tokens", true);
            pr3_buf_put_u64(b, s->generate.completion_tokens);
            KEY(b, "content", false);
            pr3_json_put_string(b, s->generate.content);
            KEY(b, "finish_reason", false);
            pr3_json_put_string(b, s->generate.n_calls > 0 ? PR3_S("tool_calls") : PR3_S("stop"));
            KEY(b, "prompt_tokens", false);
            pr3_buf_put_u64(b, s->generate.prompt_tokens);
            KEY(b, "tokens", false);
            pr3_buf_put_u64(b, total);
            KEY(b, "tool_calls", false);
            put_calls(b, s->generate.calls, s->generate.n_calls);
            KEY(b, "total_tokens", false);
            pr3_buf_put_u64(b, total);
        }
        pr3_buf_put_char(b, '}');
        KEY(b, "step_type", false);
        pr3_json_put_string(b, PR3_S("generate"));
        break;
    }
    case PR3_STEP_TOOL_CALL:
        pr3_buf_put_char(b, '{');
        KEY(b, "arguments", true);
        pr3_json_put_string(b, s->tool_call.arguments);
        KEY(b, "tool", false);
        pr3_json_put_string(b, s->tool_call.tool);
        pr3_buf_put_char(b, '}');
        KEY(b, "output", false);
        pr3_buf_put_char(b, '{');
        KEY(b, "result", true);
        pr3_json_put_string(b, s->tool_call.result);
        KEY(b, "success", false);
        pr3_buf_put(b, s->tool_call.success ? PR3_S("true") : PR3_S("false"));
        pr3_buf_put_char(b, '}');
        KEY(b, "step_type", false);
        pr3_json_put_string(b, PR3_S("tool_call"));
        break;
    case PR3_STEP_RESPOND:
        pr3_buf_put_bytes(b, "{}", 2);
        KEY(b, "output", false);
        pr3_buf_put_char(b, '{');
        KEY(b, "content", true);
        pr3_json_put_string(b, s->respond.content);
        KEY(b, "turns", false);
        pr3_buf_put_u64(b, s->respond.turns);
        pr3_buf_put_char(b, '}');
        KEY(b, "step_type", false);
        pr3_json_put_string(b, PR3_S("respond"));
        break;
    }
    pr3_buf_put_char(b, '}');
}

pr3_status pr3_trace_write_json(const pr3_trace *t, pr3_buf *dst) {
    pr3_buf copy = {dst->ptr, dst->len, dst->cap, dst->overflow}, *out = &copy;
    pr3_buf_put_char(out, '{');
    KEY(out, "agent", true);
    pr3_json_put_string(out, PR3_S("pr3"));
    KEY(out, "ended_at", false);
    pr3_buf_put_u64(out, t->ended_at);
    KEY(out, "engine", false);
    pr3_json_put_string(out, t->engine);
    KEY(out, "model", false);
    pr3_json_put_string(out, t->model);
    KEY(out, "outcome", false);
    pr3_json_put_string(out, t->outcome == PR3_OUTCOME_SUCCESS ? PR3_S("success") : PR3_S("failure"));
    KEY(out, "query", false);
    pr3_json_put_string(out, t->query);
    KEY(out, "result", false);
    pr3_json_put_string(out, t->result);
    KEY(out, "started_at", false);
    pr3_buf_put_u64(out, t->started_at);
    KEY(out, "steps", false);
    pr3_buf_put_char(out, '[');
    for (uint32_t i = 0; i < t->n_steps; i++) {
        if (i) pr3_buf_put_char(out, ',');
        put_step(out, &t->steps[i]);
    }
    pr3_buf_put_char(out, ']');
    KEY(out, "total_tokens", false);
    pr3_buf_put_u64(out, t->total_tokens);
    KEY(out, "trace_id", false);
    pr3_buf_put_bytes(out, "\"pr3-", 5);
    pr3_buf_put_u64(out, t->session);
    pr3_buf_put_char(out, '-');
    pr3_buf_put_u64(out, t->turn);
    pr3_buf_put_bytes(out, "\"}", 2);
    dst->len = copy.len;
    dst->overflow = copy.overflow;
    return copy.overflow ? PR3_E_LIMIT : PR3_OK;
}

pr3_status pr3_call_id_parse(pr3_str text, uint64_t *id) {
    pr3_str prefix = PR3_S("call-");
    if (text.len <= prefix.len || memcmp(text.ptr, prefix.ptr, prefix.len) != 0) return PR3_E_INVALID;
    uint64_t v;
    if (pr3_parse_u64((pr3_str){text.ptr + prefix.len, text.len - prefix.len}, &v) != PR3_OK || v == 0)
        return PR3_E_INVALID;
    *id = v;
    return PR3_OK;
}

void pr3_call_id_put(pr3_buf *b, uint64_t id) {
    pr3_buf_put(b, PR3_S("call-"));
    pr3_buf_put_u64(b, id);
}

bool pr3_tool_round_budget(uint32_t used, uint32_t limit, pr3_buf *out) {
    if (used == 0) return false;
    if (used < limit) {
        pr3_buf_put(out, PR3_S("Execution budget: "));
        pr3_buf_put_u64(out, limit - used);
        pr3_buf_put(out, PR3_S(" tool-call rounds remain for this user request. Batch independent reads when "
                               "useful. Reserve a round for any required answer-validation tool. This status "
                               "does not grant permission for actions."));
    } else {
        pr3_buf_put(out,
                    PR3_S("Execution budget: 0 tool-call rounds remain for this user request. Do not request "
                          "more tools. Answer from the recorded results. Clearly state any requested facts "
                          "or actions that remain unresolved; do not invent them or claim an action "
                          "succeeded without its successful result."));
    }
    return true;
}
