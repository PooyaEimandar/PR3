/* The tool registry and the schema subset (doc/04, "Tool registry and the schema
 * subset"). A schema is checked once, at registration, and kept in a compact form:
 * properties sorted by name with their type and, for enums, their canonical values. A
 * call's arguments are parsed into a tape in the step arena, checked in one pass and
 * written out canonically from the same tape. Everything here treats the model's output
 * as untrusted input. */
#include "core_internal.h"

#include <pr3/poison.h>

#define SHOWN PR3_NAME_MAX_BYTES /* how much of a name or value a refusal shows */

/* ---- Error details */

void core_error_set(pr3_core_error *err, pr3_core_error_kind kind, uint64_t a, uint64_t b) {
    if (err == NULL) return;
    err->kind = kind;
    err->a = a;
    err->b = b;
    err->why_len = 0;
}

void core_error_why(pr3_core_error *err, pr3_str s) {
    if (err == NULL) return;
    size_t room = PR3_CORE_WHY_BYTES - err->why_len;
    pr3_str part = pr3_str_clip(s, room);
    memcpy(err->why + err->why_len, part.ptr, part.len);
    err->why_len += (uint32_t)part.len;
}

/* s as a refusal shows it: at most SHOWN bytes, with the marker if cut. */
static void why_shown(pr3_core_error *err, pr3_str s) {
    core_error_why(err, pr3_str_clip(s, SHOWN));
    if (s.len > SHOWN) core_error_why(err, CORE_MARKER);
}

static pr3_status invalid(pr3_core_error *err, pr3_str tool, pr3_str prop, pr3_str why1, pr3_str shown,
                          pr3_str why2) {
    core_error_set(err, PR3_CORE_ERROR_INVALID_TOOL_SPEC, 0, 0);
    why_shown(err, tool);
    core_error_why(err, PR3_S(": "));
    if (prop.ptr != NULL) {
        core_error_why(err, PR3_S("property '"));
        why_shown(err, prop);
        core_error_why(err, PR3_S("': "));
    }
    core_error_why(err, why1);
    if (shown.ptr != NULL) why_shown(err, shown);
    core_error_why(err, why2);
    return PR3_E_INVALID;
}

#define NO_STR ((pr3_str){NULL, 0})

/* ---- Lookups */

uint32_t core_tool_find(const pr3_core *core, pr3_str name) {
    uint32_t lo = 0, hi = core->n_tools;
    while (lo < hi) {
        uint32_t mid = lo + (hi - lo) / 2;
        int c = pr3_str_cmp(core->tools[mid].name, name);
        if (c == 0) return mid;
        if (c < 0)
            lo = mid + 1;
        else
            hi = mid;
    }
    return CORE_NONE;
}

static uint32_t prop_find(const core_schema *s, pr3_str name) {
    uint32_t lo = 0, hi = s->n_props;
    while (lo < hi) {
        uint32_t mid = lo + (hi - lo) / 2;
        int c = pr3_str_cmp(s->props[mid].name, name);
        if (c == 0) return mid;
        if (c < 0)
            lo = mid + 1;
        else
            hi = mid;
    }
    return CORE_NONE;
}

static pr3_str kind_name(pr3_json_type t) {
    switch (t) {
    case PR3_JSON_TYPE_STRING: return PR3_S("a string");
    case PR3_JSON_TYPE_NUMBER: return PR3_S("a number");
    case PR3_JSON_TYPE_INTEGER: return PR3_S("an integer");
    case PR3_JSON_TYPE_BOOLEAN: return PR3_S("true or false");
    case PR3_JSON_TYPE_OBJECT: return PR3_S("an object");
    case PR3_JSON_TYPE_ARRAY: return PR3_S("a list");
    }
    return PR3_S("a value");
}

/* The longest canonical form of a value in a text of n bytes: only a number grows, at
 * most 4.5 times ("1e15" is written 1000000000000000.0), and a list joined by ", " adds a
 * byte per value. */
