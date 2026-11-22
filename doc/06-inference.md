# Inference

Two engines behind one interface. The **llama.cpp backend** calls llama.cpp at a pinned release through `llama.h` directly, with no wrapper in between ([D-12](19-decisions-and-risks.md)); it is the first engine PR3 ships and the baseline every engine change is measured against ([part 15](15-benchmarks.md)). The **PR3 engine** is PR3's own, in C: a GGUF loader, a tokenizer, CPU kernels for the quantizations the pinned models use, Qwen3 and Qwen3.5 execution, then SIMD, Metal and Vulkan. Everything above the engine (prompt formats, the streaming text pipeline, tool-call parsing, KV reuse and checkpoints, evidence fitting, the report grammar) is shared.

## Interface

```c
typedef struct pr3_engine_config {      /* defaults */
    uint32_t n_ctx;                     /* 4096 */
    uint32_t max_new_tokens;            /* 512 */
    int32_t  gpu_layers;                /* 0; 999 on Apple GPUs */
    bool     thinking;                  /* false */
    float    temperature, top_p;        /* 0.7, 0.8 */
    int32_t  top_k;                     /* 20 */
    float    presence_penalty;          /* 1.5 */
    uint32_t seed;                      /* 0 */
    bool     capture_prompt_evidence;   /* false */
} pr3_engine_config;

pr3_status pr3_infer_open(const pr3_infer_backend *backend, pr3_str model_path,
                          const pr3_engine_config *cfg, pr3_infer **out);
pr3_status pr3_infer_chat(pr3_infer *e, const pr3_chat_request *req,
                          pr3_token_sink *sink, pr3_chat_output *out);
pr3_status pr3_infer_read_ahead(pr3_infer *e, const pr3_chat_request *req, pr3_str opening,
                                pr3_read_ahead *out);
void       pr3_infer_stop(pr3_infer *e);   /* sticky, safe from any thread */
void       pr3_infer_close(pr3_infer *e);  /* frees context, then model */
```

`pr3_infer` runs on the host's inference thread ([part 02](02-architecture.md#threads)); it is not thread-safe except for `pr3_infer_stop`, an `_Atomic bool` with release/acquire ordering. A `pr3_chat_request` carries messages (role, content, tool-call texts, whether a tool result is saved evidence), tool definitions (`read_only`, `report`, name, description, JSON Schema text) and an optional GBNF grammar. Errors have fixed display texts (`message too long for the model (<p> tokens; context is <n>)`, `the model was stopped`, …).

`pr3_infer_backend` is a table of function pointers with two implementations, `pr3_backend_llama` and `pr3_backend_pr3`, selected at open. The benchmark build links both; a release links one ([ENG-10](17-delivery-plan.md)).

## The host above the engine

### Orchestration

Policy ([INF-01](17-delivery-plan.md)):

- **Guarded answers.** Without a grammar, when any message is a tool result: drafts are never streamed (the sink gets empty pulses so cancel stays responsive); a planning pass runs with tools and returns its calls if it proposes any; otherwise a tool-free answer pass runs, and an answer pass that proposes calls or answers nothing fails with `could not finish an answer from the available results`; a good answer is emitted as one chunk. Usage sums over passes.
- **Repeated plans.** A call repeats when it has the same name and JSON-equal arguments (key order ignored) as an assistant call after the last user message. If every proposed call repeats and every one is read-only, the engine appends that turn and a `NOT EXECUTED. This batch repeats proposals already made in this user turn. …` tool message and generates once more; repeating again clears the calls and runs the answer pass; a mix of repeats and new calls fails with `could not choose a new step from the recorded results`.

### Prompt formats

Detected from the model's chat template: it must contain `<|im_start|>`; `<function=` means Qwen3.5 (with `preserve_thinking` if the template mentions it), else `<tool_call>` means Qwen3, else the model is refused with `unsupported model: PR3 runs Qwen3 and Qwen3.5-family GGUFs`.

Rendering produces **pieces**, each tokenized on its own: only `<|im_start|>` and `<|im_end|>` are control pieces (tokenized with special tokens parsed); untrusted text (user messages, tool results) is split right after the `<` of any of `<tool_call>`, `</tool_call>`, `<tool_response>`, `</tool_response>`, `<think>`, `</think>`, so text from a page can never form one of Qwen's user-defined tokens. Boundaries (`system_end`, `query_end`, `continuation_start`) are recorded as piece counts for checkpoints.

