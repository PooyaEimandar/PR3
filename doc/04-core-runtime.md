# Core runtime

`pr3_core` is PR3's agent loop: a deterministic, sans-I/O state machine. The host feeds it inputs (the user said something, the model answered, a tool finished) and performs the actions it returns (run the model, run a tool, propose an action, show a reply, store a trace). The same inputs always give the same actions and the same text, byte for byte, which the golden vectors pin ([D-15](19-decisions-and-risks.md)). It never copies the history into a request and never keys a map by a heap string: it hands out borrowed views and keys by integers ([part 03](03-memory-safety.md)).

## Interface

```c
#include <pr3/core.h>

pr3_core_config cfg = pr3_core_config_default();
size_t need = pr3_core_size(&cfg);                  /* exact bytes, or 0 if cfg is invalid */
void *mem = host_alloc(need);                       /* static array, malloc, mmap: the host's choice */
pr3_core *core;
if (pr3_core_init(mem, need, &cfg, &core) != PR3_OK) { /* ... */ }

pr3_core_register_tool(core, &spec, &err);          /* schema checked now, not per call */

pr3_input in = { .kind = PR3_INPUT_UTTERANCE, .at = now_ms,
                 .utterance = { .session = 1, .content = PR3_S("What is 2+2?") } };
pr3_actions out;
pr3_core_error err;
if (pr3_core_step(core, &in, &out, &err) == PR3_OK) {
    for (uint32_t i = 0; i < out.len; i++) { /* perform out.items[i] */ }
}
/* out and everything it points to stay valid until the next call that changes the core */
```

