# PR3 — Portable Private Personal Intelligence

**Technical Architecture, Open-Source Strategy & Benchmarking**  
**Version:** 1.8 — Rust-first; sans-I/O `no_std` core; protocol-first reach; platform-model backends; trust-boundary schemas, trust tiers and verification lineage from the PR3/Samantha plan; application-neutral research roadmap; voice-first personal model  
**Date:** 19 October 2026  
**Status:** Rust-first architectural direction confirmed (ADR-001); trust architecture adopted with seL4 as a preserved tier (ADR-002); pending proof-of-concept and engineering validation  
**Working name:** PR3

---

## 0. Change log

### v1.8 (20 October 2026) — first model on device

1. **ADR-003** (Appendix C, §4.3): for the spike, `pr3-llamacpp` wraps the maintained `llama-cpp-2` bindings behind PR3's own frozen API instead of a hand-written C shim; prebuilt libraries are deferred.

### v1.7 (19 October 2026) — voice-first personal model

PR3's reference app becomes a voice-first private assistant that learns its user, kept deliberately simple at first.

1. **Voice first, text optional** (§1.1, §4.1–§4.3, §11): voice notes in, replies as text or speech by the user's choice; on-device speech through platform engines first, whisper.cpp as the fallback, in a new `pr3-speech` crate.
2. **Qwen3 as the default model family** for the bundled and node backends (§4.3), with Qwen3-Embedding as the candidate for vector memory (§4.4); the M2 benchmark against platform models still decides per platform.
3. **Personal model** (§2, §4.1, §4.4, §4.5, §11 M3): learns the user's style and routine choices on their own devices, and never acts as the user without confirmation or a pre-approved scope (§1.2).
4. **Security, evaluation and risks** (§7.1, §8.2, §13, §14): audio as untrusted input, voice never used as authentication, impersonation through a stolen personal model; speech and personal-model metrics.
5. **Optional digital face, later** (§6.6, §11 M6): a stylized glyph-rain face rendered with WebGPU in the Tauri app and lip-synced to the speech. No photoreal avatar.

### v1.6 (19 October 2026) — application-neutral research roadmap

PR3 is an open-source research project. v1.6 removes the domain application earlier versions planned on top of PR3, all legal and compliance work items, and all commercial planning. The architecture is unchanged.

1. **Domain application removed** (§1, §1.1, §2.2, §9, §11 M5, §13, §14, §16): §9 now states the rules any domain application built on PR3 must follow, and M5 becomes a domain-neutral sandbox milestone for consequential actions.
2. **Examples rewritten** around an outbound email instead of a trade (§4.6, §6.3, §7.2).
3. **Domain-specific evaluation removed** (§8.2–§8.5): the domain benchmark and gate are replaced by an actions beta.
4. **Legal and compliance work items removed** throughout; the repository is Apache-2.0.
5. **Positioning and headline result** (§1, §2, §2.3, §8, §13, §14, §16): PR3 is positioned as the layer underneath assistants, with cross-device continuity as its user-visible feature; the headline benchmark becomes authorization under prompt-injection attack (AgentDojo), replacing the OpenJarvis task-accuracy delta; CaMeL is recorded as the nearest research neighbour.
6. **Commercial content removed** (§1.2, §2, §10, §11, §12, §14, §16, Appendix D): markets, editions, design partners, the `pilot` release channel and business-companion pointers. §10 now covers application areas and interoperability, with a private personal PR3 — the user's own phone and node — as the first reference deployment.

### v1.5 (19 October 2026) — public-release preparation

v1.5 moved commercially sensitive detail out of this document ahead of the public release (M6) and added a `README.md`. Engineering content was unchanged.

### v1.4 (19 October 2026) — review corrections

v1.4 corrects security-design gaps, build claims that would not hold, and gate definitions found in review. Direction and scope are unchanged.

1. **Intent envelope** (§4.1, §4.6): identity, nonce and timestamp are stamped by the runtime from the session binding and host clock/entropy; the model supplies only enumerated or bounded values, and any identity or replay field in its output is rejected.
2. **Execution credential location** (§4.6, §6.3, §11 M5, §13): for live providers the credential and execution adapter stay on the phone; the node holds neither an approval key nor a provider credential, otherwise "prepare but not approve" does not hold.
3. **Push relay** (§4, §6.3, §7.1, §11 M2-E5): APNs/FCM require a publisher-operated relay (or UnifiedPush); it carries sealed wake-ups only and is in the threat model.
4. **`pr3-ffi` full and core builds** (§4.2, §6.5, Appendix B): `std` + UniFFI + adapters with unwind, and `no_std` core with abort, as separate profiles; Cargo cannot set `panic` per package.
5. **Panic-site allowlist** replaces "no panic symbols", which `alloc` makes unachievable (§4.2, §7.2, §8.5, §12).
6. **`pr3-memory-sqlite`** split from `pr3-memory`, so `std` stays additive (§4.1, §4.2).
7. **Platform FFI** (§3, §4.2, §7.2, §12, §13): `unsafe` allowed in one audited `sys` module of `pr3-platform-llm` and `pr3-vault`; Tauri's mobile-plugin bridge and UniFFI callbacks preferred.
8. **hashbrown without `default-hasher`** (§4.2), so maps must take the host-keyed hasher.
9. **Pinned nightly for verification jobs** (§3, §12); UBSan scoped to C/C++ code (§7.2, §12).
10. **llama.cpp prebuilts** fetched by an explicit checksummed step, never by `build.rs` (§4.3, §12).
11. **Roadmap and gates** (§8.5, §11, §16): milestone ↔ release gate ↔ go/no-go ↔ channel table; M1 trimmed (iOS prebuilts → M4; Gemini Nano spike and Android Keystore → M2; Kani → M3) with a numeric gate; M3 gate can fail; PR3-Personal corpus scheduled (M1-E8); `pr3-client-c` freestanding/Emscripten builds → M4, and the Unreal fork is recorded there.
12. **Benchmark-only HTTP backend** for matched comparisons against HTTP-based harnesses (§4.2, §8.1).
13. **Editorial**: crate column in §4.1; console tiers renamed C0–C2 (§6.5, Appendix B); v1.2 change-log scope corrected; unsourced claims marked (§2.1, Appendix B).

### v1.3 (19 October 2026) — lineage from the PR3/Samantha plan

v1.3 folds in the reusable parts of the earlier *PR3 / Samantha Master Plan* (27 August 2026). That plan proposed a seL4 + AOSP + Debian secure operating platform; its own pivot clause (§31.5 there) anticipated the embeddable-runtime shape of v1.2, so v1.3 is the execution of that clause rather than a change of direction. The OS, shells, VMM, hardware, team, budget and fundraising content of the August plan is **not** carried over; Appendix D maps every section to reused / adapted / dropped.

1. **Trust-boundary schemas** (§4.6 new): structured intent, validator rules and the propose → validate → policy → confirm → scoped capability → execute → audit sequence; capability and approval-receipt schema (§7.2); hash-chained, signed-checkpoint audit (§4.4).
2. **Security invariants and expanded threat model** (§7.1), with "model ≠ authority", the propose/dispose split and "stay useful when the model is gone" promoted to engineering principles (§3).
3. **Trust tiers** (§7.3 new): `dev` → `protected` → `secure`. The phone is the trusted confirmation device for node-prepared actions (§6.3). seL4 is a tier the same `no_std` trusted-side modules can target later, not a deliverable (ADR-002, Appendix C).
4. **Identity model** (§4.1, §6.3): device, user, agent and protocol-host identities with registered keys and revocation; grants, approvals and capabilities bind to identity.
5. **Memory data separation and disclosure checklist** (§4.3, §4.4): canonical / derived / ephemeral; derived indexes rebuildable; per-backend disclosure evaluation recorded in the trace.
6. **Connector and plugin model** (§4.7 new): integration hierarchy and signed manifest; never screen automation for high-risk operations.
7. **Verification** (§12): narrow formal methods — Kani on the policy, capability and approval state machines; a TLA+ model of the handoff/sync protocol checked under fault injection; Proptest on the capability lifecycle — plus required test layers and release channels.
8. **Roadmap and gates** (§11, §14): trusted-side modules and `pr3-vault` land in M1; phone-side confirmation and a model-checked handoff protocol in M2; reliability targets (zero unauthorized execution, zero replay acceptance) carried into every gate.

### v1.2 (19 October 2026) — engineering revision

v1.2 is an engineering revision over v1.1. Sections §9, §10, §14 and §15 were unchanged except for cross-references; in §2 only the comparison table changed (platform on-device models and ZeroClaw, items 4 and 9).

1. **Language decision closed.** Rust-first is confirmed and recorded as ADR-001 with an explicit trigger for revisiting it (Appendix C).
2. **`pr3-core` is `no_std + alloc` by construction** and sans-I/O: a `step()` state machine that consumes events and emits actions, with no async runtime, clock, I/O or threads of its own. `std` is an additive Cargo feature, never a separate code path (§4.2, §12).
3. **Reach model restated: "PR3 runtime where Rust runs; PR3 protocol everywhere else."** New `pr3-protocol` crate (the stable public contract) and `pr3-client-c` (C11, no dependencies) for consoles, Emscripten/Unreal HTML5, MCU/TEE and other non-Rust hosts (§4.2, §6.4–6.6).
4. **Inference has three backend classes:** platform models (Apple Foundation Models, Gemini Nano), bundled GGUF via llama.cpp, and the private node. The bundled model download becomes optional; M2 adds a platform-vs-bundled benchmark before committing to a mandatory download (§4.3, §8.1, §11).
5. **`pr3-llamacpp` reclassified** from "optional adapter" to M1's primary engineering risk, with a concrete binding strategy (§4.3, §11, §13).
6. **Consoles explicitly deferred** with a preserved, cheap path: `no_std` core, C client, and a one-page cognition-tiers RFC (§6.7, Appendix B).
7. **Mobile, desktop and hybrid confirmed as the product.** The hybrid's role on mobile is restated: the always-on node does the proactive work phones cannot do in the background (§6.1–6.3).
8. **Testing and CI rewritten for Rust:** `#![forbid(unsafe_code)]` enforced in CI, cargo-fuzz on every parser, Miri on core, a no_std check target, a panic-symbol check on the staticlib, and a pinned-emsdk job for the C client (§7.2, §12).
9. **ZeroClaw added as a comparable, not a base.** It enters the competitive landscape (§2.1), the benchmark program for tool-reliability and security scenarios (§8.1), and the reuse review at the adapter layer (§5.1). Building PR3 on ZeroClaw was evaluated and rejected (ADR-001, Appendix C).

---

## 1. Executive Summary

**PR3** is an open-source, privacy-first, portable personal AI runtime designed to work **independently and offline on mobile and desktop**, or **cooperatively in a private hybrid configuration**. It is intended as an embeddable intelligence engine rather than another cloud-dependent chat wrapper. The same core should be integrable into personal applications, OEM software, automotive infotainment, game engines, and constrained devices when their operating-system and model-runtime capabilities permit — and where they do not, those hosts speak PR3's protocol to a PR3 runtime that runs elsewhere.

PR3 is **inspired by OpenJarvis**, particularly its division of intelligence, inference engines, agent execution, memory, evaluations, and trace-driven learning. PR3 is not proposed as a literal line-for-line rewrite or an unmodified fork. It will implement a lean, independently maintained **Rust intelligence core**, integrated directly as a Cargo dependency in Rust/Tauri hosts, exported through an **optional C ABI** for Swift/Kotlin/Unreal/OEM applications, and reached from everything else through a **versioned wire protocol and a small C client**. It will reuse proven native inference infrastructure, use the operating system's on-device model where one exists, and benchmark openly against OpenJarvis.

**Research hypothesis:** with the same model, an agent running on PR3 executes *no unauthorized consequential action and leaks no data through tool arguments* under standard prompt-injection attacks (AgentDojo), where a plain tool-calling harness does not, while keeping benign-task utility close to that harness and adding cross-device continuity. **This is a target, not an established result.** PR3 claims it only when reproducible benchmarks show it under equal models, datasets, tools and hardware (§8).

**Domain applications** build on PR3 through its SDK, plugins and connectors; PR3 itself is application-neutral (§9).

### 1.1 Project boundaries

- **PR3 Core:** open-source engine, portable SDK, memory interfaces, model orchestration, tool contracts, policy enforcement, traces and evaluations.
- **PR3 Protocol & Client:** the versioned wire contract between PR3 runtimes and non-Rust hosts, plus a dependency-free C11 client library.
- **PR3 Apps:** open-source reference mobile/desktop clients — voice first, text optional — and the private-node coordinator (`pr3-node`).
- **Domain applications:** built outside this repository on PR3's SDK, plugins and connectors (§9).
- **Research/training:** separate development pipeline; the distributed mobile/desktop product **does not require Python, PyTorch, or cloud model APIs at runtime**.

### 1.2 Non-goals for the initial release

- Training a frontier foundation model from scratch.
- Claiming AGI or universal superiority over Qwen, OpenJarvis, or commercial frontier models.
- Including PyTorch/LibTorch in every mobile or embedded binary.
- Guaranteeing 24/7 autonomous operation on iOS or Android while the app is suspended.
- Letting an LLM directly access private keys, bypass permissions, or execute consequential actions without authorization.
- Reimplementing the full OpenJarvis feature catalog immediately.
- **In-process LLM inference on game consoles or inside a single Unreal HTML5 binary** (deferred; see §6.7 and Appendix B).
- **Owning the entire inference stack on devices whose OS already provides an on-device model.** PR3 bundles a model for coverage and control, not as the only path.
- **A full operating system, a seL4-hosted phone or desktop platform, custom shells or compositors, or dedicated hardware** — the long-term target of the August 2026 PR3/Samantha plan. seL4 survives only as a preserved deployment tier (§7.3, ADR-002).
- **A commercial product.** PR3 is an open-source research runtime; this roadmap plans no editions, pricing or go-to-market.
- **Acting as the user toward others.** PR3 may draft in the user's voice, but nothing is sent, said or signed in their name without confirmation or a pre-approved scope.
- **A cloud-hosted personal model, a photoreal avatar, or anyone's likeness** other than the user's own, by their choice.

---

## 2. Thesis & Positioning

