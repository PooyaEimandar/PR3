# Architecture

Part of the [PR3 documentation](README.md). Section numbers are stable; code comments cite them as “docs Sec. N”.

## 4. Reference Architecture

```text
   +------------------+   +------------------+   +--------------------------+
   | Mobile app       |   | Desktop app      |   | Non-Rust hosts           |
   | Android / iOS    |   | macOS/Win/Linux  |   | consoles, Unreal Engine, |
   | Tauri 2 / native |   | Tauri 2 / native |   | MCU/TEE, OEM C/C++       |
   +--------+---------+   +--------+---------+   +------------+-------------+
            |                      |                           |
   +--------v----------------------v--------+        +---------v----------+
   | PR3 Runtime (Rust)                     |        | pr3-client-c (C11) |
   | pr3-core: no_std, sans-I/O step()      |        | protocol framing,  |
   | agent | policy | memory | learning     |        | host callbacks     |
   +-----+--------------+-------------+-----+        +---------+----------+
         |              |             |                        |
 +-------v------+ +-----v------+ +----v--------------+         |
 | Platform LLM | | Bundled    | | Private hybrid    |<--------+ pr3-protocol
 | Apple FM /   | | GGUF via   | | pr3-hybrid:       |           over TLS
 | Gemini Nano  | | llama.cpp  | | pairing, handoff, |
 | (where avail)| | in-process | | encrypted sync    |
 +--------------+ +------------+ +--------+----------+
                                          |
                                 +--------v-----------+
                                 | pr3-node           |
                                 | user's desktop or  |
                                 | self-hosted, 8B-32B|
                                 +--------------------+

        Optional application plugins: email / calendar / files / OEM
        Publisher-hosted push relay: pr3-node -> relay -> APNs/FCM -> phone
        (opaque wake-ups only; UnifiedPush on Android; Sec. 6.3)
```

**Logical distinction:** A task can be *executed remotely* without synchronizing all personal memories; and memories may be *synchronized* without granting remote agents authority to execute actions. These must be separate protocols and permission scopes. A protocol client has *execution* access only to the scopes its host was granted.

### 4.1 Functional modules