static size_t canon_cap(size_t n) { return n * 6 + 64; }

/* The scratch a text of n bytes needs: its tape, the sorted keys of the objects open at
 * once, the writer's frames and keys, and a canonical value. 0 on overflow. */
size_t core_tool_scratch_bytes(size_t n) {
    if (n > UINT32_MAX / 8) return 0;
    uint32_t values = (uint32_t)(n / 2 + 1);
    pr3_json_tape worst = {NULL, values, values, values < PR3_JSON_MAX_DEPTH ? values : PR3_JSON_MAX_DEPTH};
    size_t parse = pr3_json_parse_bytes(n), write = pr3_json_write_bytes(&worst), keys, total;
    if (parse == 0 || write == 0 || pr3_ckd_mul(&keys, (size_t)values + 2, 8) ||
        pr3_ckd_add(&total, parse, write) || pr3_ckd_add(&total, total, keys) ||
        pr3_ckd_add(&total, total, canon_cap(n)) || pr3_ckd_add(&total, total, (size_t)12 * CORE_SLACK))
        return 0;
    return total;
}

/* ---- Registration */

static bool copy_str(pr3_arena *a, pr3_str s, pr3_str *out) {
    char *p = (char *)pr3_arena_alloc(a, s.len, 1);
    if (p == NULL) return false;
    if (s.len) memcpy(p, s.ptr, s.len);
    *out = (pr3_str){p, s.len};
    return true;
}

static bool is_string(const pr3_json_tape *t, uint32_t i, pr3_str want) {
    return i != PR3_JSON_NONE && pr3_json_kind_of(t, i) == PR3_JSON_STRING &&
           pr3_str_eq(pr3_json_str(t, i), want);
}

/* The first key of object obj (in byte order) not in allowed, or an empty view. */
static bool unsupported_key(const pr3_json_tape *t, const uint32_t *keys, uint32_t n, const pr3_str *allowed,
                            size_t n_allowed, pr3_str *out) {
    for (uint32_t k = 0; k < n; k++) {
        pr3_str key = pr3_json_str(t, keys[k]);
        bool ok = false;
        for (size_t j = 0; j < n_allowed && !ok; j++) ok = pr3_str_eq(key, allowed[j]);
        if (!ok) {
            *out = key;
            return true;
        }
    }
    return false;
}

/* Appends the value at i, canonical, to b (in the step arena, canon_cap bytes). */
static void put_value(pr3_core *core, const pr3_json_tape *t, uint32_t i, unsigned flags, pr3_buf *b) {
    pr3_status s = pr3_json_write(t, i, &core->step, b, flags);
    PR3_CHECK(s == PR3_OK); /* the step arena and b are sized for any schema */
}

