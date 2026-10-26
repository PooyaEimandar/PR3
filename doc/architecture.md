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
| **Bundled GGUF** (`pr3-llamacpp`) | Any device that passes a memory/thermal profile; all desktops | Coverage where no platform model exists; control over weights, templates and tool-calling format; reproducible benchmarks | Optional download, Wi-Fi only, app usable before it lands. Mobile: CPU backend first, Metal on iOS, Vulkan on Android as a measured per-device-class upgrade. Desktop: CPU/Metal/Vulkan/CUDA as available |
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

Suggested tables: `sessions`, `messages`, `facts`, `sources`, `goals`, `task_events`, `tool_calls`, `policies`, `approvals`, `memory_tombstones`, `evaluations`, `device_sync_log`.

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
