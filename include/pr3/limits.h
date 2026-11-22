/* Every limit PR3 enforces, each with the reason for its value (doc/03, rule 7).
 * A limit that changes is a reviewed change: its edge is pinned by a test named in
 * doc/20-test-ledger.md. */
#ifndef PR3_LIMITS_H
#define PR3_LIMITS_H

/* ---- Text the user and the model exchange (doc/04, pr3_core_config_default). */

/* One user message. A long paste fits; a whole document goes through the document
 * store instead, which has its own limit. */
#define PR3_DEFAULT_MAX_INPUT_BYTES (16u * 1024u)

/* The host's notes about the user, joined into one block before a message. Kept small
 * so notes cannot crowd the conversation out of the context window. */
#define PR3_DEFAULT_MAX_NOTES_BYTES (4u * 1024u)

/* Messages kept per session. Older whole exchanges are trimmed first. */
#define PR3_DEFAULT_MAX_HISTORY 32u

/* Model rounds that may call tools within one turn, so a turn always ends. */
#define PR3_DEFAULT_MAX_TOOL_ROUNDS 4u

/* Calls considered per model message, and per turn; the turn limit also sizes the
 * loop guard. */
#define PR3_DEFAULT_MAX_TOOL_CALLS_PER_ROUND 4u
#define PR3_DEFAULT_MAX_TOOL_CALLS_PER_TURN 12u

/* A tool call's arguments as JSON. Every schema PR3 registers fits in far less; the
 * bound also sizes the JSON tape in the step arena. */
#define PR3_DEFAULT_MAX_TOOL_ARGS_BYTES (4u * 1024u)

/* A tool's result as the model sees it, so one result cannot fill the context. */
#define PR3_DEFAULT_MAX_TOOL_OUTPUT_BYTES (8u * 1024u)

/* Capacities: sessions held at once, turns in flight, and the shared history heap
 * (doc/19 D-04). */
#define PR3_DEFAULT_MAX_SESSIONS 64u
#define PR3_DEFAULT_MAX_TURNS 4u
#define PR3_DEFAULT_HISTORY_HEAP_BYTES (8u * 1024u * 1024u)

/* Tools registered at once, and the static arena their specs and parsed schemas use. */
#define PR3_DEFAULT_MAX_TOOLS 64u
#define PR3_DEFAULT_TOOL_BYTES (64u * 1024u)

/* A tool's parameter schema as JSON. Every schema PR3 registers needs far less; the
 * bound sizes the scratch it is parsed in. */
#define PR3_TOOL_PARAMETERS_MAX_BYTES (8u * 1024u)

/* ---- What the core keeps of text it does not control (doc/04, "The turn"). */

/* A refused call stays in the conversation only in outline: its name and arguments are
 * clipped, so a junk proposal cannot grow every later prompt. */
#define PR3_CORE_REFUSED_ARGS_BYTES 256u

/* A refusal or failure reason, shown to the model or the user on one or two lines. */
#define PR3_CORE_REASON_BYTES 512u

/* Origin labels of one untrusted result, and the length of each: provenance has its own
 * bound, so turning notes off cannot hide sources. */
#define PR3_CORE_SOURCES_PER_RESULT 16u
#define PR3_CORE_SOURCE_BYTES 256u

/* A model or engine name as the trace records it. */
#define PR3_CORE_MODEL_NAME_BYTES 256u

/* ---- Names. */

/* Tool and property names: 1 to 64 bytes of [a-z0-9_]. Long enough for any
 * descriptive name, short enough to print in one line of a refusal. */
#define PR3_NAME_MAX_BYTES 64u

/* ---- Canonical JSON (doc/04). */

/* Nesting of arrays and objects. Real tool arguments nest two or three levels; 127
 * bounds the parser's explicit stack at 127 bytes of state. */
#define PR3_JSON_MAX_DEPTH 127u

/* ---- Base library. */

/* Smallest block of the history heap: room for a short message body and the two
 * 32-bit links a free block stores. */
#define PR3_BUDDY_MIN_ORDER 6u

/* Largest history heap, as a power of two: 2^36 bytes (64 GiB), so block indices in
 * units of the smallest block fit in 32 bits. */
#define PR3_BUDDY_MAX_ORDER 36u

#endif /* PR3_LIMITS_H */