Every policy text, reminder, query prefix and answer opening lives in `src/infer/prompt_text.h`. They are host-written preludes the model has been evaluated with, so a changed byte is a changed product; golden vectors render every prompt fixture and compare bytes ([INF-01](17-delivery-plan.md)).

The tool section, the protection rule (tool results become `Untrusted source data (JSON string)` or `(JSON)` with the evidence view, followed by a reminder turn), the continuation turn and the answer openings are fixed texts. Tool JSON inside prompts uses the `", "` and `": "` spacing the Qwen templates expect, written straight from the JSON tape.

### Budget fitting

A prompt fits when its tokens plus `min(max_new_tokens, n_ctx / 2)` fit `n_ctx`. Windows start at the first non-tool message and at each later user message; the system message is always kept. A window that does not fit tries the evidence byte budgets in order (`[800×4]`, `[800×3]`, `[800,500,400]`, … `[160]`), then its refinements, accepted only if they extend coverage. Nothing fits: `PR3_INFER_CONTEXT_FULL`.

Fitting is incremental: the host renders each piece once per request into the request arena and keeps a per-request cache of piece tokens keyed by the piece's hash, so trying another window or budget re-tokenizes only the pieces that changed (the evidence views).

### Streaming text

Three stages run as one streaming state machine over bytes with fixed buffers; every byte is examined once and nothing is allocated per token:

| Stage | Behaviour | Implementation |
| --- | --- | --- |
| UTF-8 | Incomplete trailing bytes wait; invalid bytes become U+FFFD; leftover bytes at the end become one U+FFFD | At most 3 pending bytes |
| Think filter | `Undecided → InsideThink → Visible` over `<think>`/`</think>`; text that could still start `<think>` is held; leading whitespace before visible text is dropped; an unterminated think block is dropped; starts inside when the opening already opened one | A tag matcher with a 7-byte hold buffer: every byte is examined once |
| Tool-block extractor | Blocks between `<tool_call>` and `</tool_call>` are removed from the stream and kept; the longest suffix that could start the open tag is held; an unterminated block still counts | Visible text goes to the sink in place; call text is appended to a per-request buffer bounded by the call limits |