static pr3_status parse_prop(pr3_core *core, const pr3_tool_spec *spec, const pr3_json_tape *t, uint32_t key,
                             core_prop *out, pr3_core_error *err) {
    static const pr3_str allowed[] = {PR3_S("type"), PR3_S("description"), PR3_S("enum")};
    pr3_str name = pr3_json_str(t, key);
    uint32_t v = key + 1;
    if (pr3_json_kind_of(t, v) != PR3_JSON_OBJECT)
        return invalid(err, spec->name, name, PR3_S("must be an object schema"), NO_STR, NO_STR);
    const uint32_t *keys;
    uint32_t n;
    PR3_CHECK(pr3_json_members(t, v, &core->step, &keys, &n) == PR3_OK);
    pr3_str other;
    if (unsupported_key(t, keys, n, allowed, PR3_ARRAY_LEN(allowed), &other))
        return invalid(err, spec->name, name, PR3_S("'"), other, PR3_S("' is not supported"));
    uint32_t ty = pr3_json_member(t, v, PR3_S("type"));
    pr3_json_type type;
    if (ty == PR3_JSON_NONE || pr3_json_kind_of(t, ty) != PR3_JSON_STRING ||
        pr3_json_type_parse(pr3_json_str(t, ty), &type) != PR3_OK)
        return invalid(err, spec->name, name,
                       PR3_S("needs one type: string, number, integer, boolean, object or array"), NO_STR,
                       NO_STR);
    *out = (core_prop){.type = type};
    if (!copy_str(&core->statics, name, &out->name)) return PR3_E_NOMEM;
    uint32_t e = pr3_json_member(t, v, PR3_S("enum"));
    if (e == PR3_JSON_NONE) return PR3_OK;
    if (pr3_json_kind_of(t, e) != PR3_JSON_ARRAY || t->tokens[e].n == 0)
        return invalid(err, spec->name, name, PR3_S("enum must be a non-empty list"), NO_STR, NO_STR);
    uint32_t count = t->tokens[e].n;
    pr3_buf b;
    PR3_CHECK(pr3_buf_from_arena(&core->step, canon_cap(spec->parameters.len), &b) == PR3_OK);
    for (uint32_t i = e + 1; i < t->tokens[e].v.end; i = pr3_json_next(t, i)) {
        if (!pr3_json_is_type(t, i, type)) {
            put_value(core, t, i, 0, &b);
            (void)invalid(err, spec->name, name, PR3_S("enum value "), (pr3_str){b.ptr, b.len},
                          PR3_S(" is not "));
            core_error_why(err, kind_name(type));
            return PR3_E_INVALID;
        }
    }
    pr3_str *enum_keys = PR3_NEW_N(&core->statics, pr3_str, count);
    if (enum_keys == NULL) return PR3_E_NOMEM;
    uint32_t k = 0;
    for (uint32_t i = e + 1; i < t->tokens[e].v.end; i = pr3_json_next(t, i)) {
        b.len = 0;
        put_value(core, t, i, PR3_JSON_ZERO_UNSIGNED, &b);
        if (!copy_str(&core->statics, (pr3_str){b.ptr, b.len}, &enum_keys[k++])) return PR3_E_NOMEM;
    }
    /* the list as refusals show it: each value canonical, joined by ", " */
    b.len = 0;
    for (uint32_t i = e + 1; i < t->tokens[e].v.end; i = pr3_json_next(t, i)) {
        if (i != e + 1) pr3_buf_put(&b, PR3_S(", "));
        put_value(core, t, i, 0, &b);
    }
    out->n_enum = count;
    out->enum_keys = enum_keys;
    return copy_str(&core->statics, (pr3_str){b.ptr, b.len}, &out->enum_list) ? PR3_OK : PR3_E_NOMEM;
}