Most personal-agent offerings emphasize capabilities, integrations, and cloud-based reasoning. Platform owners now ship free on-device models and OS agent hooks — Apple Foundation Models and App Intents, Gemini Nano and Android AppFunctions — each for its own platform. Fast-moving open-source assistants (OpenClaw, ZeroClaw) ship daemons with many channels and broad tool access. Competing head-on with either loses, and a few points of task accuracy over OpenJarvis is not a reason anyone adopts a runtime.

**Positioning:** *the layer underneath — an embeddable runtime that decides what an agent may do, owned by the user, portable across devices and operating systems.* The model is SQLite: embedded, in-process, portable, and boring in a way people trust. PR3 does not try to be the assistant.

- **Platform models are backends; OS hooks are connectors.** Apple Foundation Models and Gemini Nano sit behind the inference trait (§4.3); App Intents and AppFunctions are action surfaces behind typed connectors (§4.7). PR3 adds what no single platform will: one policy, memory and audit layer across all of them.
- **Cross-device is the user-visible feature.** The phone confirms what the desktop or node prepared (§6.3), and memory follows the user across Apple, Android, Windows and Linux — something platform owners structurally do not offer across each other's systems.
- **Security is the measurable claim.** The headline result is authorization under attack — no unauthorized consequential actions and no tool-argument exfiltration with the same model where plain harnesses fail (§8.1) — not a task-accuracy delta.
- **A private model of you, voice first.** You talk; PR3 answers in text or speech, remembers, drafts like you and learns your routine choices — on your own devices, never acting as you without your say (§4.5).

### 2.1 Landscape and related work