Pieces leave through the token ring as at most 64-byte chunks ([part 02](02-architecture.md#threads)). The visible text is also kept for the final answer, trimmed.

### Tool-call parsing

| Format | Parse | Render back into history |
| --- | --- | --- |
| Hermes (Qwen3) | Trim, parse JSON; an object's `name` (string, else empty) and `arguments` (a string verbatim, any other value as compact sorted JSON, missing as `{}`); anything else is malformed (empty name, raw text as arguments) | `<tool_call>\n{"name": "NAME", "arguments": ARGS}\n</tool_call>` |
| XML (Qwen3.5) | `<function=NAME>`, then `<parameter=KEY>` values ended by `</parameter>`, the next `<parameter=` or the end, one leading and one trailing newline stripped; values converted by the schema's declared type (first non-`null` entry of a type list): undeclared → string, `null` (any case) → null, `integer`, `number` (whole values below 2^53 become integers), `boolean` (any case), anything else parsed as JSON; a failed conversion keeps the string; without `<function=` but starting with `{`, Hermes | `<tool_call>\n<function=NAME>\n<parameter=K>\nV\n</parameter>\n…</function>\n</tool_call>` |

Both parsers run over the extracted block buffer with no allocation outside the request arena, and both have fuzz targets seeded with real model output.

### Sampling

The chain, in order: the grammar (the host's or the automatic report grammar; an invalid grammar is an error, never a fallback to unconstrained output), presence penalty over the last 64 tokens (repeat 1.0, frequency 0.0), top-k, top-p (min keep 1), temperature, then the seeded distribution. Generation stops at end-of-generation or a control token, which is not decoded; the budget is `min(n_ctx − prompt, max_new_tokens)`.

### KV reuse, checkpoints and read-ahead

The plan (`None`, `Clear`, `Trim`, `Restore`) from the longest common token prefix, always re-decoding the last prompt token for its logits; a saved system prefix (a full sequence state taken at `system_end` when tools are offered) preferred when it extends further; on recurrent and hybrid models, checkpoints of the partial sequence state at the token ends of `system_end`, `query_end` and `continuation_start`, at most two (the first and the most recent). Read-ahead renders the next turn's prompt with two different probe endings and fills their common prefix in batches of 32, yielding between batches to any request that arrives.

A checkpoint of a recurrent model's sequence state is large (about 52 MB for Qwen3.5-4B, 156 MB for Qwen3.5-27B), so the host keeps a pool of three state buffers sized from `llama_state_seq_get_size_ext` at load, reused by handle, and one batch of `n_ctx` allocated once: a long session's checkpoint traffic allocates nothing.

### Evidence and the report grammar

**Evidence selection** picks exact excerpts from saved evidence packets lexically: rarity weights, round-robin across origins, at most three spans of 500 bytes per source, each with a `quote_ref`. **The report grammar** is a GBNF grammar that admits only citations of verified quotes in the fitted evidence, at most 32, applied when a read-only report tool is offered, thinking is off and every tool is read-only. Both are tested on the fixtures under `test/fixtures/evidence/`. `quote_ref` is a wire format: `q:` + `%08x` of `offset × 512 + len` + the first 16 hex digits of `pr3_hash_of(["pr3:quote-reference:v1", source, packed as u32 BE, text])`.

## The llama.cpp backend

- **Revision.** llama.cpp release `b11429` (commit `d81235049384534c167caea52b85a694f6103d14`), fetched through `third_party/manifest.txt`. Upgrades are deliberate and come with a conformance run of the prompt, tokenizer and model fixtures.
- **Build.** llama.cpp's own CMake with Metal on Apple, the static C++ runtime on Android, `GGML_NATIVE` off for release builds, and `-O3` even in PR3's debug builds, so a debug build still measures a realistic engine.
- **Process rules.** One backend initialisation per process; logs only when `PR3_LLAMA_LOGS` is set; `offload_kqv` and `op_offload` off when no layers are offloaded; the context freed before the model, and the model released before process exit, because Metal aborts if buffers outlive it.

## The PR3 engine

The engine that replaces llama.cpp in shipped builds once it passes the same fixtures and beats it on the device ([M6](16-roadmap.md)). It is where "close to the bare metal" pays most: no general-purpose framework, one model family, memory planned from the file's header.

| Part | Design |
| --- | --- |
| GGUF | `mmap` (or the board's flash mapping) and a bounded reader of the header, metadata and tensor table; every offset, size, dimension and type checked with checked arithmetic against the mapped length before use; unsupported types refused by name ([ENG-01](17-delivery-plan.md)) |
| Tokenizer | Qwen's byte-level BPE: the pre-tokenizer as a hand-written scanner equivalent to the model's split pattern (no regex engine), merges in a perfect hash built at load, special tokens from the vocabulary; compared token for token with llama.cpp on English, Persian, mixed-script, Unicode edge-case and control-marker fixtures |
| Memory plan | At load, from the header: weights stay mapped; KV cache, recurrent state, activations and scratch are laid out in one arena with every buffer's offset computed once; a forward pass allocates nothing |
| Kernels | Scalar kernels first for exactly the types the pinned files contain (Q4_K, Q5_K, Q6_K, Q8_0, F16, F32 as found by the inventory in [ENG-02](17-delivery-plan.md)), checked per operation against recorded llama.cpp outputs; then NEON and AVX2/AVX-512 paths with the scalar ones kept as the oracle |
| Model | Qwen3 attention blocks and Qwen3.5's interleaved recurrent and full-attention blocks as the files declare them; prefill in batches, decode per token; checkpoints of the recurrent state at the same boundaries as the llama.cpp backend |
| Threads | A fixed worker pool created at load, split by rows; no thread creation per token |
| GPU | Metal on Apple (resource lifetimes, command-buffer synchronization, cancellation between command buffers, a low-memory fallback to the CPU), then Vulkan for Android and Linux on a named device ([ENG-08](17-delivery-plan.md)) |

## Performance targets

Measured with MLPerf LoadGen on the same device and model file ([part 15](15-benchmarks.md#mlperf-inference)):

| Measure | The llama.cpp backend under PR3 | The PR3 engine |
| --- | --- | --- |
| Decode tokens/s | Equal to llama.cpp's own tools within noise; PR3's overhead per token under 2 µs | At least llama.cpp's on CPU; Metal at least llama.cpp's on Apple GPUs |
| Time to first visible token | Pieces tokenized once, no per-token allocations | At most llama.cpp's |
| Peak memory | Pooled checkpoint buffers, one batch, no copies of the conversation | Weights mapped plus one planned arena |
| Cancel to idle | Within one token | Within one token |

---

[Back to the index](../README.md)