static pr3_status parse_schema(pr3_core *core, const pr3_tool_spec *spec, core_schema *out,
                               pr3_core_error *err) {
    static const pr3_str top_allowed[] = {PR3_S("type"), PR3_S("properties"), PR3_S("required"),
                                          PR3_S("additionalProperties"), PR3_S("description")};
    if (spec->parameters.len > (size_t)PR3_TOOL_PARAMETERS_MAX_BYTES) {
        char tmp[24];
        pr3_buf n = pr3_buf_make(tmp, sizeof tmp);
        pr3_buf_put_u64(&n, (uint64_t)PR3_TOOL_PARAMETERS_MAX_BYTES);
        (void)invalid(err, spec->name, NO_STR, PR3_S("parameters is larger than "), NO_STR, NO_STR);
        core_error_why(err, (pr3_str){tmp, n.len});
        core_error_why(err, PR3_S(" bytes"));
        return PR3_E_INVALID;
    }
    pr3_json_tape t;
    if (pr3_json_parse(&core->step, pr3_str_bytes(spec->parameters), &t, NULL) != PR3_OK)
        return invalid(err, spec->name, NO_STR, PR3_S("parameters is not JSON"), NO_STR, NO_STR);
    if (pr3_json_kind_of(&t, 0) != PR3_JSON_OBJECT ||
        !is_string(&t, pr3_json_member(&t, 0, PR3_S("type")), PR3_S("object")))
        return invalid(err, spec->name, NO_STR, PR3_S("parameters must be an object schema"), NO_STR, NO_STR);
    const uint32_t *keys;
    uint32_t n;
    PR3_CHECK(pr3_json_members(&t, 0, &core->step, &keys, &n) == PR3_OK);
    pr3_str other;
    if (unsupported_key(&t, keys, n, top_allowed, PR3_ARRAY_LEN(top_allowed), &other))
        return invalid(err, spec->name, NO_STR, PR3_S("'"), other, PR3_S("' is not supported"));

    *out = (core_schema){0};
    uint32_t props = pr3_json_member(&t, 0, PR3_S("properties"));
    if (props != PR3_JSON_NONE) {
        if (pr3_json_kind_of(&t, props) != PR3_JSON_OBJECT)
            return invalid(err, spec->name, NO_STR, PR3_S("properties must be an object"), NO_STR, NO_STR);
        const uint32_t *pkeys;
        uint32_t pn;
        PR3_CHECK(pr3_json_members(&t, props, &core->step, &pkeys, &pn) == PR3_OK);
        core_prop *list = PR3_NEW_N(&core->statics, core_prop, pn > 0 ? pn : 1);
        if (list == NULL) return PR3_E_NOMEM;
        for (uint32_t k = 0; k < pn; k++) {
            pr3_arena_mark m = pr3_arena_mark_get(&core->step);
            pr3_status s = parse_prop(core, spec, &t, pkeys[k], &list[k], err);
            pr3_arena_rewind(&core->step, m);
            if (s != PR3_OK) return s;
        }
        out->props = list;
        out->n_props = pn;
    }
    uint32_t req = pr3_json_member(&t, 0, PR3_S("required"));
    if (req != PR3_JSON_NONE) {
        if (pr3_json_kind_of(&t, req) != PR3_JSON_ARRAY)
            return invalid(err, spec->name, NO_STR, PR3_S("required must be a list"), NO_STR, NO_STR);
        for (uint32_t i = req + 1; i < t.tokens[req].v.end; i = pr3_json_next(&t, i))
            if (pr3_json_kind_of(&t, i) != PR3_JSON_STRING)
                return invalid(err, spec->name, NO_STR, PR3_S("required must list property names"), NO_STR,
                               NO_STR);
        uint32_t count = t.tokens[req].n;
        uint32_t *idx = PR3_NEW_N(&core->statics, uint32_t, count > 0 ? count : 1);
        if (idx == NULL) return PR3_E_NOMEM;
        uint32_t k = 0;
        for (uint32_t i = req + 1; i < t.tokens[req].v.end; i = pr3_json_next(&t, i)) {
            pr3_str want = pr3_json_str(&t, i);
            idx[k] = prop_find(out, want);
            if (idx[k] == CORE_NONE)
                return invalid(err, spec->name, NO_STR, PR3_S("required '"), want,
                               PR3_S("' is not a property"));
            k++;
        }
        out->required = idx;
        out->n_required = count;
    }
    uint32_t add = pr3_json_member(&t, 0, PR3_S("additionalProperties"));
    out->additional = add != PR3_JSON_NONE && pr3_json_kind_of(&t, add) == PR3_JSON_TRUE;
    return PR3_OK;
}