| Project | Relevant strengths | PR3 differentiating work to validate |
|---|---|---|
| [OpenJarvis](https://github.com/open-jarvis/OpenJarvis) | Local-first research platform; agent implementations; evaluation, energy/cost measurement; trace-driven learning; desktop app | Native mobile runtime, optional in-process inference, zero mandatory Python at runtime, private device handoff and repeatable comparison |
| [ZeroClaw](https://github.com/zeroclaw-labs/zeroclaw) | Single-binary Rust agent daemon; trait-based kernel ABI (`zeroclaw-api`); ~20 LLM providers over HTTP (Ollama for local); 30+ channels; supervised autonomy with OS sandboxes and cryptographic tool receipts; WASM plugin host; hardware control on Pi/ESP32; MIT/Apache dual license; large, active community (record dated star/contributor counts at each refresh) | Embeddable library rather than daemon; in-process inference rather than provider-over-HTTP; Android/iOS targets; `no_std` core free of Tokio; evaluation and trace-driven learning loop. ZeroClaw is the closest engineering comparable and a reuse source at the adapter layer (§5.1), not a base |
| [OpenClaw](https://github.com/openclaw/openclaw) | Broad personal assistant ecosystem, channels, tools, automation | Smaller embeddable SDK, device-local execution and model-agnostic benchmarked cognitive behavior |
| [NullClaw](https://github.com/nullclaw/nullclaw) | Small Zig runtime, pluggable providers, multiple channels, sandbox/security controls | Cross-device learning and personal-intelligence continuity, explicit native mobile/OEM SDK evaluation |
| [Hermes Agent](https://github.com/NousResearch/hermes-agent) | Skills, persistent execution and experience capture | Portable local-first runtime and objectively evaluated learning policy |
| Platform on-device models (Apple Foundation Models, Gemini Nano) | Free, zero-footprint, OS-integrated inference with guided generation/tool calling | Not competitors: PR3 treats them as backends. Differentiation is user-owned memory, policy, hybrid node and cross-platform continuity on top of them |
| OS agent hooks (Apple App Intents, Android AppFunctions) | System-wide action surfaces with deep OS integration, one platform each | Not competitors: PR3 uses them as connectors (§4.7) and adds cross-platform policy, user-owned audit and device-bound approval on top |
| [CaMeL](https://arxiv.org/abs/2503.18813) (research, 2025) | Prompt-injection defense by design: control flow comes from the trusted user query, so untrusted data cannot change it; capabilities enforce policy on tool calls. 77% of AgentDojo tasks solved with provable security, against 84% undefended | Nearest research neighbour (§2.3). PR3 makes the idea a portable runtime with device-bound approvals and a formally checked policy core |
| Mobile on-device AI apps | Native mobile inference and conversation | Durable agent state, privacy governance, tested hybrid inference and proactive domain workflows |

The comparison is a research baseline; feature coverage, upstream releases and measured performance must be refreshed at each PR3 milestone.

### 2.2 Differentiators requiring proof

1. **Portable binary integration:** stable C ABI, configurable allocator, optional feature modules; build against multiple OS and OEM toolchains — and a protocol client for toolchains the runtime cannot target.
2. **True local autonomy:** on-device model plus agent, tool calls, local memory and policy checks, with offline-mode test evidence.
3. **Private hybrid and cross-device continuity:** authenticated task delegation to a user-controlled desktop/private node, minimizing data transfer; the phone confirms what other devices prepare; memory follows the user across operating systems; fully functional local fallback for supported workflows.
4. **Outcome-based intelligence:** trace evaluation, adaptive selection of models/tools/workflows and regression-tested policy promotion.
5. **Verifiable privacy:** identifiable data movement, deny-by-default network/tool policies, independent cryptographic key ownership and auditable approval flows.
6. **Domain extensibility:** PR3 is application-neutral; domain applications are extensions built on it (§9), never dependencies of the core.

### 2.3 Research positioning

The nearest research neighbour is **CaMeL** ("Defeating Prompt Injections by Design", Debenedetti et al., 2025). Like PR3, it separates what the model proposes from what is allowed to run: control flow is derived from the trusted user request, so untrusted data retrieved later cannot change which tools run, and capabilities stop private data from leaving through unauthorized flows. Evaluated on AgentDojo, it solves 77% of tasks with provable security, against 84% for an undefended system.

CaMeL is a research system. PR3's contribution is to make the propose/dispose split a portable runtime:

- structured intents with runtime-stamped envelopes, so the same core runs in apps, games, browsers and embedded hosts (§4.2, §4.6);
- approvals bound to the user's own devices — phone-side confirmation and vault-signed, one-use capabilities (§6.3, §7.2);
- a policy core checked with Kani and a handoff protocol checked in TLA+ (§12);
- evaluation on the same suite, AgentDojo, so results compare directly with CaMeL's published numbers (§8.1).

---

## 3. Guiding Engineering Principles

- **Less is more:** every mandatory dependency, abstraction and background service must justify its cost.
- **Rust-first core:** stable Rust toolchain pinned per release, plus a separately pinned nightly used only by the verification jobs that require it (Miri, cargo-fuzz, sanitizers); safe Rust by default; `unsafe` confined to two audited crates (`pr3-llamacpp`, `pr3-ffi`) and to a single `sys` module in each of `pr3-platform-llm`, `pr3-vault` and `pr3-speech` for platform FFI, enforced by CI (§4.2).
- **Sans-I/O core:** `pr3-core` is a pure state machine — `step(input) -> actions`. No async runtime, no clock, no filesystem, no sockets, no threads inside it. Adapters drive it. This one property yields the Wasm target, the console path and the C ABI's polling model for free.
- **`no_std + alloc` always, `std` additive:** the core and every interface crate compile without `std`; the `std` feature only adds convenience impls, never behavior or types.
- **Runtime where Rust runs, protocol everywhere else:** do not let the lowest-common-denominator target dictate the core language. Hosts that cannot run the Rust runtime (console SDKs, a game's Emscripten build, MCUs, TEEs) use `pr3-client-c` over `pr3-protocol`.
- **Direct Rust SDK first, C ABI second:** an idiomatic Cargo crate for Tauri and other Rust apps; a stable, versioned C ABI (and UniFFI bindings for Swift/Kotlin) as an optional distribution target. No Rust types, panics or unwinding may cross the C ABI.
- **Platform model where it exists, bundled model where it doesn't:** inference is model-neutral; Apple Foundation Models and Gemini Nano are first-class backends alongside bundled GGUF and the private node. The bundled download is optional.
- **Deterministic first, LLM second:** on battery-constrained devices the LLM runs only after deterministic code has decided something is worth explaining; it is never the first filter.
- **Host-owned resources:** host hooks provide allocator, monotonic clock, entropy, storage, networking, scheduling and device APIs.
- **Core/adapter separation:** no direct Apple, Android, Windows, Linux, SQLite or model-specific headers inside the minimal core.
- **No blocking host loop:** asynchronous/pollable state machine; never block the UI or game render thread for model inference.
- **Privacy defaults:** inference stays local unless the user permits connection to a specific private node; external providers are strictly opt-in and not necessary for basic operation.
- **Model ≠ authority:** every model output — local, platform or node — is an untrusted proposal. Tool responses, emails, web pages, retrieved memory, protocol frames and model outputs are all untrusted input; every consequential action passes through deterministic authorization, and the policy engine never delegates its final decision to an LLM.
- **Propose/dispose split inside the runtime:** the untrusted side (model, retrieved context, tool outputs, UI) proposes; the trusted side (intent validation, policy, capability issuance, vault access, audit, confirmation) disposes. On today's OSes both sides share a process; the boundary is kept explicit so the trusted side can later move behind platform-protected storage or into a seL4 protection domain without changing semantics (§7.3).
- **Stay useful when the model is gone:** PR3 must keep working when the provider is unavailable, the user is offline, a subscription lapses, a vendor changes terms, or the AI layer is disabled. Deterministic engines and local memory do not depend on inference.
- **Deterministic consequential values:** amounts, limits, recipients and any other value a consequential action depends on are computed or resolved by validated non-LLM code.
- **Evidence before claims:** collect latency, energy, accuracy, security failures and task outcomes continuously.

---

## 4. Reference Architecture

```text
   +------------------+   +------------------+   +--------------------------+
   | Mobile app       |   | Desktop app      |   | Non-Rust hosts           |
   | Android / iOS    |   | macOS/Win/Linux  |   | consoles, UE HTML5,      |
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
        (opaque wake-ups only; UnifiedPush on Android; §6.3)
```

**Logical distinction:** A task can be *executed remotely* without synchronizing all personal memories; and memories may be *synchronized* without granting remote agents authority to execute actions. These must be separate protocols and permission scopes. A protocol client has *execution* access only to the scopes its host was granted.

### 4.1 Functional modules

| Module | Crate (§4.2) | Responsibilities | Mandatory? |
|---|---|---|---|
| `core` | `pr3-core` | Context/session ownership, state transitions, events, cancel/retry | Yes |
| `protocol` | `pr3-protocol` | Versioned wire schema for requests, events, handoff and sync | Yes |
| `model_api` | `pr3-inference` | Inference contract; model capabilities, token and context metadata | Interface yes; implementation no |
| `tool_api` | `pr3-agent` | Versioned schema, permission check, asynchronous result | Yes |
| `policy` | `pr3-core` (trusted side) | Capability checks, tool allowlists, approval receipts, risk classes; deterministic, never LLM-decided | Yes |
| `intent` | `pr3-core` (trusted side) | Structured-intent schema and validator: runtime-stamped envelope (identity, nonce, time), unknown-field rejection, bounds on model-chosen values, freshness, replay prevention, reference resolution without exposing secrets, minimum data to policy (§4.6) | Yes |
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
| `app_connectors` | application plugins (§4.7) | Email, calendar, files, payments, OEM SDK | Optional and separately permissioned |

### 4.2 Rust workspace, host API, C ABI and protocol client

**Canonical API:** strongly typed Rust traits and events. Rust and Tauri hosts import `pr3` via Cargo and call it directly; there is **no FFI boundary between a Rust host and the PR3 Rust core**.

Workspace (logical organization; consolidate crates if they do not justify the maintenance cost):

```text
pr3/
├── Cargo.toml                 # workspace, features, pinned toolchain
├── rust-toolchain.toml
├── crates/
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
├── apps/pr3-node/             # desktop private inference and handoff service
├── examples/tauri/            # Tauri 2 host integration
├── examples/browser/          # Wasm/JS example, mock and remote inference
├── examples/unreal-client/    # Unreal C++ host using pr3-client-c (native + HTML5)
└── benches/                   # matched OpenJarvis/ZeroClaw benchmarks; benchmark-only HTTP backend
```

**Crate modes.** The rule is `no_std + alloc` always, `std` additive. Any crate listed as `std only` is an adapter and may not be depended on by a `no_std` crate.

| Crate | Mode | `unsafe` |
|---|---|---|
| `pr3-core`, `pr3-agent`, `pr3-protocol`, `pr3-inference` (traits), `pr3-memory` (traits) | `no_std + alloc`; `std` feature additive | forbidden |
| `pr3-ffi` **full** build | `std` staticlib/cdylib with UniFFI and every adapter (bundled GGUF, SQLite, vault, hybrid); `panic = "unwind"`, unwinds caught at every exported function. Mobile, desktop and native-game hosts | permitted, audited |
| `pr3-ffi` **core** build | `no_std + alloc` staticlib: core and trusted-side modules only, inference/storage/network supplied by host callbacks; `panic = "abort"`. Appendix B hosts | permitted, audited |
| `pr3-llamacpp` | `std` only | permitted, audited |
| `pr3-platform-llm`, `pr3-vault`, `pr3-speech` | `std` only | `#![deny(unsafe_code)]`; permitted only in one audited `sys` module for platform FFI |
| `pr3-hybrid`, `pr3-memory-sqlite`, `pr3-learning`, `pr3-node` | `std` only | forbidden |
| `pr3-web` | either; `no_std` keeps Wasm small | forbidden |
| `pr3-client-c` | C11, freestanding-capable | n/a (C) |

**Two `pr3-ffi` builds, two profiles.** Cargo does not allow `panic` to be set per package, and UniFFI and the `std` adapters cannot link into a `no_std` artifact, so the full and core builds are separate invocations with their own profiles (`release` with unwind; `release-core` with abort).

**Platform FFI.** Apple Foundation Models is Swift-only, Gemini Nano is reached through Kotlin, and Android Keystore and Windows CNG/TPM are JNI and Win32 calls — all of which need `unsafe` when called directly. Prefer paths that keep the call safe on the Rust side: Tauri 2's mobile-plugin bridge in Tauri hosts, and host-implemented UniFFI callback interfaces (generated in `pr3-ffi`) in native hosts. Direct calls that remain go in the crate's `sys` module, under the same sanitizer and review rules as the other audited crates. `pr3-speech` follows the same rule; its whisper.cpp fallback reuses the `pr3-llamacpp` pattern (pinned submodule, frozen C shim, prebuilt libraries) inside that module.

**Trusted-side modules stay separable.** Inside `pr3-core`, `intent`, `policy`, `capability`, `audit` and `identity` are modules with no dependency on the agent loop or on inference types. They can be split into their own crates, moved behind a platform-protected boundary, or compiled into seL4 protection domains via `rust-sel4` later without an API change (§7.3). CI enforces the dependency direction: agent and inference modules may depend on trusted-side types; the reverse is a build error.

**`pr3-core` constraints (enforced, not aspirational):**

```toml
# crates/pr3-core/Cargo.toml
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
- **CI builds the `no_std` configuration on a real target** — `cargo check -p pr3-core --no-default-features --target thumbv7em-none-eabihf` (no `std` exists there, so leaks fail loudly) and `--target wasm32-unknown-unknown`. Cargo unifies features across a workspace, so a root `cargo check` silently enables `std` for every crate; the `no_std` check must be its own command.

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

Specify ABI version negotiation, opaque ownership, buffer lengths, thread safety, callbacks, allocators, asynchronous polling and panic containment (panic-free trusted-side code is the primary defense; the full build catches unwinds at every exported function and returns `PR3_ERROR`; the core build aborts, so a panic there takes the host down — which is why its panic sites are allowlisted). Use `cbindgen` for the header and UniFFI for idiomatic Swift/Kotlin. **The C ABI does not make Rust compile for closed proprietary console toolchains**; that is what the protocol client is for.

**`pr3-client-c` for hosts that cannot link the Rust runtime:**

- C11, zero dependencies, freestanding-capable: no `malloc`, no sockets, no threads, no `stdio` of its own. The host supplies allocator, `send`/`recv`, monotonic clock and entropy callbacks — the same host-owned-resources principle as the core.
- Implements `pr3-protocol` framing (CBOR; FlatBuffers to be evaluated for C++ game teams), request/event queues, reconnection state and approval-receipt verification. It never interprets tool results or holds secrets.
- Builds in CI as: Linux x86_64/ARM64 from M0; from M4, a "console-style" freestanding build with stubbed callbacks and an Emscripten build using the emsdk pinned by the Unreal HTML5 fork recorded in M4-E3.
- Fuzzed (libFuzzer/AFL on the frame parser), ASan/UBSan, MISRA-ish lints where cheap. Console and OEM partners compile it with their own SDKs without touching the open core.

### 4.3 Inference strategy

Three backend classes behind one trait. Routing is by **capability** (context, tool-calling, guided output), **privacy class** of the request, and **device profile**:

| Backend | Where | Role | Notes |
|---|---|---|---|
| **Platform model** | Apple Foundation Models (iOS/iPadOS/macOS 26+ on Apple Intelligence devices); Gemini Nano via ML Kit GenAI / Prompt API on supported Android devices | Default local backend where available: zero download, OS-maintained, ~3B class, guided generation and tool calling | Black box; no custom weights; device and region gaps; model may not be downloaded yet. ML Kit's GenAI APIs may send usage metrics to Google — evaluate against PR3's privacy claims before enabling by default |
| **Bundled GGUF** (`pr3-llamacpp`) | Any device that passes a memory/thermal profile; all desktops | Coverage where no platform model exists; control over weights, templates and tool-calling format; reproducible benchmarks | Optional download, Wi-Fi only, app usable before it lands. Mobile: CPU backend first, Metal on iOS, Vulkan on Android as a measured per-device-class upgrade. Desktop: CPU/Metal/Vulkan/CUDA as available |
| **Private node** | User's paired desktop or self-hosted `pr3-node`, 8B–32B+ | Escalation for long reasoning, long context, and all proactive/background work | Explicit, authorized TLS endpoint; receives no more context than the task needs |

- **Mobile:** the local model is load-bearing, not an accelerator. Most users will not run a node, so without local inference the choice collapses to "cloud or nothing." Deterministic engines filter first; the LLM explains, summarizes and answers retrieval-backed questions.
- **Desktop:** `llama.cpp` as the portable default; optional MLX (Apple), ONNX Runtime or ExecuTorch where supported and advantageous.
- **Research / export:** PyTorch and tools such as the ExecuTorch exporter remain possible *outside* production runtime.
- **No model assumption:** models, chat templates, tokenizer and tool-calling formats are versioned per model. Never assume all GGUFs implement reliable structured tool calls. Platform models declare capabilities through the same `ModelCapabilities` struct.
- **Default model family: Qwen3**, pinned at M0 (re-check then for a newer generation). Phone: 1.7B (mid-range) or 4B (flagship), 4-bit quantized, with 0.6B as a routing and classification helper. Node: 30B-A3B mixture-of-experts (about 3B active parameters, fast enough for spoken replies) or 14B/32B dense. One family on phone and node keeps the chat template and tool-call format identical; non-thinking mode serves fast replies, thinking mode harder node tasks. The M2 benchmark still decides per platform between Qwen and the platform model.

**Disclosure policy — routing by privacy class.** Before any context leaves the deterministic layer for a backend, the router evaluates and records in the trace: which backend is selected; whether it is approved for this data class; whether local inference is required; what redaction applies; the minimum context needed; whether the backend may retain data; and whether user or deployment policy forbids sending it off-device. Platform-model vendors' retention and telemetry behavior are inputs to this table and are measured before a backend is enabled by default. The learning policy may change *which approved* backend is chosen; it may never change the approval itself (§4.5).

**Speech, voice first.** Speech is a pipeline: on-device speech-to-text → the selected model → text-to-speech, each behind a `pr3-speech` trait so backends swap the way models do. Platform engines come first (the on-device speech recognizers and voices of Apple and Android); whisper.cpp is the fallback where no on-device recognizer exists, and a local neural voice (such as Piper) a later option. Voice notes — speak, get a written reply — work everywhere and come first; low-latency spoken conversation targets the node. End-to-end speech models (Qwen-Omni class) are a node experiment only, because today they run mainly on Python stacks the runtime does not ship. Push-to-talk comes before any always-listening wake word, and raw audio is kept only if the user chooses.

**`pr3-llamacpp` binding strategy (M1's primary engineering risk).** Spike status: items 1–2 are replaced by pinned `llama-cpp-2` bindings behind PR3's own frozen Rust API, and item 3's prebuilts are deferred (ADR-003). The target design:

1. Vendor llama.cpp as a git submodule pinned to a reviewed commit; upgrade on a cadence with a conformance run, never ad hoc.
2. Write a **tiny C shim with a frozen API** over the subset PR3 uses (load, tokenize, decode step, sampling hooks, KV-cache control, release). Bindgen the shim, not llama.cpp. Upstream API churn is absorbed in the shim.
3. Build per-target static libraries of llama.cpp + shim in CI (macOS arm64/x86_64, Linux, Windows, Android arm64 via NDK, iOS arm64 + simulator) and publish them as checksummed release artifacts. An explicit fetch step (`cargo xtask fetch-llama`, or the host's CI cache) pins URL and SHA-256 and places them in a configured directory; `build.rs` only links what is there, never downloads, skips linking on docs.rs, and rebuilds from source only with a feature flag. This keeps CMake out of every downstream `cargo build` while keeping `--offline` and reproducible builds working; bundling the libraries into the published crate is ruled out by crates.io's 10 MB crate limit.
4. Cargo features map 1:1 to ggml backends (`cpu`, `metal`, `vulkan`, `cuda`); CPU is always compiled in as the fallback.
5. All `unsafe` lives in this crate; the public API is safe Rust with explicit `Send`/`Sync` story and cancellation. Fuzz the shim boundary; run under ASan/UBSan in CI.

### 4.4 Memory and knowledge architecture

Use **SQLite** as the first persistent engine, with FTS5 for lexical retrieval. Add small local embedding models (candidate: Qwen3-Embedding 0.6B) and vector search only after demonstrating gains on held-out tasks. Avoid heavyweight standalone vector DBs in the default distribution.

Suggested tables: `sessions`, `messages`, `facts`, `sources`, `goals`, `task_events`, `tool_calls`, `policies`, `approvals`, `memory_tombstones`, `evaluations`, `device_sync_log`.

Each important memory record must include: `source_id`, `created_at`, `valid_at`, `confidence`, `privacy_class`, `principal_id`, `consent_scope`, `retention_policy`, and deletion status. Memory extraction is a proposal; user-provided corrections supersede extracted inferences. Conversations — voice transcripts and text — are a memory source like any other: extracted facts and preferences are proposals the user can confirm or correct, preferences fade unless reinforced, and other people's content stays tagged with its principal and is never disclosed to third parties. Treat retrieved documents as data, never as instructions.

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

A model never calls a consequential tool directly. It emits a **structured intent** — a typed proposal in which the model chooses only enumerated or bounded values and refers to everything else by reference (here a contact, a draft and a file). It never supplies secrets, raw addresses, identities or replay-protection values:

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

The runtime — not the model — wraps the proposal in an **envelope** before validation. Identity comes from the session's binding (§6.3), freshness and replay values from host-supplied clock and entropy:

```text
Envelope (set by the runtime):
  actor:          agent:mail-assistant      session binding
  principal:      user:…                    session binding
  request_nonce:  01JPR3…                   host entropy
  received_at:    2026-10-19T06:00:00Z      host clock
  proposal:       { …model output above… }
```

The **intent validator** (trusted side, `no_std`) rejects unknown fields on security-sensitive intents — including any identity, nonce or timestamp field the model tries to supply, so a prompt-injected model cannot pick its own replay values or claim another principal; checks every model-chosen value against its enumeration or bound (a known `intent_type`, no more than the allowed number of recipients and attachments); resolves every `*_ref` against canonical data — a contact the user already has, a draft owned by this principal, a file inside the granted scope — without handing the model raw values or secrets; checks the envelope's freshness and nonce against a replay window; normalizes into an internal type; and forwards only the minimum fields the policy decision needs. The deterministic engine — not the model — resolves the actual addresses, attachment hashes and sizes, so a prompt-injected model cannot type an attacker's address or attach a file outside its scope.

**Execution sequence** (the same shape for mail, calendar, payments and file sharing; mail shown):

1. **Model → runtime → intent validator:** `mail.send` as above, wrapped in the runtime's envelope.
2. **Validator → deterministic checks:** resolved recipients (known contacts, none added in this session), attachment scope, size and data class (§4.3 disclosure policy), rate limits.
3. **Checks → policy:** normalized action with its risk class (an external recipient plus an attachment is high risk).
4. **Policy → decision:** deny; allow (low risk, within a pre-authorized scope); or **confirmation required**.
5. **Confirmation (§6.3):** PR3's own UI on the phone renders the exact recipient addresses, subject, attachment names, sizes and hashes, and expiry; the user approves with OS biometrics; `pr3-vault` signs the approval receipt.
6. **Vault → execution adapter:** a one-use, time-boxed capability bound to the receipt — never the mail provider's OAuth token (§7.2 item 5).
7. **Execution adapter** (sandbox connector or the user's mail provider) sends once; the idempotency key prevents a second send on retry or reconnect. For live providers it runs wherever the provider credential lives — on the phone, not the node (§6.3).
8. **Audit:** intent, decision, receipt, capability and result are appended to the hash chain; the user receives a completion receipt that links them.

No step gives the model credentials, raw secrets, or a way to widen its own scope. "No action" is a valid outcome at steps 2–5 and is audited like any other.

### 4.7 Connector and plugin model

Connectors expose **typed operations**, never uncontrolled screen automation. Preferred integration order:

1. A stable application or provider API.
2. OS-level agent integration (Apple App Intents, Android AppFunctions) where available — used, never made the core protocol.
3. An approved MCP-style connector with a reviewed manifest.
4. Accessibility/UI automation only as a low-risk fallback.
5. **Never** screen automation for payment, identity or credential operations.

Every connector and plugin ships a **signed manifest** the policy engine reads: declared permissions (capability grants, §7.2); data classes it may read and emit; operations with their reversibility level; confirmation rules per operation; rate limits; audit semantics; version and publisher identity. Manifests are data: a plugin cannot widen its own grants at runtime, and neither can the learning policy (§4.5). Arbitrary code is never imported by default; a WASI-component sandbox for third-party plugins is the candidate mechanism when the ecosystem opens (compare ZeroClaw's plugin host, §5.1).

---

## 5. OpenJarvis-Inspired Implementation Mapping

The current upstream OpenJarvis contains Python modules for memory, training and learning orchestration, as well as multiple Rust crates for agents, engines, skills, traces, learning and security. The inspected Rust llama.cpp adapter calls a **local HTTP `/completion` server**; PR3 links the model **in process** on mobile and desktop. A Rust ReAct implementation extracts `Action` and `Action Input` with regex; PR3 prefers provider-supported structured tool calling, strict JSON schema validation and deterministic permission checks.

| OpenJarvis component / source area | Why it matters | PR3 approach | Priority |
|---|---|---|---|
| `rust/crates/openjarvis-core` | Types/events/registry | Minimal `no_std` Rust traits/events; C ABI and protocol for non-Rust hosts | P0 |
| `rust/crates/openjarvis-agents` | ReAct/monitor/orchestrator | Single bounded sans-I/O state machine, structured actions, cancellation | P0 |
| `rust/crates/openjarvis-engine` | Provider abstraction, llama.cpp HTTP bridge | In-process local inference, platform-model backends, authenticated private-node adapter | P0 |
| `src/openjarvis/memory` | Provenance, extracted facts, persistence | SQLite provenance, trust tiers, policy-aware retrieval and deletion | P0–P1 |
| `rust/crates/openjarvis-learning` | Trace policy/bandit/reward | Native adaptive policy with offline evaluation and rollback | P1 |
| `src/openjarvis/learning` | Data extraction, optimization, LoRA | Trace-driven policy evaluation in Rust; neural research outside runtime | P2 |
| `agents/monitor_operative`, scheduler | Persistent monitoring | Durable scheduled task/events on `pr3-node` for 24/7 jobs | P1 |
| OpenJarvis benchmarking | Accuracy/cost/energy emphasis | Reproducible shared harness with matched baselines | P0 |
| Skills and tool ecosystem | Reusable capabilities | Minimal plugin manifest + permission schema; don't import arbitrary code by default | P1 |

**Attribution:** any reused or modified OpenJarvis file keeps its notices, and the README credits OpenJarvis (§15).

Useful source references:

- [OpenJarvis repository](https://github.com/open-jarvis/OpenJarvis)
- [OpenJarvis README](https://github.com/open-jarvis/OpenJarvis/blob/main/README.md)
- [Rust llama.cpp adapter](https://github.com/open-jarvis/OpenJarvis/blob/main/rust/crates/openjarvis-engine/src/llamacpp.rs)
- [Rust ReAct agent](https://github.com/open-jarvis/OpenJarvis/blob/main/rust/crates/openjarvis-agents/src/native_react.rs)
- [Rust trace-driven policy](https://github.com/open-jarvis/OpenJarvis/blob/main/rust/crates/openjarvis-learning/src/trace_policy.rs)
- [Python LearningOrchestrator](https://github.com/open-jarvis/OpenJarvis/blob/main/src/openjarvis/learning/learning_orchestrator.py)
- [Python memory store](https://github.com/open-jarvis/OpenJarvis/blob/main/src/openjarvis/memory/store.py)
- [Karpathy llm.c](https://github.com/karpathy/llm.c) — philosophical reference for readable code and cost of complexity; *not* an agent runtime.

### 5.1 ZeroClaw: reuse candidates at the adapter layer

ZeroClaw is the nearest engineering comparable to PR3 (Rust, trait-based kernel ABI, approval-gated autonomy, tool receipts, SQLite memory, WASM plugins). Its *shape* is the opposite of PR3's — a Tokio-bound daemon whose runtime depends on every other crate, inference only through HTTP providers, no mobile target — so it is not a base (ADR-001). Specific pieces are worth evaluating for reuse under its MIT/Apache dual license, with notices retained:

| ZeroClaw component | Why it matters | PR3 approach | When |
|---|---|---|---|
| `zeroclaw-tool-call-parser` | Normalizes OpenAI-style, Anthropic-style and Qwen/Ollama tool-call formats, including streaming deltas | Candidate for output normalization in `pr3-inference` adapters; directly addresses the §13 "different tool-calling behavior by model" risk | Evaluate before M1-E2 |
| Tool-receipt format and security policy types | Field-tested design for binding an approval to an action | Compare against PR3 approval receipts (§7.2) before finalizing the schema; adopt field names where equivalent | M1 |
| `zeroclaw-api` trait surface | Shows which abstractions a large Rust agent community settled on | Reference when naming PR3 traits; no code dependency | M0 |

Rules: nothing from ZeroClaw enters a `no_std` crate; PR3 never depends on `zeroclaw-runtime`; each candidate is checked with `cargo tree` for Tokio/`reqwest` pull-in before adoption. Reuse is a per-component decision made on evidence, not a direction.

---

## 6. Supported Deployment Modes

### 6.1 Fully offline mobile

Local model (platform or bundled), local database, local skills/tools and policy enforcement. No cloud LLM. Must support airplane-mode tests for document Q&A, local reminders, summaries of cached data and other permitted tasks. Email sync, web content and external APIs cannot update without connectivity.

Mobile-specific rules: the app is useful before any model download completes; the bundled download is Wi-Fi-only and resumable; the deterministic engine runs before the LLM so battery is spent only on explanations worth giving; proactive/background behavior is **not** promised on the phone (see §6.3).

### 6.2 Fully offline desktop

Same Rust core with a larger local model, longer contexts and optional heavier retrieval/learning features. Local model loading, agent actions and memory must not require an always-running HTTP server. The desktop may additionally run `pr3-node` for other devices.

### 6.3 Private hybrid: mobile + desktop

Mobile performs routine inference locally, delegates selected difficult tasks after a data-transfer policy check, and resumes locally when the node is unavailable. The node executes longer-running workflows and returns provenance-tagged results.

**The hybrid matters more on mobile than anywhere else:** iOS and Android do not allow sustained background inference, so the proactive part of PR3 — watching, summarizing, deciding whether to alert — runs on the always-on node, and the phone wakes on push to render and act. Without a node, proactive features are limited to what OS-scheduled background windows allow, and the product must say so.

**Push needs a publisher-operated relay.** APNs and FCM accept only messages authenticated with the app publisher's credentials, so a self-hosted node cannot wake the phone directly without holding the publisher's key. The hybrid therefore uses a **push relay** run by the app publisher (the pattern Matrix uses with its Sygnal push gateway) or, on Android, UnifiedPush to a distributor the user chooses. The relay carries only an opaque wake-up — a device token and a payload sealed to the phone — never task content. It is a hosted dependency of proactive features only; offline and node-less modes do not use it. It appears in the threat model (§7.1).

Use authenticated pairing, certificate/key pinning or mutually authenticated TLS, replay protection, short-lived authorizations and explicit remote cancellation. Zero-knowledge E2EE cannot coexist with the remote node reading plaintext task data; the node must be trusted by the user and authorized to process that subset.

**Identity.** Every device, user, agent and protocol host has an identity with registered public keys, session binding and a revocation state. Grants, approvals and capabilities bind to these identities, so revoking a device, a plugin or a game host invalidates everything issued to it without touching the rest.

**The phone is the trusted confirmation device.** Actions prepared on the node — an outbound message, a calendar change, a payment — are confirmed on the phone: PR3's own UI renders the exact recipients, attachments, amounts and expiry from the *validated intent*, not from anything the node sent for display; the user approves with OS biometrics; the approval receipt is signed with a key `pr3-vault` keeps in the platform keystore. The node never holds an approval key, so a compromised node can prepare but not approve.

That guarantee holds only if the node also cannot *execute* without the phone. Third-party providers — mail, calendar, payment services — check their own OAuth tokens or API keys, not PR3 receipts; a node holding such a token could act with no approval at all. For live execution, the provider credential therefore stays in the phone's platform keystore and the execution adapter runs on the phone, released by the one-use capability — unless a provider itself verifies PR3 receipts. Sandbox connectors are unaffected. Which arrangement applies to each live provider is recorded at the M5 gate together with the `protected`-tier decision (§7.3).

Phone-side confirmation is the earliest practical answer to the trusted-path problem — a compromised display showing one action while requesting another — and it costs nothing beyond what the hybrid already builds.

### 6.4 OEM / Embedded

Two deliverables, chosen by what the target can run:

- **Rust runtime** for embedded Linux and other Tier 2 Rust targets (Ferrocene where a qualified toolchain is required; QNX where its Rust support applies). Host supplies storage, scheduling and inference; compile-time features remove unnecessary dependencies.
- **`pr3-client-c`** for MCU-class devices, secure elements and TEEs. Nothing LLM-shaped runs there; the TEE's job is key custody and approval-receipt verification, which PR3 deliberately keeps outside itself.

This is **not** a claim that a full LLM can run on every device. Automotive safety-critical controls remain outside LLM authority.

### 6.5 What ships where

| Target | Ships | Inference | Important constraint |
|---|---|---|---|
| Android | Rust runtime via Tauri 2 (Cargo) or Kotlin host (UniFFI / C ABI) | Gemini Nano where available; bundled GGUF CPU-first, Vulkan measured; node | Background execution/power limits; device zoo; Prompt API availability |
| iOS / iPadOS | Rust runtime via Tauri 2 or SwiftUI host (UniFFI / C ABI) | Apple Foundation Models on Apple Intelligence devices; bundled GGUF Metal; node | Sandbox/background restrictions; model-download and signing rules |
| macOS | Rust runtime + optional `pr3-node` | Foundation Models; GGUF Metal/MLX | Apple codesigning / distribution |
| Windows / Linux | Rust runtime + `pr3-node` | GGUF CPU/Vulkan/CUDA | Driver availability; OS sandboxing |
| Embedded Linux / OEM | Rust runtime (Tier 2 targets; Ferrocene if qualified) | Small GGUF CPU, or node | Memory budget; vendor BSPs → host callbacks |
| MCU / secure element / TEE | `pr3-client-c` only | Node | Keys live here; PR3 does not |
| Native games — desktop/mobile | Rust runtime as staticlib via the `pr3-ffi` full build, in-process | GGUF or node | Frame budget; host-declared memory/thread budget |
| Consoles | `pr3-client-c` only (initial). `pr3-ffi` core build via custom target when a partner funds it | Node (console tier C2). In-process deferred | Closed SDKs, platform certification, no public GPU backend; see §6.7, Appendix B |
| Web / Unreal HTML5 | `pr3-web` as a separate Wasm module + JS bridge, or `pr3-client-c` compiled by the host's own emsdk | Sibling WebGPU engine (desktop browsers) or node | Single Rust/Emscripten binary with Unreal is out of scope; see §6.6 |

### 6.6 Browser Wasm and Unreal HTML5 integration

**Browser-first scope:** compile agent state, policy, schemas, trace processing and memory *interfaces* to `wasm32-unknown-unknown` (`no_std` keeps the module small). Use JavaScript/browser host bindings for network (`fetch`/WebSocket), persistence (IndexedDB/OPFS through a compatible adapter), timing and task scheduling. Native Tokio/SQLite/llama.cpp adapters are never dependencies of `pr3-web`.

**In-browser inference is a separate proof-of-concept:** the Rust/Wasm agent core delegates to an independently packaged WebGPU inference engine in the page, or to an approved private node. Model memory, WebGPU availability and frame impact must be measured; mobile browsers are node-only.

**Unreal HTML5 — supported path:** `pr3-client-c` compiled by the Unreal fork's *own* pinned emsdk and linked into the game's binary, speaking the protocol over WebSocket to a node. CI carries a job on that exact emsdk version. A separate Rust `.wasm` module with a JS message bridge is the alternative when the client's footprint matters less than keeping Rust in the loop.

**Out of scope:** a single Rust-plus-Unreal Emscripten binary via `wasm32-unknown-emscripten`. LLVM object-format mismatches and pthreads/Asyncify flag conflicts make it fragile regardless of language unless everything is compiled by the same emsdk — which is exactly what the C client achieves.

**WASI:** treat `wasm32-wasip2` as a distinct target with host-supplied capabilities; browser WebGPU and native filesystem behavior do not carry over automatically.

**Optional digital face, later.** A stylized face — falling glyphs that form a head, not a photoreal human — rendered with WebGPU in the Tauri webview, with a WebGL2 fallback where a webview lacks WebGPU. A hidden low-poly head driven by mouth-shape weights sets the glyphs' brightness, so the face speaks in sync with the audio; listening, thinking and speaking map to rain speed and glow. Mouth shapes come from the node's audio-driven model when available and from text-to-speech phoneme timing offline, carried as presence events in `pr3-protocol`. The face only displays: approvals stay on the phone (§6.3).

### 6.7 Game consoles — deferred, path preserved

In-process console inference is technically feasible (the hardware exceeds a flagship phone) but is **not** a 2026 priority: retail SDKs expose no GPU inference backend, so a CPU-only model caps at ~1–1.5B and the server stays mandatory anyway; the dominant costs are SDK access, platform certification and platform relations rather than code; the generation is late-cycle; and in-game AI is publisher-owned by construction, which is off PR3's user-owned thesis.

What PR3 does now, at a cost of roughly a page and one CI job: keep `pr3-core` `no_std`-clean, ship `pr3-client-c`, and maintain Appendix B as the design a studio partner would react to. Order of attack when a partner appears: Xbox (GDK is closest to a stock Rust/Win32 target) → PS5 → Switch 2 (ARM, tightest budget).

---

## 7. Security, Privacy & Trust Model

### 7.1 Threat model

Assume any of the following may be compromised or hostile: the model (local, platform or node) and its provider; prompts, documents, emails, web pages and audio heard by the microphone; retrieved memory; tool and connector outputs; a plugin; a protocol-client host (a game, an MCU); the paired node; the publisher's push relay (§6.3); the host OS or kernel; the network; peripheral firmware; a generated script; and the app's own UI layer when spoofing is attempted.

Protect against: prompt injection through any of those inputs, exfiltration through tool arguments, replayed or forged approvals, impersonation through a stolen personal model or voice, leaked OAuth tokens, insecure pairing, malformed protocol frames, model hallucination presented as fact, logging of personal data, corrupted or poisoned local memory, UI spoofing of a confirmation, and unauthorized message, payment or other consequential execution.

**Invariants the design must preserve** (release gates, tested adversarially — §12):

- Private keys cannot be exported from the vault; the model never sees a credential.
- High-risk actions cannot bypass policy; the final authorization decision is never made by an LLM.
- Audit history cannot be silently rewritten.
- A model, plugin or learning policy cannot grant itself additional authority.
- A compromised plugin or host cannot inherit another's capabilities.
- Provider and connector tokens are scoped, short-lived and replaceable.
- The user can revoke any agent, device, host, plugin or provider.
- Consequential actions require a trusted confirmation path that a compromised node or guest UI cannot forge.
- Voice is never authentication; a spoken "yes" still goes through the trusted confirmation path.
- Nothing is sent, said or signed in the user's name without confirmation or a pre-approved scope.
- The personal model and any voice or face data never leave the user's devices unencrypted.

### 7.2 Required controls

1. **Capabilities, not broad access:** read-mail, create-draft, send-mail, create-calendar-event and share-file are distinct grants. Protocol clients receive only the scopes their host was granted.
2. **Secrets outside model context:** OAuth tokens, API keys and signing keys held by protected storage or a dedicated credential-broker process, never placed in prompts, never carried by `pr3-client-c`.
3. **Sandbox untrusted tools:** OS-specific enforcement, not merely a model instruction; mobile apps remain constrained by native app sandbox.
4. **Network egress policy:** deny external model endpoints by default; allow only explicitly selected service destinations. Platform-model backends are local, but their vendors' telemetry is measured and disclosed.
5. **Approval receipts and one-use capabilities:** an approval binds principal, actor, operation, object, exact parameters, expiry, nonce and target provider; it is signed by a key in the platform keystore after biometric/OS authentication; `pr3-vault` exchanges it for a capability with the same binding:

   ```text
   Capability:
     actor:             agent:mail-assistant
     principal:         user:…
     operation:         mail.send
     object:            to:alex@example.com / draft:sha256:4c1e… / attachments:sha256:9b07…
     provider:          mail:sandbox          (or the user's mail provider)
     receipt_hash:      8a3f…
     issued_at/expires: now / +90 s
     max_uses:          1
   ```

   The capability — not a credential — is what the execution adapter receives. Field names are compared with ZeroClaw's tool-receipt format before the schema is frozen (§5.1).
6. **Idempotency:** unique action keys and reconciliation against provider execution status; do not re-execute actions after retry/disconnection.
7. **Data handling:** local encryption, redacted logs, retention and deletion semantics, import/export, secure device removal; deletion of all physical backup remnants cannot be guaranteed without backup control.
8. **Supply chain:** pinned dependency versions and vendored llama.cpp commit, SBOM, signature verification, reproducible builds where feasible, periodic third-party review.
9. **Memory safety as a measured property, not a slogan:** `#![forbid(unsafe_code)]` on every crate except `pr3-llamacpp`, `pr3-ffi` and the `sys` modules of `pr3-platform-llm`, `pr3-vault` and `pr3-speech`, enforced in CI; cargo-fuzz on every parser (JSON, schema, protocol frames, model templates); Miri on `pr3-core`; panic-site allowlist check on the `no_std` artifact; fallible allocation at untrusted boundaries; ASan and libFuzzer on the audited `unsafe` code, UBSan on the C/C++ it wraps (llama.cpp and whisper.cpp shims) and on `pr3-client-c`; adversarial prompt-injection suites.
10. **Vault discipline:** `pr3-vault` returns results or restricted capabilities, never raw master secrets; signing happens inside platform key storage (Secure Enclave, StrongBox, TPM) wherever the platform offers it.
11. **Secrets never cross a protocol boundary:** `pr3-client-c` carries capabilities and receipts, not credentials; the node holds no approval keys.

Rust is selected for safe ownership and concurrency guarantees in the core, **not as a substitute for a security review**. `unsafe` code in llama.cpp bindings, GPU runtimes, native plugins, FFI and platform APIs remains a critical attack surface, and the C client is C.

### 7.3 Trust tiers — `dev`, `protected`, `secure`

The propose/dispose split (§3) is a design boundary first and a deployment boundary later. Three tiers share one core and one protocol; a tier changes *where the trusted side runs*, never what it does.

| Tier | Trusted side runs in | Keys and signing | Protects against | When |
|---|---|---|---|---|
| **`dev`** (PR3 Runtime) | The app's own process, as separable `no_std` modules | Platform keystore via `pr3-vault` (Secure Enclave, StrongBox, Keychain/Keystore, TPM) | A misbehaving model, plugin or tool; replay; UI spoofing (phone confirmation) | M1 onward — the product on today's OSes. No claim against a compromised host kernel |
| **`protected`** | A platform-isolated environment: Android AVF protected VM, or a dedicated OS-sandboxed process on desktop | Same, plus attestation of the protected environment where available | A compromised app process or guest userland | Post-M5, only if live execution (payments, irreversible actions) needs it; gated by evidence, not scheduled |
| **`secure`** | seL4 protection domains (`rust-sel4`, Microkit), with the app and its OS as an untrusted guest | Keys never leave a native seL4 domain | A compromised guest OS kernel | Preserved option only (ADR-002). Not funded, not scheduled |

Rules:

- The `no_std + alloc`, sans-I/O, panic-free discipline on the trusted-side modules (§4.2) is what makes `protected` and `secure` reachable without a rewrite — the same property that preserves the console path (Appendix B).
- Security claims are per tier and per configuration. PR3 may say "critical isolation boundaries are enforced by the formally verified seL4 microkernel in supported configurations" only for a shipped `secure` build on a verified configuration, and never that the whole device, VMM, firmware or AI stack is verified. The `dev` tier makes no claim against a compromised OS and must not imply one.
- On ARM64, seL4 and Android's pKVM both occupy EL2; `protected` (AVF/pKVM) and `secure` (seL4) are alternative backends, not layers. Nested virtualization is never a dependency.
- No tier work beyond `dev` begins before the M5 gate, and `secure` begins only on an RFC with a funded partner (government, key custody or OEM) and a chosen reference board.

---

## 8. Benchmarking: PR3 vs OpenJarvis and the Wider AI Ecosystem

A good comparative program needs two separate leaderboards: **agent-system quality** and **underlying model intelligence**. Using the same Qwen weights in PR3 and OpenJarvis tests the system; changing models simultaneously confounds the comparison.

### 8.1 Equal-conditions OpenJarvis comparison

**Freeze a reproducible benchmark manifest:** PR3 commit, OpenJarvis commit, model ID/hash and quantization, hardware, OS, inference backend, token/context budgets, temperature/seed (where applicable), tool definitions, network access, datasets, evaluation scripts and run dates. Match all these where possible. Report unavoidable differences.

Required comparison configurations:

| Comparison | Purpose |
|---|---|
| **PR3 + model X vs plain tool-calling harness + model X vs OpenJarvis + model X, AgentDojo** | Headline result: attack success (unauthorized consequential actions, tool-argument exfiltration) and benign-task utility under attack, compared with CaMeL's published numbers. Confirmations are answered by a scripted approver that accepts only the actions the user task requires |
| PR3 + model X vs OpenJarvis + model X | Agent, memory, planning and workflow quality |
| PR3 local vs PR3 hybrid, matched task | Benefit/cost of private offloading |
| **PR3 platform model vs PR3 bundled GGUF, matched task** | Decide whether a mandatory model download is justified per platform (new in v1.2; gate at M2) |
| PR3 no-memory vs PR3 memory | Effect of retrieval |
| PR3 fixed routing vs adaptive routing | Effect of learning policies |
| PR3 on-device mobile vs supported mobile baselines | Mobile utility/efficiency |
| PR3 + model X vs other model X harnesses | Reproducibility and model orchestration |
| **PR3 vs ZeroClaw, same local model behind the same endpoint, tool-reliability and security scenarios** | Schema-valid tool calls, argument correctness, prompt-injection resistance, approval-gate behavior. Not task quality or energy — ZeroClaw has no comparable evaluation harness, so OpenJarvis remains the baseline there (new in v1.2) |

PR3 ships no HTTP-provider backend, so matched comparisons against HTTP-based harnesses (ZeroClaw, and OpenJarvis's llama.cpp server bridge) run PR3 through a **benchmark-only OpenAI-compatible backend** in `benches/`, pointed at the same server. That isolates agent behavior from the inference engine; in-process versus server efficiency is reported as a separate comparison. The benchmark backend is never part of a shipped build.

### 8.2 Evaluation dimensions

| Category | Measurement | Design note |
|---|---|---|
| Task completion | End-to-end pass@1, partial success, recovery | Ground truth for every scenario |
| Tool reliability | Schema-valid tool calls, argument correctness, retries | Include malicious/untrusted inputs; platform models' guided generation counted separately |
| Memory | Retrieval precision/recall, stale-fact errors, correct deletion | Time-separated held-out scenarios |
| Long-horizon autonomy | Durable state, interrupted workflow recovery, duplication | Simulate network/device failures |
| Learning | Improvement on held-out post-policy tasks | Compare against no-learning controls |
| Privacy | Unauthorized disclosure rate, outbound bytes, secret exposure | Zero critical failure as release gate |
| Authorization | Capability replay acceptance; unauthorized high-risk execution; incorrect object/recipient resolution; forged or bypassed approvals | Targets 0, 0, ≈0, 0; adversarial suites include a lying node and a spoofing UI |
| Injection resistance | Attack success rate on AgentDojo; utility with and without attacks | Same model and tools for every system; reported per attack class |
| Speech | Word error rate per language, voice-note turnaround, spoken-reply latency, battery per minute of conversation | On-device and node measured separately |
| Personal model | Drafts accepted without edits; preference-prediction accuracy on held-out approvals; calibration; corrections per week | Blind test: can the user tell drafts from their own writing? |
| Efficiency | TTFT, tokens/s, total task latency, RSS, energy/task, binary size, **model download footprint** | Separate inference from agent overhead |
| Reliability | Crash-free runs, memory corruption, p95 latency | Stress tests and fuzzing |

### 8.3 AI benchmarks and external participation

PR3 is an **agent system**, not inherently a foundation model. To enter public leaderboards, distinguish whether a benchmark accepts an agent harness, an API, or standalone model weights. Participation in model-only benchmarks tests the attached model and is not proof that PR3 itself has a better base model.

Candidate suites to evaluate for fit:

- **Security (headline):** AgentDojo — 97 realistic tasks (email, banking, travel) and 629 prompt-injection test cases for tool-using agents; CaMeL's published results on it are the research baseline.
- **Agent tasks:** GAIA, τ-bench, AgentBench, BFCL (function calling), selected WebArena/OSWorld-style tasks if appropriate.
- **Coding integrations:** SWE-bench Verified with consistent tool and environment budgets.
- **Model reasoning:** MMLU-Pro, GPQA-style tests, LiveCodeBench and other model benchmarks when comparing specific weights (not claiming runtime intelligence).
- **Custom PR3-Personal-200:** consent-safe synthetic tasks covering email, calendar, private memory, multi-device handoff and file-sharing workflows.

Before adopting any benchmark, confirm its current rules and contamination policy. Do not publicly imply official benchmark certification without actual submission and acceptance.

### 8.4 Research validity

- Use train/development/held-out splits and blinded task evaluation where possible.
- Run repeated trials for stochastic agent tasks; report confidence intervals and variance.
- Report failures alongside successes and audit all manual interventions.
- Use equivalent tool permissions and task context for OpenJarvis and PR3.

### 8.5 Proposed acceptance gates (targets, not measured results)

| Release | Gate |
|---|---|
| Core alpha | `pr3-core` builds `no_std` on `thumbv7em-none-eabihf` and `wasm32-unknown-unknown` with every reachable panic site on the allowlist (§4.2); deterministic mock-agent test suite; cross-builds succeed; `pr3-client-c` compiles on Linux and round-trips the protocol |
| Local MVP | ≥90% success on tightly scoped offline PR3-Personal tasks; no critical permission bypass in tests; platform-vs-bundled benchmark reported per platform |
| Hybrid beta | >99% state reconciliation on fault-injection tests, zero duplicated simulated actions; authenticated handoff |
| Security & quality beta | With a matched model on AgentDojo: zero unauthorized consequential actions and zero tool-argument exfiltration for PR3, against measured attack success for a plain tool-calling harness and OpenJarvis; benign-task utility within 10 points of the undefended harness (provisional); task-quality parity with OpenJarvis on the predeclared subset |
| Actions beta | Zero unauthorized, duplicated or replayed consequential actions across sandbox connectors under fault injection and the §12 red-team scenarios; deterministic amounts, limits and recipients exact on a gold test corpus |

Targets may be revised only by a documented decision before final benchmark runs; a failed target must be reported rather than hidden.

---

## 9. Domain Applications

PR3 is application-neutral. Domain applications — productivity, developer tools, games or anything else — are built outside this repository on PR3's SDK, plugins and connectors (§4.7). They are extensions, never dependencies of the core, and this document does not plan any of them.

### 9.1 Rules for applications that take consequential actions

- Every consequential action goes through the structured-intent sequence (§4.6); an application may add checks, never remove them.
- Values an action depends on — amounts, limits, recipients — are computed or resolved by deterministic, validated code; the model explains and summarizes, it does not decide.
- Limits and risk settings are configured by the user, never inferred silently from conversation.
- Kill switch, per-action maximums, stale-data cutoffs, expiry, idempotency and a full audit trail.
- *No action* is always a valid output, and notification throttling stops the application from prompting users into unnecessary actions.
- No private key or unrestricted provider credential ever reaches the model.

### 9.2 Fitting the on-device model

On-device models are around the 3B class. Domain applications should give them narrow, structured jobs — explain a deterministic engine's output, summarize a signed event, answer a question from retrieval — and escalate long reasoning to the node. The deterministic engine, not the LLM, carries the correctness burden.

---

## 10. Application Areas & Interoperability

| Area | Example | Additional challenges |
|---|---|---|
| Personal assistant | A private, voice-first PR3 on the user's own phone and node: voice notes, email, calendar, documents, proactive briefings, drafts in the user's style | Provider OAuth permissions, background restrictions |
| Developer tools | Embedded local coding/review agent | Codebase context, performance, sandboxing |
| Games / Unreal | Local NPC cognition, persistent characters; the game as a client of the player's own PR3 node | Frame-time, deterministic game logic, platform policy on cross-title user data |
| Automotive infotainment | In-car assistant for non-safety-critical functions | NPU/OS SDKs, voice, safety separation |
| Organizations | On-prem private agent with verifiable data access | Identity, governance, integration |
| Edge / appliances | Embedded decision and tool runtime | Extremely small RAM, optional remote inference |

**Priority:** (1) repeatable benchmarks (§8); (2) a **private personal PR3** — the user's own phone and node, with all of their data under their control — as the first reference deployment, used daily by its own developers and exercising the trust architecture end to end; (3) other areas only after documented field reliability. Avoid opening all areas in parallel.

**Interop note (optional, post-M2):** if `pr3-node` exposes an OpenAI-compatible endpoint, third-party runtimes such as ZeroClaw can use it as a model provider, which gives a PR3 user those runtimes' channel integrations (Telegram, Discord, email) without PR3 writing channel code. This is an adoption lever to revisit after the hybrid gate, not a milestone and not a dependency.

**Open source.** PR3 is Apache-2.0 (§15). The core stays fully usable by anyone, nothing is held back, and users' private memories and data are never sold or shared.

**User research.** Talk to 10–15 developers/OEM integrators and 15–20 prospective end users to learn deployment blockers, privacy requirements and workflow value. Interview counts are research goals, not evidence.

---

## 11. Delivery Roadmap, Epics & Issues

Dates and effort are planning estimates for an experienced founder using AI coding tools plus focused specialist review, **not delivery guarantees**.

**How the gates fit together.** The release gates in §8.5 are the pass criteria. Each milestone gate below restates the release gate it must meet plus milestone-specific deliverables; the §14 go/no-go questions are the scope decisions taken on those results. Where they differ, §8.5 governs.

| Milestone | Release gate (§8.5) | Go/no-go (§14) | Release channel (§12) |
|---|---|---|---|
| M0 | Core alpha | After M0 | `dev` |
| M1 | — (internal; M1 gate below) | — | `dev` |
| M2 | Local MVP (Android, desktop); Hybrid beta | After M2 | `dev` |
| M3 | Security & quality beta | After M3 | `dev` |
| M4 | Local MVP on iOS | — | `dev` |
| M5 | Actions beta | After M5 | `dev` |
| M6 | — | — | `stable` |

### Spike build order — simulators and macOS first

The first 6–8 weeks build the spike in small, reviewable steps rather than milestone by milestone. Every step has the same gate: it runs in the macOS app, on the iOS simulator and on the Android emulator, and its tests pass. Work the spike does not need — `pr3-client-c`, Unreal/Emscripten, the full protocol v0 and formal methods beyond the policy core — waits until the spike's results are in.

| Step | Builds | Milestone epics it advances |
|---|---|---|
| 1 | Cargo workspace; `pr3-core` as a sans-I/O `no_std` `step()` machine; Tauri 2 app on macOS, iOS simulator and Android emulator with a chat screen driven by `step()` and a mock model | M0-E3, M1-E1 |
| 2 | Qwen3 through `pr3-llamacpp`: macOS app first, then simulator and emulator | M1-E3, M2-E1 |
| 3 | Agent loop with structured tool calls and OpenJarvis-compatible traces | M1-E1, M1-E2 |
| 4 | SQLite memory with provenance | M1-E5 |
| 5 | Trusted side for one action (sandbox email): intent → policy → phone confirmation → one-use capability → audit | M1-E7, M2-E3 |
| 6 | The macOS app as the user's node: pairing and handoff | M2-E2, M2-E3, M2-E4 |
| 7 | Voice notes | M2-E7 |
| 8 | AgentDojo harness: PR3 against a plain tool-calling harness with the same model | M0-E1, M3-E5 |

The spike ends with the 30-day user-zero diary and the AgentDojo result; together they decide whether the full roadmap continues as written.

### Milestone M0 — Benchmark & Architecture Spike (Weeks 1–3)

**Deliverables:** upstream OpenJarvis version pin, representative workflows, evaluation harness, platform feasibility matrix, Rust workspace with `no_std` core and CI check targets, `pr3-protocol` v0 schema, `pr3-client-c` skeleton, ADR-001.

**Epics/issues:**
- M0-E1: freeze OpenJarvis baseline and the AgentDojo version, record model/quantization, create reproducible container/desktop environment.
- M0-E2: implement 30–50 task smoke-suite and evaluation taxonomy.
- M0-E3: `pr3-core` as sans-I/O `no_std + alloc` crate; CI `cargo check --no-default-features` on `thumbv7em-none-eabihf` and `wasm32-unknown-unknown`; forbid-unsafe and panic-site allowlist checks; builds for macOS/Linux/Windows and a minimal Android ARM64 host.
- M0-E4: `pr3-protocol` v0 (CBOR framing, versioning, handoff/sync message families); `pr3-client-c` skeleton compiling on Linux. The freestanding and Emscripten builds serve deferred targets and move to M4-E3 with the Unreal sample.
- M0-E5: dependency inventory (`cargo deny`, SBOM); threat model and security invariants (§7.1) as a checked-in document; ADR-001 and ADR-002 (Appendix C); the intent envelope, capability-receipt and audit-record schemas land in `pr3-protocol` v0 — they are protocol, not implementation.

**Gate:** identical core sources compile `no_std` and `std` across targets and reproduce deterministic mock-agent behavior; protocol round-trips between a Rust runtime and the C client.

### Milestone M1 — Offline Intelligence Kernel (Weeks 4–8)

**Deliverables:** bounded agent state machine, structured tools, `pr3-llamacpp` with prebuilt libraries for desktop and Android, SQLite memory, Foundation Models spike on macOS, PR3-Personal corpus, CLI and integration tests.

Scope note: M1 was trimmed in v1.4 to fit five weeks. iOS prebuilts move to M4 (with the iOS host), the Gemini Nano spike and Android Keystore move to M2 (with the Android host), and Kani harnesses move to M3; Proptest covers the trusted-side state machines until then.

**Epics/issues:**
- M1-E1: task state lifecycle, cancellation, retry, checkpointing in `pr3-core`/`pr3-agent`.
- M1-E2: tool schema, validation, registry, authorization callbacks; cargo-fuzz targets for every parser.
- M1-E3: **`pr3-llamacpp` — the milestone's headline risk:** vendored pinned submodule, frozen C shim, bindgen on the shim, prebuilt static libs for macOS, Linux, Windows and Android arm64 with the checksummed fetch step (§4.3), feature-per-backend, safe public API, ASan/UBSan + fuzz on the boundary.
- M1-E4: `pr3-platform-llm` spike: Apple Foundation Models on one macOS 26 Apple Intelligence Mac, behind the shared `InferenceBackend` trait with declared `ModelCapabilities`.
- M1-E5: SQLite store (`pr3-memory-sqlite`), source provenance, FTS5, migration tests.
- M1-E6: error and crash handling; Miri on core; sanitizers/fuzzing CI.
- M1-E7: trusted-side modules — intent envelope and validator, policy decision, capability issuance/expiry, hash-chained audit — with Proptest properties (§12); `pr3-vault` adapter over desktop key storage (macOS Keychain, Windows CNG/TPM, Linux Secret Service); CI check that agent/inference modules never become dependencies of trusted-side modules.
- M1-E8: grow the M0 smoke suite into the PR3-Personal corpus (§8.3) — 200 consent-safe synthetic tasks with a held-out split — frozen before the M2-E6 and M3-E5 runs.

**Gate:** the M0 smoke suite, run fully offline on desktop, reaches ≥75% task success (provisional; fixed before the final run per §8.5) with traceable tool calls on the bundled backend and on Foundation Models on macOS; every consequential action in the suite passes through envelope → intent → policy → capability → audit.

### Milestone M2 — Android + Desktop Hybrid (Weeks 9–14)

**Deliverables:** Android host; macOS/Linux `pr3-node`; authenticated device pairing; handoff, resume and offline fallback; platform-vs-bundled decision; voice notes.

**Epics/issues:**
- M2-E1: direct Tauri Rust integration (Kotlin/JNI or UniFFI only for platform-specific capabilities); on-device 1B–4B inference benchmarks, CPU first, Vulkan measured per device class.
- M2-E2: `pr3-node` with local 8B–32B reference model; proactive/background jobs run here.
- M2-E3: pairing, identity registration and revocation, authenticated encrypted transport and replay control; **phone-side trusted confirmation** of node-prepared actions with biometric approval and vault-signed receipts (§6.3); `pr3-vault` on Android Keystore/StrongBox (moved from M1).
- M2-E4: task-state delegation protocol with a TLA+ (or Quint) model checked under node loss, message duplication, reordering and replay; interruption/duplicate-action tests derived from the model; `pr3-client-c` exercised as a second client type.
- M2-E5: notification through the publisher push relay with sealed wake-ups, and UnifiedPush (§6.3); power profiling, deterministic-pre-filter-before-LLM policy; onboarding usable before the optional model download.
- M2-E6: Gemini Nano Prompt API backend on one supported Android device (moved from M1), then the **platform model vs bundled GGUF benchmark** on PR3-Personal tasks; decide per platform whether the bundled download is default, optional or omitted.
- M2-E7: voice notes in the Android and desktop apps — platform speech-to-text, replies as text or platform speech by the user's choice; conversation → memory proposals.

**Gate:** Local MVP on Android and desktop, and Hybrid beta (§8.5): same task works locally, delegates privately when authorized, and recovers from node loss without duplicate actions; platform-vs-bundled decision documented.

### Milestone M3 — Privacy Memory & Adaptive Learning (Weeks 15–20)

**Deliverables:** encrypted personal memory, provenance, limited cross-device synchronization, trace-based routing (including backend-class routing) and held-out evaluation.

**Epics/issues:**
- M3-E1: SQLCipher/keychain/keystore integration with key recovery design.
- M3-E2: append-only sync log, conflict reconciliation and tombstones.
- M3-E3: trace evaluation, policy routing across platform/bundled/node, rollback and held-out test harness.
- M3-E4: privacy egress tests, prompt-injection attacks, hostile-protocol-frame tests and redaction audit; Kani harnesses on the policy, capability and approval-receipt state machines (moved from M1, §12).
- M3-E5: compare PR3 vs OpenJarvis under matched scenarios; publish honest report.
- M3-E6: personal model — style profile and preference model with held-out evaluation (§4.5, §8.2); a 30-day user-zero diary (daily: time saved, confirmations asked, drafts accepted) as gate evidence.

**Gate:** Security & quality beta (§8.5) met, adaptive routing beats fixed routing on held-out tasks (§8.1), critical privacy gates passed, and the user-zero diary (M3-E6) is recorded. If adaptive routing shows no held-out gain, the fixed policy ships and the gate records learning as not yet earning its cost — a scope decision under §14, not a pass.

### Milestone M4 — iOS & SDK (Weeks 21–28)

**Deliverables:** iOS host, packaged native library with UniFFI Swift bindings, hardened `pr3-client-c`, Unreal C++ sample (native and HTML5 via the client), memory/CPU profiling and documented toolchain support.

**Epics/issues:**
- M4-E1: signed iOS framework / Swift bindings; Foundation Models backend production-ready; iOS arm64 + simulator llama.cpp prebuilts (moved from M1) and the bundled GGUF Metal path per the M2 decision.
- M4-E2: platform background execution and notification design on iOS.
- M4-E3: C ABI versioning and smoke tests in a minimal C host and Unreal C++ host; select and record the Unreal HTML5 fork (repository and commit) and the emsdk version it pins — every reference to "the Unreal fork" in this document means that record; `pr3-client-c` fuzzed and sanitized, console-style freestanding build, pinned-emsdk CI job (both moved from M0); **console targets remain out of scope** (Appendix B maintained as RFC).
- M4-E4: asset/model packaging and download integrity checks.
- M4-E5: release artifacts, SBOM, docs and reproducible examples.
- M4-E6: voice on iOS; spoken replies (platform voices) on iOS and Android; whisper.cpp fallback on desktop.

**Gate:** Local MVP on iOS (§8.5); fully offline reference workflow on Android/iOS/desktop; cross-platform SDK examples build; Unreal HTML5 sample links with the fork's emsdk.

### Milestone M5 — Consequential Actions in Sandbox Mode (Weeks 29–36)

**Deliverables:** end-to-end consequential workflows through sandbox connectors — outbound mail, calendar changes and a simulated payment provider — with phone-side confirmation, one-use capabilities, idempotent execution and audit; proactive workflows on `pr3-node`; a red-team report.

**Epics/issues:**
- M5-E1: sandbox connectors with signed manifests (§4.7) for mail, calendar and a simulated payment provider.
- M5-E2: deterministic per-action limits, expiry, kill switch, idempotency keys and reconciliation against the sandbox provider's status.
- M5-E3: confirmation UX for each action class on Android and iOS; *no action* as a valid, audited outcome (§9.1).
- M5-E4: proactive workflows on `pr3-node` with explanations and notification throttling through the push relay (§6.3).
- M5-E5: red-team pass — a lying node, an over-reaching plugin, a spoofing UI, replayed receipts, and prompt injection through mail and documents (§12).
- M5-E6: decide whether live execution needs the `protected` tier (§7.3) and, for each candidate live provider, where its credential and execution adapter live (§6.3).

**Gate:** Actions beta (§8.5); red-team findings closed or accepted with a documented reason; zero critical authorization failures; the `protected`-tier and credential-location decisions recorded.

### Milestone M6 — Public Open Source Release & Expansion (Post-M5)

Release signed SDKs, `pr3-client-c`, portable samples, benchmark report, contribution guidelines, versioned protocol and C ABI, and the supported-target matrix (§6.5). Expand hardware targets only after measured need; revisit console scope per Appendix B's trigger. The optional WebGPU digital face (§6.6) is post-M5 work.

---

## 12. Build, CI/CD & Quality Gates

**Toolchain:** Rust stable + Cargo workspaces, `rust-toolchain.toml`, `cargo fmt`, Clippy (with the `pr3-core` deny-list), `cargo test`, `cargo deny`/`cargo audit`, reproducible dependency locks. A second, separately pinned nightly runs only Miri, cargo-fuzz and `-Zsanitizer` jobs, all of which require it; product builds never use it. CMake/Clang/Android NDK/Xcode apply only to building the vendored llama.cpp prebuilts, `pr3-client-c` and platform packaging — never to a downstream `cargo build`.

**CI matrix:**

- **Hosts:** macOS ARM64 and x86_64, Linux x86_64/ARM64, Windows x64/ARM64 where available; Android ARM64; iOS ARM64 plus simulator under supported Xcode.
- **`no_std` discipline:** `cargo check --no-default-features` for every `no_std` crate on `thumbv7em-none-eabihf` and `wasm32-unknown-unknown`, as a separate job (feature unification otherwise masks leaks); panic-site allowlist check of the `pr3-ffi` core build (§4.2).
- **Safety:** `#![forbid(unsafe_code)]` verified by a lint job that fails on any new `unsafe` outside `pr3-llamacpp`, `pr3-ffi` and the `sys` modules of `pr3-platform-llm`/`pr3-vault`/`pr3-speech`; cargo-fuzz corpora for JSON, schema, protocol frames and model-template parsers; Miri on `pr3-core` tests; ASan on the audited `unsafe` crates; UBSan on the llama.cpp and whisper.cpp shims and `pr3-client-c` (UBSan does not apply to Rust code; Miri is its Rust counterpart).
- **Prebuilts:** llama.cpp + shim static libs built per target from the pinned commit and published as versioned artifacts with checksums, fetched by the explicit step in §4.3 — never by `build.rs`.
- **C client:** Linux build from M0; from M4, freestanding build with stubbed callbacks and Emscripten build on the emsdk pinned by the recorded Unreal fork; libFuzzer on the frame parser.
- **Smoke tests:** direct Cargo/Tauri host, C ABI minimal host, UniFFI Swift/Kotlin bindings, protocol round-trip Rust↔C.
- **Determinism:** protocol/unit tests; model-backed tests with version-pinned weights; platform-model tests marked as device-gated.
- **Supply chain:** security scanning, SBOM, reproducible builds where feasible.

**Verification beyond tests — narrow by design.** Do not attempt to verify the whole product. Apply formal methods where a bug costs money or trust, and nowhere else:

| Component | Method | What is proven or checked |
|---|---|---|
| Policy decision, capability issuance/expiry, approval-receipt binding (`pr3-core`, `no_std`) | Kani bounded model checking on the Rust source; Proptest properties | No path grants a capability without a matching receipt; expired or replayed receipts never authorize; a plugin's grants cannot widen at runtime |
| Hybrid handoff and sync protocol (`pr3-protocol`, `pr3-hybrid`) | TLA+ (or Quint) specification checked with TLC under node loss, message duplication, reordering and replay | No duplicated consequential action; state reconciles; tombstones win over stale writes |
| Audit chain | Property tests plus a standalone verifier tool | Any rewrite breaks the chain; checkpoints verify against the device key |
| Model and policy-pack updates | State-machine model plus power-loss tests | Never left without a runnable model or policy; rollback always possible |

Required test layers in addition: unit tests for every deterministic policy rule; malformed-model-output and hostile-protocol-frame fuzzing; replay and race tests; host-compromise simulation (a node that lies, a plugin that over-reaches, a UI that spoofs); power-loss and interrupted-download tests; cross-version protocol conformance; a red-team pass before any live execution; an independent security review before live consequential actions ship.

**Release channels:** `dev` (no security claims) → `stable` (signed release) → `secure-reference` (fixed hardware and reviewed configuration, if and when a `protected` or `secure` tier ships). Claims are scoped per channel and per tier (§7.3). §11 maps channels to milestones.

**Portability warning:** Cargo compilation and the C ABI do not eliminate Apple signing, Android/iOS background limits, GPU/NPU backend availability or platform-specific SDK restrictions. Browser Wasm requires browser-adapted storage/network/scheduler adapters. Console support is not part of initial acceptance criteria.

---

## 13. Risks & Mitigations

| Risk | Severity | Mitigation |
|---|---|---|
| llama.cpp binding drift and build complexity leaking into every `cargo build` | High | Vendored pinned commit; frozen C shim; prebuilt per-target libs; upgrade cadence with conformance run; all `unsafe` in one crate |
| Unsafe Rust/FFI and native backend vulnerabilities | High | Safe-Rust core enforced by CI; `unsafe` confined to two audited crates and three platform-FFI `sys` modules; fuzzing, sanitizers and C ABI tests |
| `no_std` discipline erodes (a `std::fs` or Tokio dependency slips into core) | Medium | Separate `no_std` check job on a target with no `std`; forbid-unsafe lint job; ADR-001 names this as the condition that reopens the language decision |
| Platform models displace the bundled backend, or change behavior/availability | Medium | Model-neutral trait; platform models are backends not dependencies; M2 benchmark decides bundling per platform; telemetry measured before default-on |
| Mobile memory/thermal/battery limits | High | Platform model where available; quantized small bundled model; deterministic pre-filter before LLM; short-context profiles; graceful degradation |
| Different tool-calling behavior by model | High | Per-model templates, declared `ModelCapabilities`, structured schemas, conformance suite, reject malformed actions; evaluate reuse of `zeroclaw-tool-call-parser` (§5.1) |
| Emscripten/Unreal fork version drift breaks the HTML5 client | Medium | Pinned emsdk CI job; C11 with no toolchain-specific features; single-binary Rust+Unreal explicitly out of scope |
| Python-free runtime falls behind research | Medium–High | Separate training/research environment; export learned artifacts; avoid reinventing all ML |
| OpenJarvis improves upstream faster | High | Track upstream monthly; run fixed and rolling baselines; compete on specific measurable outcomes |
| Platform owners absorb the agent layer within their own ecosystems | High | Stay underneath: platform models as backends, OS hooks as connectors (§2); differentiate on cross-platform policy, memory and audit; adoption through developers who embed PR3, since end users rarely adopt a security layer for its own sake |
| Unauthorized tool activity | Critical | Capability security, protected credentials, per-action signing, kill switch, fuzzing and audit |
| Background execution unreliable on phones | High | Proactive work on the user-controlled node; honest feature limits; notifications not guaranteed offline |
| E2EE sync data loss/conflicts | High | Event logs, stable IDs, tombstones, recovery backups, fault-injection tests |
| Console scope creep before a partner exists | Medium | Appendix B trigger; no console work beyond `no_std` hygiene and the C client until a funded partner or next-gen platform ML APIs appear |
| Trusted-path spoofing: a compromised node or UI shows one action and requests another | Critical | Phone-side confirmation rendered by PR3 from the validated intent, biometric approval, vault-signed receipt bound to exact parameters; the node holds neither an approval key nor a live provider credential (§6.3) |
| Impersonation through a stolen personal model, voice or face data | Critical | Encrypted at rest under vault-held keys; never leaves the user's devices unencrypted; device revocation wipes it; nothing sent as the user without approval (§7.1) |
| Approvals become rubber stamps as the personal model improves | High | Routine actions in pre-approved low-risk scopes so prompts stay rare and meaningful; deliberate friction on consequential ones; approval-time trend tracked (§14) |
| Scope creeps back toward a full OS / seL4 platform | Medium | Trust-tier rule (§7.3): nothing beyond `dev` before M5, `secure` only on a funded RFC; Appendix D records what was deliberately dropped |
| Capability or receipt schema frozen too early or too loosely | Medium | Schemas live in `pr3-protocol` with versioning; Kani/Proptest harnesses from M1; field comparison with ZeroClaw's receipt format before freezing |
| Unproven security claim | High | Pre-registered AgentDojo comparison with matched models; compare against CaMeL's published numbers; release all results including failures |

---

## 14. Success Metrics & Go/No-Go Gates

**Technical North Star:** completed, correct and authorized real-world tasks *per joule and per user intervention*, with privacy violations treated as hard failures.

Track monthly: AgentDojo attack success rate, offline task success, matched OpenJarvis comparison delta, private hybrid fallback success, average and p95 response time, joules/task, mobile RAM, model download footprint, platform-vs-bundled routing share, tool-schema error rate, unauthorized action rate, recovered task rate, user corrections per task, drafts accepted without edits, and approval-time trend.

Reliability targets carried into every release gate: unauthorized high-risk execution 0; capability replay acceptance 0; secret exposure incidents 0; incorrect object or recipient resolution ≈0; low-risk workflow success ≥95% by M4; recovery from node loss ≥99%.

**Go/no-go gates:**

- **After M0:** Can the architecture build `no_std` and `std` on supported targets, round-trip the protocol to the C client, and benchmark against OpenJarvis fairly?
- **After M2:** Can the phone truly operate offline (platform or bundled model), recover gracefully from private-node unavailability, and has the bundling decision been made on evidence?
- **After M3:** Does the security result hold on AgentDojo (§8.5), and does learning produce reproducible quality gains or at least parity with materially better efficiency?
- **After M5:** Do consequential workflows run end to end in sandbox mode with zero critical authorization failures, and does any live provider justify the `protected` tier?

If any gate fails, reduce product scope or reuse upstream components rather than automatically expanding engineering effort.

---

## 15. Governance & Community

Suggested governance: public RFCs/ADRs (starting with ADR-001), versioned Rust SDK, protocol and C ABI compatibility commitments, transparent benchmark reports, contribution guidelines and a security reporting process. PR3 is licensed under Apache-2.0 (`LICENSE`).

Keep user data, credentials and personal trace logs outside the public codebase. Console SDK adapters, if ever built, live in the console partner's repositories; the public repository carries only `pr3-client-c` and the `no_std` core they build against. Public benchmarks use synthetic or consented datasets.

PR3 may claim "inspired by OpenJarvis" with attribution, but should avoid suggesting endorsement by OpenJarvis or Stanford unless such affiliation is explicitly established.

---

## 16. Recommended Immediate Actions

1. Pin and build the current OpenJarvis baseline, including its Rust modules; select a consistent model for both projects; set up AgentDojo with that model as the headline security benchmark.
2. Write a 30–50 scenario evaluation corpus covering structured tool calls, memory, policy, offline behavior and interruptions.
3. Stand up the workspace with `pr3-core` as a sans-I/O `no_std + alloc` crate and the `no_std` CI check job on day one; record ADR-001 and ADR-002.
4. Write the threat model, the security invariants and the intent-envelope / capability-receipt / audit-record schemas into `pr3-protocol` v0 before the agent loop exists; they are the contract everything else is built against.
5. Define the rest of `pr3-protocol` v0 and compile `pr3-client-c` on Linux; its freestanding and Emscripten builds wait for M4.
6. Build `pr3-llamacpp` as described — pinned submodule, frozen shim, prebuilts — before any sophisticated agent features; in parallel, spike Apple Foundation Models on macOS behind the same trait (Gemini Nano follows with the Android host in M2).
7. Measure PR3 on AgentDojo against a plain tool-calling harness and OpenJarvis with the same model, and platform-vs-bundled on matched tasks; decide bundling and feature scope from evidence.
8. Start consequential-action work (the M5 sandbox connectors) only after the privacy and reliability gates pass.
9. Do nothing on consoles beyond keeping Appendix B current, and nothing on `protected`/`secure` tiers beyond keeping the trusted-side modules `no_std`-clean.

### Final architecture recommendation

> **PR3 = Rust-first, sans-I/O `no_std` intelligence core + direct Cargo/Tauri integration + optional versioned C ABI and UniFFI bindings + versioned wire protocol with a dependency-free C client for everything Rust cannot reach + platform-model / bundled-GGUF / private-node inference behind one trait + user-owned memory + evaluated adaptive policies + explicit private hybrid device cooperation.**
>
> **Positioning = the layer underneath: platform models are backends and OS hooks are connectors; the claim is authorization under attack, and the visible feature is cross-device continuity.**
>
> **OpenJarvis = scientific/engineering reference and matched benchmark baseline, not a mandatory production dependency.**
>
> **Domain applications = extensions built on PR3, never owners of the user's private intelligence.**
>
> **Trust architecture = model ≠ authority; propose/dispose split; intent → policy → confirmation → one-use capability → hash-chained audit; the phone as the trusted confirmation device.**
>
> **Trust tiers = `dev` now; `protected` only if live execution needs it; `secure` (seL4) preserved by the same `no_std` trusted-side modules, not scheduled.**
>
> **Consoles = a preserved option, not a 2026 deliverable.**

This arrangement gives PR3 a credible route to cross-platform deployment and independently measurable intelligence improvements without confusing portable application engineering with training a frontier foundation model, without letting the hardest-to-reach target dictate the design of the core, and without rebuilding an operating system to get a trustworthy agent.

---

## Appendix A. Selected references

- OpenJarvis source: https://github.com/open-jarvis/OpenJarvis
- OpenJarvis documentation: https://open-jarvis.github.io/OpenJarvis/
- OpenJarvis research paper: https://arxiv.org/abs/2605.17172
- CaMeL, "Defeating Prompt Injections by Design" (Debenedetti et al., 2025): https://arxiv.org/abs/2503.18813
- AgentDojo (Debenedetti et al., 2024): https://arxiv.org/abs/2406.13352
- Apple App Intents: https://developer.apple.com/documentation/appintents
- Karpathy `llm.c`: https://github.com/karpathy/llm.c
- llama.cpp: https://github.com/ggml-org/llama.cpp
- Rust: https://www.rust-lang.org/
- Tokio: https://tokio.rs/
- UniFFI: https://github.com/mozilla/uniffi-rs
- cbindgen: https://github.com/mozilla/cbindgen
- cargo-fuzz: https://github.com/rust-fuzz/cargo-fuzz
- Miri: https://github.com/rust-lang/miri
- cargo-deny: https://github.com/EmbarkStudios/cargo-deny
- hashbrown: https://github.com/rust-lang/hashbrown
- Ferrocene (qualified Rust toolchain): https://ferrocene.dev/
- Apple Foundation Models framework: https://developer.apple.com/documentation/foundationmodels
- ML Kit GenAI APIs (Gemini Nano): https://developers.google.com/ml-kit/genai
- Rig: https://github.com/0xPlaygrounds/rig
- Swiftide: https://github.com/bosun-ai/swiftide
- Burn (optional ML): https://github.com/tracel-ai/burn
- Qwen GGUF: https://huggingface.co/Qwen
- PyTorch/PEFT (optional offline training): https://pytorch.org/
- ExecuTorch: https://github.com/pytorch/executorch
- SQLite: https://sqlite.org/
- Cargo: https://doc.rust-lang.org/cargo/
- OpenClaw: https://github.com/openclaw/openclaw
- NullClaw: https://github.com/nullclaw/nullclaw
- ZeroClaw: https://github.com/zeroclaw-labs/zeroclaw
- ZeroClaw crate architecture: https://github.com/zeroclaw-labs/zeroclaw/blob/master/docs/book/src/architecture/crates.md
- Kani (Rust model checker): https://github.com/model-checking/kani
- Proptest: https://github.com/proptest-rs/proptest
- TLA+: https://lamport.azurewebsites.net/tla/tla.html
- Quint: https://github.com/informalsystems/quint
- seL4 verified configurations: https://docs.sel4.systems/projects/sel4/verified-configurations.html
- seL4 Microkit manual: https://docs.sel4.systems/projects/microkit/manual/latest/
- Rust in seL4 userspace: https://docs.sel4.systems/projects/rust/
- Android Virtualization Framework: https://source.android.com/docs/core/virtualization

**Evidence note:** OpenJarvis source conclusions above derive from selective reading of the current repository structure and representative agent, engine, memory and learning files. ZeroClaw conclusions derive from its README and crate-architecture documentation as of 19 October 2026 and must be re-verified before any reuse decision. Platform-model capabilities (Apple Foundation Models, Gemini Nano) are as publicly described in October 2026 and must be re-verified against current vendor documentation at each milestone. Full upstream parity, mobile performance and security properties require dedicated test/audit work.

---

## Appendix B. RFC-001 — Console Cognition Tiers (deferred)

**Status:** Draft, not scheduled. **Purpose:** the design a studio partner would react to, so that a console conversation does not start from zero.

### B.1 Context

Console silicon is sufficient: a jailbroken PS5 has reportedly run llama.cpp on the GPU with a heavily quantized 27B model at roughly 20 tokens/s (unsourced — cite in Appendix A, and omit from any copy shared with platform holders). On retail SDKs, however, there is no GPU inference backend (PS5 AGC, Xbox DX12, Switch 2 NVN have no ggml backend), so a shipping title gets CPU-only inference, and PR3 is a guest in the title's memory and frame budget.

Approximate budgets available to titles (as reported publicly, October 2026): Xbox Series X ≈13.5 GB and Series S ≈8 GB; PS5 ≈12–13 GB; Switch 2 ≈9 GB with 6 CPU cores available to developers. A 1–1.5B Q4 model with a modest KV cache needs ≈1.3 GB; 3B needs ≈2.5 GB.

### B.2 Tiers

| Tier | Where | Does | Why |
|---|---|---|---|
| C0 | Game code | State, rules, what an NPC *can* do | Deterministic, cert-safe |
| C1 | On-console 0.5–1.5B, CPU, 2 pinned cores, grammar-constrained | Intent, barks, short reactions, local memory summaries | <300 ms, works offline, no server cost per utterance |
| C2 | Private server (publisher's, or the player's paired PR3 node) 8B–32B | Long dialogue, planning, director decisions, consolidation | Quality; one model serves many players |

C2 is the default brain; C1 is an accelerator for low-stakes cognition. The on-console model never removes the server. (Console tiers are prefixed `C` to keep them distinct from Rust target tiers and the trust tiers of §7.3.)

### B.3 Host budget contract

Declared by the title at init: memory ceiling, pinned cores, per-frame time slice, model load/unload hooks (reclaim for cutscenes, reload after), suspend/resume behavior, output filter and deterministic fallback for every LLM-driven line.

### B.4 Integration shape

- **Phase A (available now):** `pr3-client-c` compiled under the console SDK, speaking `pr3-protocol` to a C2 node. No Rust on the console.
- **Phase B (partner-funded):** the `pr3-ffi` core build (`pr3-core` as a `no_std + alloc` staticlib) via custom target spec and `-Zbuild-std=core,alloc` (nightly, partner SDK, private CI), driven by the game's C++ through the C ABI; ggml CPU backend built with the console toolchain and supplied as the inference callback. Order: Xbox (GDK is closest to a stock Win32/Rust target) → PS5 → Switch 2.
- **Never in Phase B:** `std` in core, Tokio, sockets or files inside PR3 on the console; threads beyond the host's pinned cores.

### B.5 Non-engineering gates

Devkit/SDK access, TRC/XR/Lotcheck review, platform policy on cross-title user data (relevant to the "game as client of the player's PR3 node" idea), package size on Switch 2 game cards.

### B.6 Trigger to schedule this work

Any one of: a studio with SDK access asks and co-funds the port; a next-generation platform ships an on-device ML/LLM API usable by third-party titles; or a devkit and idle time make a ggml-CPU + `no_std` core experiment on Xbox cheap enough to produce a real datapoint. Absent these, the only console work is keeping `pr3-core` `no_std`-clean and `pr3-client-c` building.

---

## Appendix C. Architecture Decision Records

### ADR-001 — Implementation language for the PR3 core

**Status:** Accepted, 19 October 2026. **Decision:** Rust-first (v1.1/v1.2) over C++20 with C ABI (v1.0).

**Context.** Two architectures were drafted: a C++20 core with a stable C ABI, and a Rust core with direct Cargo integration and an optional C ABI. Additional targets were then raised: embedded and mobile-desktop apps, Emscripten Unreal HTML5 games, and native games on desktop, mobile and consoles.

**Decision drivers (for Rust).**
1. The first host applications are Rust/Tauri; a C++ core would put a C ABI between them and their own engine for no benefit.
2. The benchmark baseline's native layer (OpenJarvis) is Rust crates; porting, diffing and selective reuse are direct.
3. The threat model is dominated by parsing attacker-controlled bytes (prompt injection, tool output, protocol frames, connector responses); safe-by-default parsing is a product feature for a runtime that authorizes consequential actions.
4. `wasm32-unknown-unknown` with a thin JS bridge is first-class; C++→Wasm drags an Emscripten runtime along.
5. Cargo workspaces, lockfiles, `cargo deny` and a unified toolchain across eight-plus targets suit a one-person team far better than CMake.

**Costs accepted (where C++ would have been easier).**
- llama.cpp is C/C++; Rust owns the bindings. Mitigated by the `pr3-llamacpp` strategy (§4.3).
- Toolchain reach: console SDKs, some certified OEM toolchains and exotic MCUs are C/C++-only. Mitigated by the protocol-first reach model and `pr3-client-c` (§4.2, §6.5).
- A single Rust+Unreal Emscripten binary is fragile. Mitigated by compiling the C client with the Unreal fork's own emsdk (§6.6).
- Core ML/MLX/ExecuTorch have native C++/ObjC APIs. Not blocking; llama.cpp and the platform-model shims cover the baseline.

**Alternatives rejected.**
- *C++20 core (v1.0):* right only if in-process LLM inference on consoles or inside a single Unreal HTML5 binary were a day-one requirement. It is not (§1.2, §6.7).
- *Dual cores (C++ for consoles, Rust elsewhere):* two implementations of agent semantics diverge; rejected.
- *C core:* maximal toolchain reach, worst memory-safety story for a parser-heavy runtime; rejected for the core, adopted only for the thin protocol client.
- *Build on ZeroClaw:* a mature Rust agent daemon with strong security engineering and a large community. Rejected as a base because its center is what PR3 must replace — a Tokio-bound runtime that depends on every other crate, a daemon operating model phones forbid, inference only through HTTP providers, and no mobile or embedding story — while its edges (channels, gateway, service management) are what PR3 does not need. Forking would mean removing the runtime and diverging from a fast-moving upstream whose obvious next steps (in-process inference, an eval harness) will be built there anyway. Retained as a landscape comparable (§2.1), a benchmark comparator for tool reliability and security (§8.1), and a per-component reuse source at the adapter layer (§5.1).

**Revisit triggers.** This decision is reopened if (a) in-process console inference becomes a day-one requirement for a signed, funded game integration, or (b) `pr3-core` can no longer be kept `no_std + alloc` and panic-free, which would remove the custom-target console path that makes Rust viable there. Either condition is to be raised as an RFC with evidence, not decided by drift.

### ADR-002 — Trust architecture: seL4 as a preserved tier, not a deliverable

**Status:** Accepted, 19 October 2026. **Decision:** adopt the security constitution of the August 2026 PR3/Samantha plan as PR3's trust architecture; implement it at the `dev` tier on today's operating systems; keep `protected` and `secure` reachable through the `no_std` trusted-side modules; do not build the operating system, shells, VMM or hardware.

**Context.** The August plan proposed PR3 as a seL4-hosted platform with AOSP and Debian as untrusted guests, native seL4 services for identity, vault, policy, capability, audit, update and confirmation, and a seven-person team to build it. v1.2 redefined PR3 as an embeddable personal-AI runtime. The question was what of the August plan survives.

**What is adopted.** Model ≠ authority; the propose/dispose split; the invariants in §7.1; structured intent, validator and execution sequence (§4.6); capability/approval receipts (§7.2); hash-chained audit (§4.4); identity with revocation (§6.3); the three-target ladder as trust tiers (§7.3); narrow formal methods (§12); the claims discipline — security claims are per tier and configuration, never "the whole system is verified".

**What is not adopted.** seL4 as a product, AOSP and Debian guests, custom SystemUI/launcher and Wayland compositor, a VMM, reference hardware and DMA/IOMMU strategy, a dedicated phone, and the team, budget and valuation built around them.

**Drivers.**
1. v1.2's thesis is an embeddable runtime inside existing apps and OSes; an OS is a different project with different users.
2. In the August plan, seL4/VMM/BSP bring-up consumed most of the team and most of the risk register while delivering nothing an embeddable runtime's first users would notice.
3. The August plan's own pivot conditions apply: seL4 is not worth its cost until a deployment requires host-compromise resistance; none does yet.
4. Platform key storage plus phone-side confirmation deliver the user-visible guarantee for consequential actions at the `dev` tier: the model never sees a credential, and a compromised node cannot approve.
5. The `no_std`, sans-I/O, panic-free trusted-side modules are exactly what a seL4 protection domain would run (`rust-sel4`, Microkit), so the option is preserved at zero marginal cost.

**Costs accepted.** The `dev` tier does not protect against a compromised OS kernel and every claim must say so. If a deployment requires that protection, PR3 is one tier away rather than there.

**Revisit triggers.** A funded partner (government, key custody, OEM) requiring host-compromise-resistant policy or keys, together with a chosen reference board; or the `protected` tier proving insufficient for live execution at the M5 gate. Either is raised as an RFC with a budget, never started by drift.

### ADR-003 — `pr3-llamacpp` wraps `llama-cpp-2` for the spike

**Status:** Accepted, 20 October 2026. **Decision:** implement `pr3-llamacpp` on the maintained `llama-cpp-2` Rust bindings, pinned exactly (`=0.1.160`), behind PR3's own small API — `Engine::load`, `Engine::chat` and PR3's `ChatMessage` type — instead of the vendored llama.cpp plus hand-written frozen C shim planned in §4.3.

**Context.** §4.3 plans a frozen C shim so llama.cpp's API churn never reaches PR3. The spike needs Qwen3 running on macOS, the iOS simulator and the Android emulator within days, and `llama-cpp-2` already builds llama.cpp from source with Metal on Apple targets and the NDK toolchain on Android.

**Why this still meets the goal.** The frozen boundary moves from C to Rust: callers see only `pr3-llamacpp`'s types, so a binding upgrade or a later switch to PR3's own shim changes one crate. The crate itself needs no `unsafe` (`#![forbid(unsafe_code)]`); the unsafe surface is the audited third-party binding.

**Costs accepted.** The llama.cpp version follows the binding's releases. Clean builds compile llama.cpp from source (about a minute on Apple silicon), and dev builds optimise `llama-cpp-sys-2` because unoptimised llama.cpp is unusable. The prebuilt libraries and checksummed fetch step of §4.3 item 3 are deferred.

**Revisit triggers.** The binding lags a llama.cpp feature PR3 needs; it cannot be made to build for a supported target; clean-build times hurt CI; or a security review prefers the narrower unsafe surface of PR3's own shim.

---

## Appendix D. Lineage from the PR3/Samantha Master Plan (27 August 2026)

The August plan is the direct ancestor of this document. Its thesis — a trusted execution, identity, memory and policy layer for AI agents, with the model treated as an untrusted proposal generator — is PR3's thesis. What changed is the vehicle: a secure operating platform there, an embeddable runtime here. The plan's §31.5 listed the conditions under which it should pivot to "an Android/Linux agent runtime without a complete OS", a signing vault or an OEM security SDK; v1.2 and v1.3 are that pivot, taken before the costs of the OS route were paid rather than after.

| August plan section | Disposition | Where in v1.3 |
|---|---|---|
| §1 Thesis: trusted execution, identity, memory, policy layer | **Reused** as the trust architecture, not as an OS | §3, §7, ADR-002 |
| §2, §9, §10 seL4 + AOSP + Debian, VMM, SystemUI, compositor | **Dropped**; seL4 kept as a preserved tier | §7.3, ADR-002 |
| §3.2 "What PR3 is not" | **Reused** and extended | §1.2 |
| §4 Why now: OS vendors moving into agent infrastructure; differentiate below the assistant layer | **Reused** | §2 |
| §5 Humane/Rabbit lessons | **Reused** as a principle | §3 "stay useful when the model is gone" |
| §6 Enterprise-first wedge and beachheads | **Not carried over** (commercial) | — |
| §7 Workflows | **Partially reused**: controlled communications shape §4.6; the executive briefing is the node's proactive job | §4.6, §6.3 |
| §8 Security philosophy, threat model, invariants, claims discipline | **Reused** | §3, §7.1, §7.3 |
| §11 Native services: identity, vault, policy, capability, audit, update, confirm | **Reused** as `no_std` modules plus the `pr3-vault` adapter; update verifier reduced to model/policy-pack rollback; confirm realized as phone-side confirmation | §4.1, §4.2, §6.3, §7.2, §12 |
| §12 Structured intent and execution sequence | **Reused**, adapted to `mail.send` | §4.6 |
| §13 Memory ownership, data separation, disclosure policy | **Reused** | §4.3, §4.4 |
| §14 Connector model | **Reused** | §4.7 |
| §15 Hardware strategy, DMA/IOMMU | **Dropped** (relevant only to a `secure` tier) | ADR-002 |
| §16 `pr3-dev` / `pr3-protected` / `pr3-secure` | **Reused** as trust tiers | §7.3 |
| §17 Repository structure | **Adapted**: trusted-side modules inside `pr3-core`, `pr3-vault`, `pr3-protocol` | §4.2 |
| §18 Test layers, formal methods, release channels | **Reused** | §12 |
| §19 Open-core licensing | **Adapted**: Apache-2.0 open-source core | §15 |
| §20 Pricing | **Not carried over** (commercial) | — |
| §21 Go-to-market and design-partner package | **Not carried over** (commercial) | — |
| §22 Competitive positioning | **Partially reused** (platform vendors as agent infrastructure) | §2 |
| §23 Moat | **Not carried over** (commercial) | — |
| §24–25 Roadmap and 90-day plan | **Dropped**, superseded | §11 |
| §26–30 Team, budget, fundraising, cap table, scenarios | **Not carried over** (commercial) | — |
| §31 KPIs and gates | **Reused**: reliability targets and gate discipline | §14 |
| §31.5 Pivot conditions | **The clause this document executes** | §0, ADR-002 |
| §32 Risk register | **Partially reused** (trusted-path spoofing, scope creep, "users do not pay for security") | §13 |
| §33 Compliance roadmap | **Dropped**: PR3 is an open-source research project | — |
| §34 Naming | **Not carried over**; the trust tiers keep the `dev` / `protected` / `secure` names | §7.3 |
| §35 Investor pitch | **Not carried over** (commercial) | — |
| §36 Final recommendation | Superseded | §16 |

The August plan's commercial sections — go-to-market, pricing, design partners, team and financing — are not carried over; PR3 is an open-source research project.