| Module | Crate ([Sec. 4.2](#42-rust-workspace-host-api-c-abi-and-protocol-client)) | Responsibilities | Mandatory? |
|---|---|---|---|
| `core` | `pr3-core` | Context/session ownership, state transitions, events, cancel/retry | Yes |
| `protocol` | `pr3-protocol` | Versioned wire schema for requests, events, handoff and sync | Yes |
| `model_api` | `pr3-inference` | Inference contract; model capabilities, token and context metadata | Interface yes; implementation no |
| `tool_api` | `pr3-agent` | Versioned schema, permission check, asynchronous result | Yes |
| `policy` | `pr3-core` (trusted side) | Capability checks, tool allowlists, approval receipts, risk classes; deterministic, never LLM-decided | Yes |
| `intent` | `pr3-core` (trusted side) | Structured-intent schema and validator: runtime-stamped envelope (identity, nonce, time), unknown-field rejection, bounds on model-chosen values, freshness, replay prevention, reference resolution without exposing secrets, minimum data to policy ([Sec. 4.6](#46-structured-intent-and-execution-sequence)) | Yes |
| `identity` | `pr3-core` (trusted side) | Device, user, agent and protocol-host identities; key registration; session binding; revocation | Yes |
| `audit` | `pr3-core` (trusted side) | Append-only, hash-chained action log with signed checkpoints; intent ↔ decision ↔ approval ↔ capability ↔ result correlation | Yes |
| `vault` | `pr3-vault` | Adapter over platform key storage (Secure Enclave, StrongBox, TPM, Keychain/Keystore); returns results or scoped capabilities, never raw secrets | Default app adapter |
| `memory_api` | `pr3-memory` | Read/write/retrieve/delete abstractions; source/provenance metadata | Interface yes |
| `learning_policy` | `pr3-learning` | Collect evaluations and choose model/strategy based on traces | Optional initial build |
| `personal_model` | `pr3-learning` | Style profile and preference model learned from the user's own messages and approval history; local only, never changes permissions | Optional |
| `storage_sqlite` | `pr3-memory-sqlite` | SQLite/FTS5 persistent data, migrations, encryption adapter | Default app adapter |
| `inference_platform` | `pr3-platform-llm` | Apple Foundation Models / Gemini Nano backends | Optional per platform |
| `inference_llama` | `pr3-llamacpp` | llama.cpp native local inference | Optional per target |
| `inference_remote` | `pr3-hybrid` | Authenticated private-node inference | Optional |
| `sync` | `pr3-hybrid` | E2EE device-state replication and conflict resolution | Optional |
| `conversation` | `pr3-agent` | Voice and text sessions, streamed replies, conversation → memory proposals | Yes |
| `task` | `pr3-core` (lifecycle), `pr3-memory-sqlite` (records) | Task records and lifecycle, which task a message follows up, resume, task history and retention ([Sec. 4.8](#48-tasks-and-conversation-continuity)) | Yes |
| `speech` | `pr3-speech` | On-device speech-to-text and text-to-speech; platform engines first, whisper.cpp fallback; mouth-shape timing for the optional face | Optional per platform |
| `client_c` | `pr3-client-c` | C11 protocol client for non-Rust hosts | Optional |
| `app_connectors` | application plugins ([Sec. 4.7](#47-connector-and-plugin-model)) | Email, calendar, files, payments, OEM SDK | Optional and separately permissioned |

### 4.2 Rust workspace, host API, C ABI and protocol client

**Canonical API:** strongly typed Rust traits and events. Rust and Tauri hosts import `pr3` via Cargo and call it directly; there is **no FFI boundary between a Rust host and the PR3 Rust core**.

Workspace (logical organization; consolidate crates if they do not justify the maintenance cost):

```text
pr3/
├── Cargo.toml                 # workspace, features, pinned toolchain
├── rust-toolchain.toml
├── config/pr3.toml            # model per device class, each pinned by revision and SHA-256
├── crate/
│   ├── pr3-core/              # sans-I/O step() state machine, pure types; trusted-side modules: intent, policy, capability, audit, identity
│   ├── pr3-agent/             # structured tool calling, deterministic lifecycle
│   ├── pr3-protocol/          # versioned wire schema (requests, events, handoff, sync)
│   ├── pr3-inference/         # model-neutral traits and streamed token events
│   ├── pr3-memory/            # provenance, retrieval, forgetting (traits only)
│   ├── pr3-memory-sqlite/     # SQLite/FTS5 adapter, migrations, encryption (std)
│   ├── pr3-learning/          # trace analysis, replay, routing-policy evaluation
│   ├── pr3-platform-llm/      # Apple Foundation Models / Gemini Nano backends via Swift/Kotlin shims
│   ├── pr3-speech/            # on-device STT/TTS: platform engines, whisper.cpp fallback
│   ├── pr3-llamacpp/          # bundled GGUF backend; vendored llama.cpp + frozen C shim
│   ├── pr3-hybrid/            # device pairing, task handoff, encrypted sync
│   ├── pr3-vault/             # adapter over platform key storage; scoped capability release, receipt signing, no raw secrets
│   ├── pr3-web/               # browser host API / JS bindings (Wasm)
│   └── pr3-ffi/               # C ABI + UniFFI; "full" (std) and "core" (no_std) builds
├── client-c/                  # pr3-client-c: C11, no deps, host callbacks, Emscripten build
├── app/pr3-node/             # desktop private inference and handoff service
├── examples/tauri/            # Tauri 2 host integration
├── examples/browser/          # Wasm/JS example, mock and remote inference
├── examples/unreal-client/    # Unreal C++ host using pr3-client-c (native + HTML5)
└── bench/                   # benchmark harnesses, never shipped: AgentDojo (bench/agentdojo); matched baselines
```

**Crate modes.** The rule is `no_std + alloc` always, `std` additive. Any crate listed as `std only` is an adapter and may not be depended on by a `no_std` crate.

| Crate | Mode | `unsafe` |
|---|---|---|
| `pr3-core`, `pr3-agent`, `pr3-protocol`, `pr3-inference` (traits), `pr3-memory` (traits) | `no_std + alloc`; `std` feature additive | forbidden |
| `pr3-ffi` **full** build | `std` staticlib/cdylib with UniFFI and every adapter (bundled GGUF, SQLite, vault, hybrid); `panic = "unwind"`, unwinds caught at every exported function. Mobile, desktop and native-game hosts | permitted, audited |
| `pr3-ffi` **core** build | `no_std + alloc` staticlib: core and trusted-side modules only, inference/storage/network supplied by host callbacks; `panic = "abort"`. [RFC-001](rfc/001-console-cognition-tiers.md) hosts | permitted, audited |
| `pr3-llamacpp` | `std` only | permitted, audited |
| `pr3-platform-llm`, `pr3-vault`, `pr3-speech` | `std` only | `#![deny(unsafe_code)]`; permitted only in one audited `sys` module for platform FFI |
| `pr3-hybrid`, `pr3-memory-sqlite`, `pr3-learning`, `pr3-node`, `pr3-tool` | `std` only | forbidden |
| `app/pr3-app` (reference app) | `std` only | permitted only in audited platform calls: physical memory on Apple, backup exclusion on iOS |
| `pr3-web` | either; `no_std` keeps Wasm small | forbidden |
| `pr3-client-c` | C11, freestanding-capable | n/a (C) |

**Two `pr3-ffi` builds, two profiles.** Cargo does not allow `panic` to be set per package, and UniFFI and the `std` adapters cannot link into a `no_std` artifact, so the full and core builds are separate invocations with their own profiles (`release` with unwind; `release-core` with abort).

**Platform FFI.** Apple Foundation Models is Swift-only, Gemini Nano is reached through Kotlin, and Android Keystore and Windows CNG/TPM are JNI and Win32 calls - all of which need `unsafe` when called directly. Prefer paths that keep the call safe on the Rust side: Tauri 2's mobile-plugin bridge in Tauri hosts, and host-implemented UniFFI callback interfaces (generated in `pr3-ffi`) in native hosts. Direct calls that remain go in the crate's `sys` module, under the same sanitizer and review rules as the other audited crates. `pr3-speech` follows the same rule; its whisper.cpp fallback reuses the `pr3-llamacpp` pattern (pinned submodule, frozen C shim, prebuilt libraries) inside that module.

**Trusted-side modules stay separable.** Inside `pr3-core`, `intent`, `policy`, `capability`, `audit` and `identity` are modules with no dependency on the agent loop or on inference types. They can be split into their own crates, moved behind a platform-protected boundary, or compiled into protection domains on a verified microkernel later without an API change ([Sec. 7.3](security.md#73-trust-tiers---dev-protected-secure)). CI enforces the dependency direction: agent and inference modules may depend on trusted-side types; the reverse is a build error.

**`pr3-core` constraints (enforced, not aspirational):**

```toml
# crate/pr3-core/Cargo.toml
[features]
default = ["std"]
std = ["serde/std", "serde_json/std"]      # conveniences only, never behavior

[dependencies]
serde      = { version = "1", default-features = false, features = ["alloc", "derive"] }
serde_json = { version = "1", default-features = false, features = ["alloc"] }
hashbrown  = { version = "0.15", default-features = false }   # no "default-hasher": maps take a host-keyed hasher
```

```rust
#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
extern crate alloc;
```

- `std` may add `impl From<std::io::Error>`, `Display` helpers and `Arc` conveniences. Everything load-bearing is written against `core` and `alloc`; `core::error::Error` is stable, so error impls do not need `std`.
- **No `RandomState`.** It needs `std` and hides a dependency on OS entropy. Use `BTreeMap` for small maps (also HashDoS-immune, relevant for untrusted JSON keys) or hashbrown with a hasher keyed from host-supplied entropy. hashbrown is pulled in without its `default-hasher` feature, so `HashMap::new()` does not compile and every map must be built with the host-keyed hasher.
- **No time, randomness, I/O or threads.** Host supplies `Clock` and entropy traits. Core needs no async at all; `Future`/`Stream` wrappers live in adapters.
- **Panic-free in practice.** On a `panic = "abort"` staticlib a panic takes the host down. Beyond the lints above, CI lists the call sites that reach `core::panicking` in the `no_std` core artifact (e.g., with `cargo-call-stack` on `thumbv7em-none-eabihf`) and checks them against a checked-in allowlist. Zero panic symbols is not achievable: any code that grows a `Vec` or uses `serde_json` links `alloc`'s capacity-overflow and allocation-failure paths. Those library sites are on the allowlist; a new panic site in PR3 code fails the job.
- **Fallible allocation at untrusted boundaries.** Default `alloc` aborts on OOM, and consoles/embedded run out. Use `try_reserve` wherever input size comes from outside: tool results, protocol frames, token buffers.
- **No float math in core**, or pull `libm`. Deterministic numeric engines (amounts, limits, scoring) live in their own `std` crates.
- **CI builds the `no_std` configuration on a real target** - `cargo check -p pr3-core --no-default-features --target thumbv7em-none-eabihf` (no `std` exists there, so leaks fail loudly) and `--target wasm32-unknown-unknown`. Cargo unifies features across a workspace, so a root `cargo check` silently enables `std` for every crate; the `no_std` check must be its own command.

Representative *interface sketch* (not an implemented API):

```rust
/// Pure, sans-I/O. The host owns the loop.
pub trait Runtime {
    fn step(&mut self, input: Input) -> Result<alloc::vec::Vec<Action>, CoreError>;
}

pub trait InferenceBackend {
    type Stream: futures_core::Stream<Item = Result<TokenEvent, InferenceError>>;
    fn capabilities(&self) -> ModelCapabilities;   // context, tool-calling, guided-output, privacy_class
    fn generate(&self, request: InferenceRequest) -> Self::Stream;
}

pub trait ToolPolicy {
    fn authorize(&self, action: &ToolAction, context: &PolicyContext) -> AuthorizationDecision;
}
```

These traits are conceptual; production signatures must address lifetimes, cancellation, `Send` requirements and executor independence. Tokio may be used in native adapters and `pr3-node`; browser Wasm uses a browser-compatible scheduler.

**C ABI for non-Rust hosts that can link the Rust runtime:**

```c
/* Conceptual ABI; generated/versioned interface is not yet implemented. */
typedef struct pr3_context pr3_context_t;
typedef enum { PR3_OK = 0, PR3_PENDING = 1, PR3_DENIED = 2,
               PR3_ERROR = -1 } pr3_status_t;
pr3_status_t pr3_create(const pr3_config_t*, pr3_context_t**);
pr3_status_t pr3_submit(pr3_context_t*, const pr3_request_t*);
pr3_status_t pr3_poll(pr3_context_t*, pr3_event_t*);
pr3_status_t pr3_cancel(pr3_context_t*, uint64_t task_id);
void pr3_destroy(pr3_context_t*);
```

Specify ABI version negotiation, opaque ownership, buffer lengths, thread safety, callbacks, allocators, asynchronous polling and panic containment (panic-free trusted-side code is the primary defense; the full build catches unwinds at every exported function and returns `PR3_ERROR`; the core build aborts, so a panic there takes the host down - which is why its panic sites are allowlisted). Use `cbindgen` for the header and UniFFI for idiomatic Swift/Kotlin. **The C ABI does not make Rust compile for closed proprietary console toolchains**; that is what the protocol client is for.

**`pr3-client-c` for hosts that cannot link the Rust runtime:**

- C11, zero dependencies, freestanding-capable: no `malloc`, no sockets, no threads, no `stdio` of its own. The host supplies allocator, `send`/`recv`, monotonic clock and entropy callbacks - the same host-owned-resources principle as the core.
- Implements `pr3-protocol` framing (CBOR; FlatBuffers to be evaluated for C++ game teams), request/event queues, reconnection state and approval-receipt verification. It never interprets tool results or holds secrets.
- Builds in CI as: Linux x86_64/ARM64 from M0; from M4, a "console-style" freestanding build with stubbed callbacks and an Emscripten build using the emsdk pinned by the Unreal HTML5 fork recorded in M4-E3.
- Fuzzed (libFuzzer/AFL on the frame parser), ASan/UBSan, MISRA-ish lints where cheap. Console and OEM partners compile it with their own SDKs without touching the open core.

### 4.3 Inference strategy

Three backend classes behind one trait. Routing is by **capability** (context, tool-calling, guided output), **privacy class** of the request, and **device profile**:

| Backend | Where | Role | Notes |
|---|---|---|---|
| **Platform model** | Apple Foundation Models (iOS/iPadOS/macOS 26+ on Apple Intelligence devices); Gemini Nano via ML Kit GenAI / Prompt API on supported Android devices | Default local backend where available: zero download, OS-maintained, ~3B class, guided generation and tool calling | Black box; no custom weights; device and region gaps; model may not be downloaded yet. ML Kit's GenAI APIs may send usage metrics to Google - evaluate against PR3's privacy claims before enabling by default |
| **Bundled GGUF** (`pr3-llamacpp`) | Any device that passes a memory/thermal profile; all desktops | Coverage where no platform model exists; control over weights, templates and tool-calling format; reproducible benchmarks | Optional download, on Wi-Fi unless the user allows mobile data, app usable before it lands. Mobile: CPU backend first, Metal on iOS, Vulkan on Android as a measured per-device-class upgrade. Desktop: CPU/Metal/Vulkan/CUDA as available |
| **Private node** | User's paired desktop or self-hosted `pr3-node`, 8B–32B+ | Escalation for long reasoning, long context, and all proactive/background work | Explicit, authorized TLS endpoint; receives no more context than the task needs |

- **Mobile:** the local model is load-bearing, not an accelerator. Most users will not run a node, so without local inference the choice collapses to "cloud or nothing." Deterministic engines filter first; the LLM explains, summarizes and answers retrieval-backed questions.
- **Desktop:** `llama.cpp` as the portable default; optional MLX (Apple), ONNX Runtime or ExecuTorch where supported and advantageous.
- **Research / export:** PyTorch and tools such as the ExecuTorch exporter remain possible *outside* production runtime.
- **No model assumption:** models, chat templates, tokenizer and tool-calling formats are versioned per model. Never assume all GGUFs implement reliable structured tool calls. Platform models declare capabilities through the same `ModelCapabilities` struct.
- **Default models: the Qwen3.5 generation, chosen per device class** in `config/pr3.toml` and pinned by revision and SHA-256 (v1.9). Desktop and node: Qwen3.8-27B dense, 4-bit (about 19 GB, so 32 GB of memory or more), with the 8-bit file as the near-lossless option on larger machines and Qwen3.6-35B-A3B (mixture-of-experts, about 3B active parameters) as the faster option if spoken replies need it. Phone: Qwen3.5-4B, 4-bit (2.7 GB), on devices with 8 GB or more; Qwen3.5-2B (1.3 GB) below that. The 4B model is measured on an 8 GB iPhone before it becomes the default there, with iOS's increased-memory-limit entitlement as the fallback. Phone and desktop run different sizes but one chat template and tool-call format, so prompts and parsing stay identical; Qwen3-0.6B stays as a test and fallback model. Non-thinking mode serves fast replies, thinking mode harder node tasks. The M2 benchmark still decides per platform between Qwen and the platform model.

**Disclosure policy - routing by privacy class.** Before any context leaves the deterministic layer for a backend, the router evaluates and records in the trace: which backend is selected; whether it is approved for this data class; whether local inference is required; what redaction applies; the minimum context needed; whether the backend may retain data; and whether user or deployment policy forbids sending it off-device. Platform-model vendors' retention and telemetry behavior are inputs to this table and are measured before a backend is enabled by default. The learning policy may change *which approved* backend is chosen; it may never change the approval itself ([Sec. 4.5](#45-learning-architecture)).

**Speech, voice first.** Speech is a pipeline: on-device speech-to-text → the selected model → text-to-speech, each behind a `pr3-speech` trait so backends swap the way models do. Platform engines come first (the on-device speech recognizers and voices of Apple and Android); whisper.cpp is the fallback where no on-device recognizer exists, and a local neural voice (such as Piper) a later option. Voice notes - speak, get a written reply - work everywhere and come first; low-latency spoken conversation targets the node. End-to-end speech models (Qwen-Omni class) are a node experiment only, because today they run mainly on Python stacks the runtime does not ship. Push-to-talk comes before any always-listening wake word, and raw audio is kept only if the user chooses.

**`pr3-llamacpp` binding strategy (M1's primary engineering risk).** Spike status: items 1–2 are replaced by pinned `llama-cpp-2` bindings behind PR3's own frozen Rust API, and item 3's prebuilts are deferred ([ADR-003](adr/003-llamacpp-binding.md)). The target design:

1. Vendor llama.cpp as a git submodule pinned to a reviewed commit; upgrade on a cadence with a conformance run, never ad hoc.
2. Write a **tiny C shim with a frozen API** over the subset PR3 uses (load, tokenize, decode step, sampling hooks, KV-cache control, release). Bindgen the shim, not llama.cpp. Upstream API churn is absorbed in the shim.
3. Build per-target static libraries of llama.cpp + shim in CI (macOS arm64/x86_64, Linux, Windows, Android arm64 via NDK, iOS arm64 + simulator) and publish them as checksummed release artifacts. An explicit fetch step (`cargo xtask fetch-llama`, or the host's CI cache) pins URL and SHA-256 and places them in a configured directory; `build.rs` only links what is there, never downloads, skips linking on docs.rs, and rebuilds from source only with a feature flag. This keeps CMake out of every downstream `cargo build` while keeping `--offline` and reproducible builds working; bundling the libraries into the published crate is ruled out by crates.io's 10 MB crate limit.
4. Cargo features map 1:1 to ggml backends (`cpu`, `metal`, `vulkan`, `cuda`); CPU is always compiled in as the fallback.
5. All `unsafe` lives in this crate; the public API is safe Rust with explicit `Send`/`Sync` story and cancellation. Fuzz the shim boundary; run under ASan/UBSan in CI.

### 4.4 Memory and knowledge architecture

Use **SQLite** as the first persistent engine, with FTS5 for lexical retrieval. Add small local embedding models (candidate: Qwen3-Embedding 0.6B) and vector search only after demonstrating gains on held-out tasks. Avoid heavyweight standalone vector DBs in the default distribution.

Suggested tables: `sessions`, `messages`, `facts`, `sources`, `goals`, `tasks` ([Sec. 4.8](#48-tasks-and-conversation-continuity)), `task_events`, `tool_calls`, `policies`, `approvals`, `memory_tombstones`, `evaluations`, `device_sync_log`.

Each important memory record must include: `source_id`, `created_at`, `valid_at`, `confidence`, `privacy_class`, `principal_id`, `consent_scope`, `retention_policy`, and deletion status. Memory extraction is a proposal; user-provided corrections supersede extracted inferences. Conversations - voice transcripts and text - are a memory source like any other: extracted facts and preferences are proposals the user can confirm or correct, preferences fade unless reinforced, and other people's content stays tagged with its principal and is never disclosed to third parties. Treat retrieved documents as data, never as instructions.

**Three classes of data, kept apart:**

```text
Canonical user data:   notes, tasks, contacts, messages, documents, calendar events
Derived AI data:       summaries, extracted facts, embeddings, rankings, forecasts
Ephemeral context:     the minimum prompt assembled for one request
```

Derived indexes are rebuildable from canonical data; the embedding store is never the only copy of anything. User memory is inspectable, editable, exportable, deletable, provider-independent, encrypted, scoped by project and identity, and usable without any model.

**Audit as a hash chain.** `task_events`, `tool_calls` and `approvals` form an append-only log in which each record carries the hash of its predecessor; `pr3-vault` signs periodic checkpoints with a device key held in platform key storage. Every record correlates intent → policy decision → approval → capability → result, so a user can ask "why did this happen?" and an auditor can verify that nothing was silently rewritten. Export is a first-class operation, so users and researchers can verify the chain independently.

For multi-device consistency, start with a **single-writer ownership model plus append-only events**, not immediate full CRDT complexity. Use stable event IDs, vector/version metadata, conflict reporting and explicit tombstones. Roll out E2EE only after clear key provisioning, pairing, recovery and device-revocation rules.

### 4.5 Learning architecture

Separate **inference-time adaptation** from **weight updates**:

- **Trace capture:** task class, model version and backend class, tool choices, tokens, time, power, outcomes, errors, approvals (with privacy redaction).
- **Feedback evaluator:** objective task result where measurable, plus user feedback as distinct signals.
- **Model/workflow router:** heuristics initially (including platform-vs-bundled-vs-node choice); contextual bandit or learned routing after adequate observations.
- **Policy promotion:** candidate policy must improve held-out metrics and must not regress on privacy/security guardrails.
- **Optional neural post-training:** SFT/LoRA/GRPO off-device, only with explicit data permission; deployment via exported weights/adapters for the bundled backend only (platform models cannot be fine-tuned).
- **Personal model:** a style profile learned from the user's sent messages (retrieval of examples first; an optional LoRA adapter on the node, trained per model size) and a preference model learned from approve/deny history in the audit log. Both stay on the user's devices, are promoted only on held-out improvement, and fall under the rule below like every learned policy.

Do **not** let an online-learning policy modify permission constraints, spending limits, signing rules, allowed recipients, or the privacy class a backend is allowed to receive.

### 4.6 Structured intent and execution sequence

A model never calls a consequential tool directly. It emits a **structured intent** - a typed proposal in which the model chooses only enumerated or bounded values and refers to everything else by reference (here a contact, a draft and a file). It never supplies secrets, raw addresses, identities or replay-protection values:

```json
{
  "intent_type": "mail.send",
  "recipient_refs": ["contact:alex"],
  "draft_ref": "draft:2026-10-19/q3-summary",
  "attachment_refs": ["file:documents/q3-summary.pdf"],
  "reason": "user_request:send_summary",
  "evidence_refs": ["message:inbox/2026-10-18/alex-asks-for-summary"]
}
```

The runtime - not the model - wraps the proposal in an **envelope** before validation. Identity comes from the session's binding ([Sec. 6.3](deployment.md#63-private-hybrid-mobile--desktop)), freshness and replay values from host-supplied clock and entropy:

```text
Envelope (set by the runtime):
  actor:          agent:mail-assistant      session binding
  principal:      user:…                    session binding
  request_nonce:  01JPR3…                   host entropy
  received_at:    2026-10-19T06:00:00Z      host clock
  proposal:       { …model output above… }
```

The **intent validator** (trusted side, `no_std`) rejects unknown fields on security-sensitive intents - including any identity, nonce or timestamp field the model tries to supply, so a prompt-injected model cannot pick its own replay values or claim another principal; checks every model-chosen value against its enumeration or bound (a known `intent_type`, no more than the allowed number of recipients and attachments); resolves every `*_ref` against canonical data - a contact the user already has, a draft owned by this principal, a file inside the granted scope - without handing the model raw values or secrets; checks the envelope's freshness and nonce against a replay window; normalizes into an internal type; and forwards only the minimum fields the policy decision needs. The deterministic engine - not the model - resolves the actual addresses, attachment hashes and sizes, so a prompt-injected model cannot type an attacker's address or attach a file outside its scope.

**Execution sequence** (the same shape for mail, calendar, payments and file sharing; mail shown):

1. **Model → runtime → intent validator:** `mail.send` as above, wrapped in the runtime's envelope.
2. **Validator → deterministic checks:** resolved recipients (known contacts, none added in this session), attachment scope, size and data class ([Sec. 4.3](#43-inference-strategy) disclosure policy), rate limits.
3. **Checks → policy:** normalized action with its risk class (an external recipient plus an attachment is high risk).
4. **Policy → decision:** deny; allow (low risk, within a pre-authorized scope); or **confirmation required**.
5. **Confirmation ([Sec. 6.3](deployment.md#63-private-hybrid-mobile--desktop)):** PR3's own UI on the phone renders the exact recipient addresses, subject, attachment names, sizes and hashes, and expiry; the user approves with OS biometrics; `pr3-vault` signs the approval receipt.
6. **Vault → execution adapter:** a one-use, time-boxed capability bound to the receipt - never the mail provider's OAuth token ([Sec. 7.2](security.md#72-required-controls) item 5).
7. **Execution adapter** (sandbox connector or the user's mail provider) sends once; the idempotency key prevents a second send on retry or reconnect. For live providers it runs wherever the provider credential lives - on the phone, not the node ([Sec. 6.3](deployment.md#63-private-hybrid-mobile--desktop)).
8. **Audit:** intent, decision, receipt, capability and result are appended to the hash chain; the user receives a completion receipt that links them.

No step gives the model credentials, raw secrets, or a way to widen its own scope. "No action" is a valid outcome at steps 2–5 and is audited like any other.

### 4.7 Connector and plugin model

Connectors expose **typed operations**, never uncontrolled screen automation. Preferred integration order:

1. A stable application or provider API.
2. OS-level agent integration (Apple App Intents, Android AppFunctions) where available - used, never made the core protocol.
3. An approved MCP-style connector with a reviewed manifest.
4. Accessibility/UI automation only as a low-risk fallback.
5. **Never** screen automation for payment, identity or credential operations.

Every connector and plugin ships a **signed manifest** the policy engine reads: declared permissions (capability grants, [Sec. 7.2](security.md#72-required-controls)); data classes it may read and emit; operations with their reversibility level; confirmation rules per operation; rate limits; audit semantics; version and publisher identity. Manifests are data: a plugin cannot widen its own grants at runtime, and neither can the learning policy ([Sec. 4.5](#45-learning-architecture)). Arbitrary code is never imported by default; a WASI-component sandbox for third-party plugins is the candidate mechanism when the ecosystem opens (compare ZeroClaw's plugin host, [Sec. 5.1](related-work.md#51-zeroclaw-reuse-candidates-at-the-adapter-layer)).

### 4.8 Tasks and conversation continuity

PR3 carries work forward through **tasks** and **memory**, not one long chat thread that the model reads back. A thread grows without bound, mixes topics, and must be read again whole once the app has closed, which on a phone's CPU costs time and battery; and whatever it holds reaches later prompts, both text the user never approved as memory ([Sec. 4.4](#44-memory-and-knowledge-architecture)) and untrusted text from tools and pages. A task is bounded instead: it has a goal, a state and an outcome, keeps its own chat for the user to read, and gives the model no more of it than the next step needs ([Sec. 4.3](#43-inference-strategy)).

**What a task is.** One goal of the user's that PR3 works on until it is done, cancelled or given up: "send Alex the Q3 summary", "remind me about the dentist on Friday", "what did the landlord say about the deposit?". A question answered in one turn is a task that opens and closes in that turn. A task record holds:

| Field | Holds | Set by |
|---|---|---|
| `id`, `principal`, `created_at`, `updated_at` | Identity and times | Runtime |
| `goal` | The request in the user's own words | The message that opened it |
| `title` | A short name for the task list | Model proposal; the user can rename it |
| `state` | Where it is in the lifecycle below | Runtime only |
| `summary` | What is known and decided so far, a few sentences | Model proposal when the task is followed up or outgrows its budget |
| `refs` | The contacts, drafts, files, memories and messages it works on, by reference ([Sec. 4.6](#46-structured-intent-and-execution-sequence)) | Runtime, from validated calls |
| `pending` | What it waits on: an approval, the user, a time, the paired computer, Wi-Fi | Runtime |
| `outcome` | The result and its audit receipt, or why it stopped | Runtime |
| `parent` | The task it follows up, if any | Runtime |
| `retain_until` | When it is erased | Retention policy |

The model proposes only the title and the summary; the deterministic runtime sets everything else, as it does for intents. A task's state is never read from model text.

**Lifecycle.** Each transition is a `task_events` record in the audit hash chain ([Sec. 4.4](#44-memory-and-knowledge-architecture)), so "why did this happen?" has an answer per task:

```text
  message ---> open ---> working (a turn) ---- needs something ----> waiting
                            ^                                          |
                            +---------------- it arrives --------------+

  working ---> done | cancelled | failed
  waiting ---> expired | cancelled
```

A turn runs inside a task: the core's turn state (rounds, loop guard, pending tool calls) is unchanged, and the task is the layer above it that lasts between turns and across restarts. A waiting task survives the app being closed. An approval never survives as given: after a restart the confirmation card is shown again, an expired card ends the task as `expired`, and nothing is retried silently. M1-E1's lifecycle, cancellation, retry and checkpointing ([Sec. 11](roadmap.md#11-delivery-roadmap-epics--issues)) are built on this record.

**Follow-ups: which task a message belongs to.** Deterministic rules first; the model only proposes, and only where the rules leave a choice:

1. An answer to something the task asked - a confirmation card, a question, a choice - belongs to that task.
2. A message within the follow-up window of the last active task (10 quiet minutes by default) continues it, unless the user said "New topic" or the message names another task. If it seems to start something new anyway - it asks something of its own, a question or a request rather than more detail for the task or an answer to PR3, none of its own words are in the task's title, opening message or last exchange, and it does not point back ("send it", "the same to Alex") - it is answered in the task first, and then PR3 offers to move it into a task of its own.
3. "Carry on with X", "what happened with X", or tapping X in the task list resumes task X.
4. Otherwise a new task opens. If the model judges that it continues an older task, it may propose `task.resume(task_ref)`: the runtime resumes an unfinished task of the same principal, or opens a new task with `parent` set when the older one has finished. Tasks are never merged.

**What the model sees.** The prompt for each step is assembled fresh, within the token budget and in this order: the system prompt and tools; the task's goal, summary, refs and pending step; the memories retrieved for this message; and the task's latest exchanges, oldest dropped first. Resuming a task costs a few hundred tokens rather than a whole thread, so a 4,096-token window is enough on a phone; long-context work goes to the node. Everything a task brings back is data, not instructions: a resumed task cannot widen its own scope, and each consequential step asks for approval again unless a pre-authorized scope covers it ([Sec. 4.6](#46-structured-intent-and-execution-sequence)).

**What is stored, and for how long.**

| Data | Kept | Where |
|---|---|---|
| Task record | Unfinished tasks until they finish; finished ones 30 days by default, then erased. The user can keep, delete or export any task | `tasks`, in the memory database |
| Task events | As long as the audit chain; they hold states and references, not text | `task_events`, hash-chained |
| The task's chat (messages, and tool activity as the conversation showed it) | As long as the task, so the user can read it in **Tasks**; deleting the task deletes it | `messages`, in the memory database |
| Memories | As today: proposed from the task, kept only if the user confirms | `memories` |

A task's chat is the user's record, not the model's: the model reads only the task it is working on, as above, and following up a finished task starts from its summary, never its chat. Forgetting a memory removes it from every task's refs and erases every task's summary, since which summaries drew on it cannot be told; each is made again from its task's chat when next needed. The task keeps its goal, state and chat.

**Tasks in the app: casual, three pages.** The composer reads, left to right, **Settings**, **Tasks**, the message field, the microphone and **Send**: the microphone sits beside Send, and Tasks beside Settings. Settings slides in from the left over the whole screen, with Close at its top left; Tasks opens in place of the conversation, and tapping it again goes back to it. It holds three full-width pages, swiped left and right, with their names on top to tap:

```text
  [Settings] [Tasks] [ Message PR3 ........ ] [Mic] [Send]

  [To Do]    Doing    Done          swipe, or tap a name
  +------------------------------+
  | Email Alex the Q3 summary    |   a task: its title, one plain line,
  | Waiting for your OK - 2 min  |   and when
  +------------------------------+
  | Dentist on Friday            |
  | I'll remind you Fri 9:00     |
  +------------------------------+
```

- **To Do:** waiting for the user or for later: an approval, an answer, a time, or set aside unfinished with "New topic".
- **Doing:** PR3 is on it: the task the conversation is on, or one running on the paired computer.
- **Done:** finished, with how it ended in plain words: sent, answered, cancelled, did not work, expired.

Tapping a task opens its chat, as the conversation showed it, at its end, under a bar that stays: **Back**, the title and **More**. **Back** returns to the same page of the list, and **Tasks** goes home. From **More**, an unfinished task offers **Continue**, which makes it the conversation's task again; a finished one offers **Follow up**, which opens a new task linked to it; any task can be exported, or deleted after a confirmation. The words stay casual: titles and plain lines, never states, ids or references. Voice does the same: "what is still open?", "carry on with the email to Alex". On the home page the conversation shows the current task; when a new one starts, the last one folds into a line that says where it went ("Sent to Alex - in Done"). Under the answer to a message that seems to start something new, a line asks "Looks like a new task - start it on its own?": **Start new task** moves the message and its answer into a task of their own, and the task they went to folds away with only its own words; **Keep here** leaves them where they are.

**Across devices.** The phone owns its tasks (single writer, [Sec. 4.4](#44-memory-and-knowledge-architecture)). A task delegated to the paired computer ([Sec. 6.3](deployment.md#63-private-hybrid-mobile--desktop)) carries its goal, summary and the refs it needs, never a transcript; the result returns as the task's outcome, and approvals stay on the phone. M2-E4's delegation protocol moves task state.

**Measured, not assumed.** The PR3-Personal corpus ([Sec. 8.3](benchmarks.md#83-ai-benchmarks-and-external-participation)) gains tasks that span sessions: resumed after a restart, followed up after a change of topic, and interrupted while waiting for approval. One task must never pick up another's context, and the model's `task.resume` proposals are scored before they are on by default. The first 26 such tasks are in `bench/task`, with three more kinds: a task that outgrows what the model is given, tasks whose details only another task said, and changes of topic without "New topic", where the offer of a new task must show for the new topic and not for the follow-ups. They run on the app's own host and model, each launch in its own process, and check PR3's answers and emails for what the task said earlier and for what only another task said or the user took out; the baseline, and what summaries changed, are in [Sec. 8.6](benchmarks.md#86-results-so-far).

**From today's spike.** Steps 1 to 3 are built, and step 4's summaries. Each task is its own session in the runtime, with its last 32 messages, trimmed to fit 4,096 tokens. Right after the answer that takes a task's history past about 1,800 tokens (7 KB) or 24 messages, while the user reads it, its older exchanges are folded into its summary: the model writes, in at most six sentences, what the user told it about themselves and their plans, what they asked for and what was decided or done, with names, numbers, dates and places as written and general questions only as topics, starting from the summary so far; the latest two exchanges stay word for word, and the runtime goes on from those. A follow-up of a finished task starts from that task's summary, made as soon as the follow-up is chosen, while the user writes, if it has none yet, and read by the engine then, as the follow-up's first message begins with it. The summary, and beside it the message that opened the task as its goal, reach the model on one line before the first message of the task it is given, marked as facts, not instructions; neither is ever part of the history, and the goal is never summed up again. There they stay put from turn to turn, so the engine reuses what it has already read of the task; and after each answer, while the user reads it and writes, the engine reads the task as the next turn will show it, so that turn reads only its message. A message sent sooner waits for a fold still running, not for the reading, which gives way to it. A summary is asked without the system prompt and tools, which small models call instead of writing it; if the model writes none, the task is not summed up again until it has two more exchanges. Summaries are kept with their task in the memory database, with how many steps of its chat they cover, and the chat the model is given again after a restart starts there. Its record, chat and events are written to the memory database as they happen, so tasks survive the app being closed: when a kept task gets its next message after a restart, its answered exchanges - the user's messages and the replies, never tool output - come back into its session first. On opening, the conversation shows its task again while it waits for the user or is within the follow-up window. A turn cut off by closing the app is on To Do as not finished. A confirmation card still waiting is shown again as a new request through the whole trusted path, with a new approval; one that ran out while the app was closed ends the task as expired, and nothing is sent. Either way the old request is gone with the gate that held it, so when PR3 opens the gate logs every request left waiting for approval or holding an unused capability as expired in the audit log; one closed mid-delivery stays open, since only the connector knows whether it went out. Finished tasks, done or expired, are erased with their chats 30 days after they last changed (`keep_days` under `[tasks]` in `config/pr3.toml`); their events stay, hash-chained, and deleting or erasing a task is logged too. A message continues the current task while it runs or within 10 minutes of its last turn, and otherwise opens a new one; "New topic", "new task" or "new job", typed or said as the whole message in any case, sets the current task aside. A message that seems to start something new by rule 2 is answered in the current task and then offered a task of its own. **Start new task** moves the message and everything after it, tool lines included, into a new task, in one change to the memory database that logs the new task as opened; the task it came from goes back to how it stood before the message, and the runtime forgets that task's history, so its next turn restores it without the moved exchange. A task waiting for an approval, or whose last turn did not finish, is on To Do; the task the conversation is on is on Doing; the rest are on Done, with the last reply as their line. Switching tasks and deleting one wait until PR3 has answered, so a turn never changes task halfway, and deleting a task also drops its session from the runtime. Task words are answered from the task list by fixed rules, never by the model, and only as a whole message, typed or said: "what is still open?" lists To Do; "carry on with X" or "back to X" goes back to the one unfinished task whose title or opening message holds every word of X, offers a choice when several do, and follows up the newest finished one when only finished ones do; "what happened with X" shows where X stands; "export X" exports it. Words that name no kept task go to the model as usual, so "continue the story" still reaches it. Exporting a task writes two files: its chat as Markdown, to read, and its record, chat and events as JSON, each event with the hash before it and how that hash is made, so the events can be checked without the database. They go to the share sheet on iOS and Android and to a save dialog on a computer; PR3 sends nothing itself, and the copy it keeps for the share sheet is removed at the next export or launch. The build order:

1. Built: New topic, the follow-up window and the **Tasks** pages with Continue, Follow up and Delete, for tasks kept in memory for one launch; the microphone beside Send.
2. Built: task records, chats and events in the memory database; tasks survive restarts, including a card still waiting for approval; finished tasks erased after 30 days.
3. Built: tasks by voice ("what is still open?", "carry on with the email to Alex", "what happened with X", "export X"), and export as Markdown and verifiable JSON through the share sheet. Then the offer of a new task when a message seems to start one, in place of the New topic button.
4. Built: summaries when a task is followed up or outgrows its budget, measured against the baseline. Next: the model's `task.resume` proposal, once measured.
5. Delegating tasks to the paired computer (M2-E4).