pr3_status core_tool_register(pr3_core *core, const pr3_tool_spec *spec, pr3_core_error *err) {
    if (!pr3_json_name_valid(spec->name))
        return invalid(err, spec->name, NO_STR, PR3_S("names use lowercase letters, digits and '_'"), NO_STR,
                       NO_STR);
    if (core_tool_find(core, spec->name) != CORE_NONE)
        return invalid(err, spec->name, NO_STR, PR3_S("registered twice"), NO_STR, NO_STR);
    if (core->n_tools == core->cfg.max_tools) {
        core_error_set(err, PR3_CORE_ERROR_OUT_OF_MEMORY, 0, 0);
        return PR3_E_NOMEM;
    }
    pr3_arena_mark mark = pr3_arena_mark_get(&core->statics), scratch = pr3_arena_mark_get(&core->step);
    core_schema schema;
    pr3_tool_spec copy = *spec;
    pr3_status s = parse_schema(core, spec, &schema, err);
    pr3_str *caps = NULL;
    if (s == PR3_OK) {
        caps = PR3_NEW_N(&core->statics, pr3_str,
                         spec->n_required_capabilities > 0 ? spec->n_required_capabilities : 1);
        bool ok = caps != NULL && copy_str(&core->statics, spec->name, &copy.name) &&
                  copy_str(&core->statics, spec->description, &copy.description) &&
                  copy_str(&core->statics, spec->parameters, &copy.parameters);
        for (uint32_t i = 0; ok && i < spec->n_required_capabilities; i++)
            ok = copy_str(&core->statics, spec->required_capabilities[i], &caps[i]);
        if (!ok) s = PR3_E_NOMEM;
    }
    pr3_arena_rewind(&core->step, scratch);
    if (s != PR3_OK) {
        pr3_arena_rewind(&core->statics, mark);
        if (s == PR3_E_NOMEM) core_error_set(err, PR3_CORE_ERROR_OUT_OF_MEMORY, 0, 0);
        return s;
    }
    copy.required_capabilities = caps;
    uint32_t at = 0;
    while (at < core->n_tools && pr3_str_cmp(core->tools[at].name, copy.name) < 0) at++;
    memmove(core->tools + at + 1, core->tools + at, (core->n_tools - at) * sizeof *core->tools);
    memmove(core->schemas + at + 1, core->schemas + at, (core->n_tools - at) * sizeof *core->schemas);
    core->tools[at] = copy;
    core->schemas[at] = schema;
    core->n_tools++;
    return PR3_OK;
}

/* ---- Validation */

/* A reason being written: parts are copied as far as they fit, and the result is clipped
 * to PR3_CORE_REASON_BYTES with the marker. */
typedef struct reason {
    pr3_buf *b;
    size_t total;
} reason;

static void say(reason *r, pr3_str s) {
    size_t room = r->b->cap - r->b->len;
    size_t n = s.len < room ? s.len : room;
    if (n) memcpy(r->b->ptr + r->b->len, s.ptr, n);
    r->b->len += n;
    r->total += s.len;
}

static bool refuse(reason *r) {
    if (r->total > PR3_CORE_REASON_BYTES) {
        pr3_str head = pr3_str_clip((pr3_str){r->b->ptr, r->b->len}, PR3_CORE_REASON_BYTES);
        r->b->len = head.len;
        pr3_buf_put(r->b, CORE_MARKER);
    }
    return false;
}

static void say_u64(reason *r, uint64_t v) {
    char tmp[24];
    pr3_buf b = pr3_buf_make(tmp, sizeof tmp);
    pr3_buf_put_u64(&b, v);
    say(r, (pr3_str){tmp, b.len});
}

/* Whether the value at v is one of the property's values, compared canonically with -0.0
 * as 0.0. A value longer than every member cannot be one, so the buffer stops there. */
static bool in_enum(pr3_core *core, const core_prop *p, const pr3_json_tape *t, uint32_t v, size_t text_len) {
    size_t longest = 1;
    for (uint32_t k = 0; k < p->n_enum; k++)
        if (p->enum_keys[k].len > longest) longest = p->enum_keys[k].len;
    if (longest > canon_cap(text_len)) longest = canon_cap(text_len);
    pr3_arena_mark m = pr3_arena_mark_get(&core->step);
    pr3_buf b;
    PR3_CHECK(pr3_buf_from_arena(&core->step, longest, &b) == PR3_OK);
    pr3_status s = pr3_json_write(t, v, &core->step, &b, PR3_JSON_ZERO_UNSIGNED);
    PR3_CHECK(s != PR3_E_NOMEM);
    bool found = false;
    for (uint32_t k = 0; s == PR3_OK && k < p->n_enum && !found; k++)
        found = pr3_str_eq((pr3_str){b.ptr, b.len}, p->enum_keys[k]);
    pr3_arena_rewind(&core->step, m);
    return found;
}