| Function | Purpose |
| --- | --- |
| `pr3_core_config_default()` | The default limits: `max_input_bytes` 16 KiB, `max_notes_bytes` 4 KiB, `max_history` 32, `max_tool_rounds` 4, `max_tool_calls_per_round` 4, `max_tool_calls_per_turn` 12, `max_tool_args_bytes` 4 KiB, `max_tool_output_bytes` 8 KiB; and the capacities `max_sessions` 64, `max_turns` 4 (concurrent turns), `max_tools` 64, `tool_bytes` 64 KiB and `history_heap_bytes` 8 MiB ([D-04](19-decisions-and-risks.md)) |
| `pr3_core_size`, `pr3_core_init`, `pr3_core_destroy` | The exact bytes a configuration needs (0 for an invalid one); the host supplies them, at any alignment, and gets them back |
| `pr3_core_register_tool` | Registers a tool; its schema is checked once, here, against the subset below |
| `pr3_core_step(core, &input, &actions, &err)` | One transition |
| `pr3_core_history`, `pr3_core_in_turn`, `pr3_core_untrusted_calls`, `pr3_core_untrusted_sources`, `pr3_core_tool_sources`, `pr3_core_upcoming`, `pr3_core_opening` | Queries |
| `pr3_core_forget`, `pr3_core_set_brief`, `pr3_core_restore` | Session management |
| `pr3_core_memory_report` | Every region's size and high-water mark, and the live sessions and turns |
| `pr3_tool_round_budget`, `PR3_AFTER_UNTRUSTED`, `PR3_CALL_CANCELLED` | The execution-budget message, the refusal text after untrusted input, and the result of a call its turn's end cancelled |
| `pr3_core_error_text` | The text of each `pr3_core_error` ([below](#errors)) |

**Views.** Everything a call returns borrows from the core: the actions of a step, and a query's messages, calls and labels. They stay valid until the next call that changes the core (a step, a registration, `forget`, `set_brief` or `restore`); a query's views also end at the next call of the same query. A finished turn's slot is released at the start of the next changing call, not at its end, so the trace its `TRACE` action points to stays readable until then. A host that needs the data longer copies it; the common host renders the prompt or stores the trace at once and copies nothing.

### Types

```c
typedef uint64_t pr3_session_id;   /* chosen by the host */
typedef uint64_t pr3_request_id;   /* correlates INFER with MODEL_OUTPUT */
typedef uint64_t pr3_timestamp;    /* milliseconds since the Unix epoch, from the host clock */

typedef enum pr3_role { PR3_ROLE_SYSTEM, PR3_ROLE_USER, PR3_ROLE_ASSISTANT, PR3_ROLE_TOOL } pr3_role;

typedef struct pr3_tool_call {     /* validated: name registered, arguments canonical JSON */
    uint64_t id;                   /* written as "call-<id>" wherever an id is shown as text */
    pr3_str  name;
    pr3_str  arguments;
} pr3_tool_call;

typedef struct pr3_message {        /* fields in size order: 48 bytes, no padding */
    pr3_str  content;
    pr3_timestamp at;
    const pr3_tool_call *tool_calls; /* calls the assistant made in this message */
    uint64_t tool_call_id;           /* for PR3_ROLE_TOOL: the call this answers; 0 = none */
    pr3_role role;
    uint32_t n_tool_calls;
} pr3_message;
```

The inputs and actions are tagged unions: `PR3_INPUT_UTTERANCE`, `PR3_INPUT_ABORT`, `PR3_INPUT_MODEL_OUTPUT`, `PR3_INPUT_MODEL_FAILED`, `PR3_INPUT_TOOL_OUTCOME`; `PR3_ACTION_INFER`, `PR3_ACTION_CALL_TOOL`, `PR3_ACTION_PROPOSE`, `PR3_ACTION_REPLY`, `PR3_ACTION_FAILED`, `PR3_ACTION_TRACE`. Input strings are borrowed for the duration of the call; the core copies what it keeps.

**Call ids.** Calls are named `call-1`, `call-2`, … in everything the model and the trace see. The core stores the number and renders the text when it serializes. A host that receives ids as text (the vector runner, the protocol) parses `call-<n>` with `pr3_call_id_parse`.

## The turn

The algorithm, step by step:

1. **Utterance.** Refuse input over `max_input_bytes` (16 KiB) or blank after trimming, and a session whose turn is running. Fit the host's notes into `max_notes_bytes` (4 KiB) in order, trimmed, blank ones left out, stopping at the first that does not fit. Prefix them with the "What you remember about the user…" block, one line per note. Push the user message, open a turn with trace id `pr3-<session>-<turn>` and a loop guard sized `max_tool_calls_per_turn` (12), record a retrieve step if notes were given, and return one `INFER`.
2. **Model output.** Look the request up; it must be the one the turn awaits. Truncate the content to `max_input_bytes` on a character boundary with `" …[truncated]"`, and the backend's model and engine names to 256 bytes. Record a generate step with at most `max_tool_calls_per_round` (4) proposals, names clipped to 64 bytes and arguments to `max_tool_args_bytes` (4 KiB). No calls: push the reply and finish with success. Otherwise:
3. **Tool calls.** Count the round; more than `max_tool_rounds` (4) fails the turn. Consider the first four proposals; if any considered proposal names an untrusted-output tool, or the turn already read untrusted text, consequential calls in this message are refused with `PR3_AFTER_UNTRUSTED`. Validate every considered proposal before issuing any: unknown tool, arguments over `max_tool_args_bytes` (4 KiB) as written or once canonical, not a JSON object, schema violation. Run every valid call through the loop guard; a repeat or the twelfth-plus call fails the turn. Push the assistant message with every shown call (refused ones clipped to 64-byte names and 256-byte arguments), push an `error: <reason>` tool result per refused call (reason clipped to 512 bytes), push one note for calls over the per-message limit (a tool message that answers no call, `tool_call_id` 0: `error: at most <n> tool calls per message; <m> more were not run`). If nothing is runnable, ask the model again; otherwise return `CALL_TOOL` for each valid call, or `PROPOSE` where the tool `requires_confirmation`.
4. **Tool outcome.** The call must be pending in its session's turn. Truncate the output to `max_tool_output_bytes` (8 KiB), prefix `error: ` on failure, push it, mark the turn untrusted if the tool has untrusted output (success or failure), record the step. When the last pending call is answered, return the next `INFER`.
5. **Infer.** Allocate the next request id, set the turn awaiting it, and build the request: the session's history with its brief on the first user message, the registered tools, and, from the second round on, the execution-budget system message; with no rounds left the tools are withheld.
6. **Finish.** Drop the turn and its pending request and calls, close the trace (a respond step on success, with `turns = rounds + 1`), and return `REPLY` or `FAILED` followed by `TRACE`. A model failure or an abort finishes with its reason, clipped to 512 bytes. A call still open when the turn ends (an abort's) is answered first, in order, with the tool result `error: cancelled before its result arrived` (`PR3_CALL_CANCELLED`) and a failed `tool_call` step, so no later request shows a call without its result. That text is a constant the message points to, so ending a turn takes no memory and cannot fail; the exchange's room already counts one result per call. A result that arrives afterwards is stale (`unknown or already answered tool call`).

**History.** At most `max_history` (32) messages per session. Before a push into a full history, whole exchanges are dropped from the front: everything up to the next user message after the first. The exchange in progress is never split, so a tool result never loses its call and the current question is never dropped. The session's record holds the larger of `max_history` and the longest exchange one turn can push (2 + `max_tool_rounds` × (`max_tool_calls_per_round` + 2)); a history that cannot be trimmed, a restored run whose only user message is its first, loses its oldest message when it reaches that size. The record has twice that many slots: dropping from the front only moves where the history starts, and a push that finds the last slot taken moves the live messages back to the first, so trimming copies nothing on most turns and the history is always one contiguous array.

**Restore, brief, forget, opening, upcoming.** `restore` brings back only user and assistant messages without tool traffic, never over existing history and never during a turn; the messages are kept as if pushed one by one, so the history's trimming decides which survive, and each is clipped to the longest user message a turn can push (the input limit and the notes block). A brief is one line within `max_notes_bytes`, shown before the first user message as "What you remember from earlier in this conversation…", and is not part of the history; `forget` refuses a running turn.

## Tool registry and the schema subset

`pr3_core_register_tool` accepts a small subset of JSON Schema and nothing else, so a schema can never promise a check that goes unchecked:

- The top level is `{"type":"object", "properties", "required", "additionalProperties", "description"}`; any other key is refused with `'<key>' is not supported`.
- Each property is `{"type", "description", "enum"}` with one type of `string`, `number`, `integer`, `boolean`, `object` or `array`; an `enum` is a non-empty list of values of that type.
- `required` names declared properties; `additionalProperties` defaults to `false`.
- Names are 1–64 bytes of lowercase ASCII letters, digits and `_`; a name registers once.
- A schema is at most 8 KiB (`PR3_TOOL_PARAMETERS_MAX_BYTES`). At most `max_tools` (64) tools register; the next is refused with `PR3_E_NOMEM`, as is one that does not fit `tool_bytes`. A refused registration leaves the registry as it was.

Schemas are parsed once into a compact form in the static arena: properties sorted by name in byte order with their kind and, for enums, the parsed values. A call's arguments are parsed into a token tape in the step arena, checked in one pass (missing required argument first, then each argument in key order: wrong type, value outside the enum, unexpected argument), and written out in canonical form straight from the tape. Enum membership compares values structurally, so `1` and `1.0` differ and `0.0` equals `-0.0`.

**Canonical JSON** ([D-08](19-decisions-and-risks.md)). Input: nesting at most 127 levels; a duplicate key keeps its last value; numbers that fit `uint64_t` or `int64_t` stay integers, and `-0` and every other number become doubles, correctly rounded (the nearest double, ties to even): exactly, by one multiplication or division, when the digits and the power of ten are both exact doubles, and otherwise by decimal arithmetic on the first 800 significant digits with a flag for any nonzero digit beyond them; a number beyond the range of a double is refused, and one below it reads as zero. UTF-8 is checked over the whole input first; after that the first fault from the left is reported, with its byte offset. The tape is sized for the worst case of the input's length (every JSON text of *n* bytes has at most *n*/2 + 1 values), so a text that needs more values cannot be JSON and is refused as such; an arena of `pr3_json_parse_bytes(n)` bytes never runs out, and the writer needs at most `pr3_json_write_bytes` more. Output: object keys sorted in byte order, no whitespace, strings escaped as `\"`, `\\`, `\b`, `\f`, `\n`, `\r`, `\t` and `\u00xx` (lowercase hex) for other control characters, everything else as raw UTF-8; doubles in the shortest form that reads back to the same value (Ryu), in decimal notation from 1e-5 up to below 1e16 and scientific notation otherwise: `100.0`, `0.0001`, `1e16`, `1e-7`. An integer-kind argument accepts only numbers that stayed integers.

## Loop guard

The guard keeps, per turn, an array of `max_tool_calls_per_turn` + 1 entries holding a 64-bit SipHash of the name and canonical arguments and a view of the bytes, and compares bytes on a hash match. The first repeat stops the turn with `loop detected: identical call to '<name>' with the same arguments`; the call over budget with `tool budget exceeded: <n> calls (budget <budget>)`. The check runs over the valid calls of a message in order, after all of them are validated and before any is issued.

## Traces

A trace has these keys: `trace_id`, `query`, `agent` (`"pr3"`), `model`, `engine`, `steps` (`retrieve`, `generate`, `tool_call`, `respond`, each with `at`, `duration_ms`, `input`, `output`), `result`, `outcome`, `started_at`, `ended_at`, `total_tokens`. The core records each step as a compact struct in the turn arena, pointing at the bytes it already copied; `pr3_trace_write_json` writes canonical JSON only when the host exports it.

## Memory layout

`pr3_core_size` adds up, with checked arithmetic:

| Region | Sized from | Holds |
| --- | --- | --- |
| Static arena | Tools are registered after init, so a fixed `tool_bytes` budget (64 KiB default) | Tool specs, parsed schemas |
| Session pool | `max_sessions` × the session record | Per session: twice the record's message slots (trimming moves the start, not the messages), the brief, its last-use stamp |
| History heap | `history_heap_bytes` | A buddy allocator ([below](#the-history-heap)) for message bodies, tool-call arrays and source labels |
| Turn pool | `max_turns` × the worst case one turn can record under the configured limits | Trace steps and their bytes, pending calls, the loop guard |
| Step arena | The largest single step's output under the configured limits | The actions, the request's message views, the brief-joined first message and the budget message |

### The history heap

Message bodies have different sizes and lifetimes that do not nest: an exchange dies when it is trimmed, sessions die independently. A bump arena cannot serve that, and a general `malloc` is what the libraries avoid. The history heap is a **buddy allocator** over one power-of-two region: blocks of 2^k bytes from 64 bytes up, split on allocation and coalesced with their buddy on release. Allocation and release are O(log n) with a free-list bitmap, there is no external fragmentation that can lock memory into one size, internal fragmentation is under half a block, and the region's high-water mark is tracked like an arena's. Every session's history shares it, so an idle session costs only its record and its messages, not a worst-case reservation.

**When the heap or the pools are full.** A session pool that is full evicts the least recently used session with no turn in progress; the host restores it from storage the next time it is used. When every session has a turn in progress, the call that needs a new session fails with `PR3_E_NOMEM` ("out of memory"). A heap that cannot fit a message fails the step the same way, in a turn or out of one, and leaves the core unchanged: a turn waits as it was, and the host may retry the step once it has freed space (`forget`) or abort the turn. A turn pool that is full fails the utterance the same way ([D-04](19-decisions-and-risks.md)). Only the heap and the pools can run out. The step and turn arenas are sized by `pr3_core_size` for the worst case the limits allow, and every step checks that what it needs fits before it changes anything, so they never run out in a step that has started.

`pr3_core_memory_report` gives each region's size and high-water mark: the static arena, the heap (in use and highest), the step arena, the largest one turn has used against a turn slot's size, and the live sessions and turns against their pools. The tests and the benchmarks check the reported size against these marks.

## Errors

A refusal returns a status and, through the caller's `pr3_core_error`, its kind and detail; `pr3_core_error_text` writes what the host shows. A refusal never changes the core.

| Kind | Status | Text |
| --- | --- | --- |
| `INPUT_TOO_LARGE` | `PR3_E_LIMIT` | `input is <n> bytes; the limit is <limit>` |
| `EMPTY_INPUT` | `PR3_E_INVALID` | `input is empty` |
| `UNKNOWN_REQUEST` | `PR3_E_NOT_FOUND` | `unknown or already answered request <id>` |
| `UNKNOWN_TOOL_CALL` | `PR3_E_NOT_FOUND` | `unknown or already answered tool call call-<n>` |
| `TURN_IN_PROGRESS` | `PR3_E_STATE` | `still answering the previous message` |
| `INVALID_TOOL_SPEC` | `PR3_E_INVALID` | `invalid tool: <name>: <why>`, for example `invalid tool: t: 'pattern' is not supported` |
| `OUT_OF_MEMORY` | `PR3_E_NOMEM` | `out of memory` |

## Lookups

| Lookup | How | Cost |
| --- | --- | --- |
| Session by id | Sorted array of (id, slot) with binary search | One cache line per probe at 64 sessions |
| A session's turn | The session record holds its turn's slot | O(1) |
| Request by id | A turn awaits at most one request; scan the live turns | At most `max_turns` compares |
| Call by id | A turn has at most four pending calls; scan the live turns | At most 16 integer compares |
| Untrusted sources of a call | A flag and source labels on the tool-result message itself | Trimming the message drops them with it |

## Tests

`test/unit/test_core.c` holds the core's behaviour tests, each named for what it pins ([test ledger](20-test-ledger.md)). The vector runner replays `test/vectors/core/steps.jsonl`: 29 handwritten scenarios and 32 seeded random ones, 2,378 calls with every action, error and query result compared byte for byte. `tools/gen-vectors/core_steps.py` writes them from a model of the core written from this document alone, so the C core and the model must agree. The suite also replays a scripted conversation under every history-heap size from 64 bytes up (each refusal must leave the core unchanged, and the run must go on as the reference did once space is freed), runs seeded random steps against the core's invariants, and drives the worst case each limit allows to check `pr3_core_size` against the high-water marks.

`test/fuzz/fuzz_core_step.c` runs the core from any byte string, as a script of steps, registrations, session calls and queries on small limits, so pools fill, histories trim and the heap runs out ([part 14](14-build-and-quality.md#fuzzing)). Two cores in separate blocks, at different alignments, run each script in lockstep. Every refusal must say why and change nothing, both cores must answer alike, every call in a request, and in an idle session's history, must have exactly one result after it, every call and proposal must name a tool of its kind with canonical arguments, every trace must write canonical JSON, and once every turn is aborted and every session forgotten, nothing may be left in use.

## Performance targets

Measured by the module benchmarks ([part 15](15-benchmarks.md#module-budgets)) on the development Mac and the Android phone:

| Workload | Target |
| --- | --- |
| A plain turn (utterance → infer → reply → trace) | Zero heap allocations; the time budget set from the first measurement in BEN-03 and held by the regression gate |
| A tool turn (two rounds, two calls each, schema validation, canonicalization) | Zero heap allocations; budget as above |
| An infer step on a full 32-message history | Constant in the history's byte size (views, not copies) |
| Core memory for one session and one turn at default limits | Exactly what `pr3_core_size` reports, checked against the arenas' high-water marks |


---

[Back to the index](../README.md)