bool core_tool_validate(pr3_core *core, const pr3_proposed_call *p, pr3_buf *args, pr3_buf *why,
                        uint32_t *tool) {
    reason r = {why, 0};
    uint32_t idx = core_tool_find(core, p->name);
    if (idx == CORE_NONE) {
        say(&r, PR3_S("unknown tool '"));
        say(&r, p->name);
        say(&r, PR3_S("'"));
        return refuse(&r);
    }
    const core_schema *schema = &core->schemas[idx];
    size_t max = core->cfg.max_tool_args_bytes;
    if (p->arguments.len > max) {
        say(&r, PR3_S("arguments are larger than "));
        say_u64(&r, max);
        say(&r, PR3_S(" bytes"));
        return refuse(&r);
    }
    pr3_arena_mark mark = pr3_arena_mark_get(&core->step);
    pr3_json_tape t;
    pr3_status s = pr3_json_parse(&core->step, pr3_str_bytes(p->arguments), &t, NULL);
    PR3_CHECK(s != PR3_E_NOMEM); /* the step arena is sized for any arguments */
    if (s != PR3_OK || pr3_json_kind_of(&t, 0) != PR3_JSON_OBJECT) {
        pr3_arena_rewind(&core->step, mark);
        say(&r, PR3_S("arguments must be a JSON object"));
        return refuse(&r);
    }
    bool ok = true;
    for (uint32_t k = 0; ok && k < schema->n_required; k++) {
        pr3_str want = schema->props[schema->required[k]].name;
        if (pr3_json_member(&t, 0, want) == PR3_JSON_NONE) {
            say(&r, PR3_S("missing required argument '"));
            say(&r, want);
            say(&r, PR3_S("'"));
            ok = false;
        }
    }
    const uint32_t *keys = NULL;
    uint32_t n = 0;
    if (ok) PR3_CHECK(pr3_json_members(&t, 0, &core->step, &keys, &n) == PR3_OK);
    for (uint32_t k = 0; ok && k < n; k++) {
        pr3_str key = pr3_json_str(&t, keys[k]);
        uint32_t v = keys[k] + 1;
        uint32_t pi = prop_find(schema, key);
        if (pi == CORE_NONE) {
            if (!schema->additional) {
                say(&r, PR3_S("unexpected argument '"));
                say(&r, key);
                say(&r, PR3_S("'"));
                ok = false;
            }
            continue;
        }
        const core_prop *prop = &schema->props[pi];
        if (!pr3_json_is_type(&t, v, prop->type)) {
            say(&r, PR3_S("argument '"));
            say(&r, key);
            say(&r, PR3_S("' must be "));
            say(&r, kind_name(prop->type));
            ok = false;
        } else if (prop->n_enum > 0 && !in_enum(core, prop, &t, v, p->arguments.len)) {
            say(&r, PR3_S("argument '"));
            say(&r, key);
            say(&r, PR3_S("' must be one of "));
            say(&r, prop->enum_list);
            ok = false;
        }
    }
    if (ok) {
        args->len = 0;
        args->overflow = false;
        s = pr3_json_write(&t, 0, &core->step, args, 0);
        PR3_CHECK(s != PR3_E_NOMEM);
        if (s != PR3_OK) {
            say(&r, PR3_S("arguments are larger than "));
            say_u64(&r, max);
            say(&r, PR3_S(" bytes"));
            ok = false;
        }
    }
    pr3_arena_rewind(&core->step, mark);
    if (!ok) return refuse(&r);
    *tool = idx;
    return true;
}
