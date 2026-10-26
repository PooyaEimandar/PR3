# Overview

Part of the [PR3 documentation](README.md). Section numbers are stable; code comments cite them as “docs Sec. N”.

## 1. Executive Summary

**PR3** is an open-source, privacy-first, portable personal AI runtime designed to work **independently and offline on mobile and desktop**, or **cooperatively in a private hybrid configuration**. It is intended as an embeddable intelligence engine rather than another cloud-dependent chat wrapper. The same core should be integrable into personal applications, OEM software, automotive infotainment, game engines, and constrained devices when their operating-system and model-runtime capabilities permit - and where they do not, those hosts speak PR3's protocol to a PR3 runtime that runs elsewhere.

PR3 is **inspired by OpenJarvis**, particularly its division of intelligence, inference engines, agent execution, memory, evaluations, and trace-driven learning. PR3 is not proposed as a literal line-for-line rewrite or an unmodified fork. It will implement a lean, independently maintained **Rust intelligence core**, integrated directly as a Cargo dependency in Rust/Tauri hosts, exported through an **optional C ABI** for Swift/Kotlin/Unreal/OEM applications, and reached from everything else through a **versioned wire protocol and a small C client**. It will reuse proven native inference infrastructure, use the operating system's on-device model where one exists, and benchmark openly against OpenJarvis.

**Research hypothesis:** with the same model, an agent running on PR3 executes *no unauthorized consequential action and leaks no data through tool arguments* under standard prompt-injection attacks (AgentDojo), where a plain tool-calling harness does not, while keeping benign-task utility close to that harness and adding cross-device continuity. **This is a target, not an established result.** PR3 claims it only when reproducible benchmarks show it under equal models, datasets, tools and hardware ([Sec. 8](benchmarks.md#8-benchmarking)).

**Domain applications** build on PR3 through its SDK, plugins and connectors; PR3 itself is application-neutral ([Sec. 9](applications.md#9-domain-applications)).

### 1.1 Project boundaries

- **PR3 Core:** open-source engine, portable SDK, memory interfaces, model orchestration, tool contracts, policy enforcement, traces and evaluations.
- **PR3 Protocol & Client:** the versioned wire contract between PR3 runtimes and non-Rust hosts, plus a dependency-free C11 client library.
- **PR3 Apps:** open-source reference mobile/desktop clients - voice first, text optional - and the private-node coordinator (`pr3-node`).
- **Domain applications:** built outside this repository on PR3's SDK, plugins and connectors ([Sec. 9](applications.md#9-domain-applications)).
- **Research/training:** separate development pipeline; the distributed mobile/desktop product **does not require Python, PyTorch, or cloud model APIs at runtime**.

### 1.2 Non-goals for the initial release

- Training a frontier foundation model from scratch.
- Claiming AGI or universal superiority over Qwen, OpenJarvis, or commercial frontier models.
- Including PyTorch/LibTorch in every mobile or embedded binary.
- Guaranteeing 24/7 autonomous operation on iOS or Android while the app is suspended.
- Letting an LLM directly access private keys, bypass permissions, or execute consequential actions without authorization.
- **In-process LLM inference on game consoles or inside a single Unreal HTML5 binary** (deferred; see [Sec. 6.7](deployment.md#67-game-consoles---deferred-path-preserved) and [RFC-001](rfc/001-console-cognition-tiers.md)).
- **Owning the entire inference stack on devices whose OS already provides an on-device model.** PR3 bundles a model for coverage and control, not as the only path.
- **A full operating system, a phone or desktop platform on its own microkernel, custom shells or compositors, or dedicated hardware** - the long-term target of an earlier plan for a pure AI OS. A verified microkernel survives only as a preserved deployment tier ([Sec. 7.3](security.md#73-trust-tiers---dev-protected-secure), [ADR-002](adr/002-trust-architecture.md)).
- **Acting as the user toward others.** PR3 may draft in the user's voice, but nothing is sent, said or signed in their name without confirmation or a pre-approved scope.
- **A cloud-hosted personal model, a photoreal avatar, or anyone's likeness** other than the user's own, by their choice.

### 1.3 The architecture in one paragraph

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
> **Consoles = a preserved option, not a 2026 deliverable.**

This arrangement gives PR3 a credible route to cross-platform deployment and independently measurable intelligence improvements without confusing portable application engineering with training a frontier foundation model, without letting the hardest-to-reach target dictate the design of the core, and without rebuilding an operating system to get a trustworthy agent.

## 2. Thesis & Positioning

Most personal-agent offerings emphasize capabilities, integrations, and cloud-based reasoning. Platform owners now ship free on-device models and OS agent hooks - Apple Foundation Models and App Intents, Gemini Nano and Android AppFunctions - each for its own platform. Fast-moving open-source assistants (OpenClaw, ZeroClaw) ship daemons with many channels and broad tool access. Competing head-on with either loses, and a few points of task accuracy over OpenJarvis is not a reason anyone adopts a runtime.

**Positioning:** *the layer underneath - an embeddable runtime that decides what an agent may do, owned by the user, portable across devices and operating systems.* The model is SQLite: embedded, in-process, portable, and boring in a way people trust.

- **Platform models are backends; OS hooks are connectors.** Apple Foundation Models and Gemini Nano sit behind the inference trait ([Sec. 4.3](architecture.md#43-inference-strategy)); App Intents and AppFunctions are action surfaces behind typed connectors ([Sec. 4.7](architecture.md#47-connector-and-plugin-model)). PR3 adds what no single platform will: one policy, memory and audit layer across all of them.
- **Cross-device is the user-visible feature.** The phone confirms what the desktop or node prepared ([Sec. 6.3](deployment.md#63-private-hybrid-mobile--desktop)), and memory follows the user across Apple, Android, Windows and Linux - something platform owners structurally do not offer across each other's systems.
- **Security is the measurable claim.** The headline result is authorization under attack - no unauthorized consequential actions and no tool-argument exfiltration with the same model where plain harnesses fail ([Sec. 8.1](benchmarks.md#81-equal-conditions-openjarvis-comparison)) - not a task-accuracy delta.
- **A private model of you, voice first.** You talk; PR3 answers in text or speech, remembers, drafts like you and learns your routine choices - on your own devices, never acting as you without your say ([Sec. 4.5](architecture.md#45-learning-architecture)) and you own your data.

### 2.1 Landscape and related work

| Project | Relevant strengths | PR3 differentiating work to validate |
|---|---|---|
| [OpenJarvis](https://github.com/open-jarvis/OpenJarvis) | Local-first research platform; agent implementations; evaluation, energy/cost measurement; trace-driven learning; desktop app | Native mobile runtime, optional in-process inference, zero mandatory Python at runtime, private device handoff and repeatable comparison |
| [ZeroClaw](https://github.com/zeroclaw-labs/zeroclaw) | Single-binary Rust agent daemon; trait-based kernel ABI (`zeroclaw-api`); ~20 LLM providers over HTTP (Ollama for local); 30+ channels; supervised autonomy with OS sandboxes and cryptographic tool receipts; WASM plugin host; hardware control on Pi/ESP32; MIT/Apache dual license; large, active community (record dated star/contributor counts at each refresh) | Embeddable library rather than daemon; in-process inference rather than provider-over-HTTP; Android/iOS targets; `no_std` core free of Tokio; evaluation and trace-driven learning loop. ZeroClaw is the closest engineering comparable and a reuse source at the adapter layer ([Sec. 5.1](related-work.md#51-zeroclaw-reuse-candidates-at-the-adapter-layer)), not a base |
| [OpenClaw](https://github.com/openclaw/openclaw) | Broad personal assistant ecosystem, channels, tools, automation | Smaller embeddable SDK, device-local execution and model-agnostic benchmarked cognitive behavior |
| [NullClaw](https://github.com/nullclaw/nullclaw) | Small Zig runtime, pluggable providers, multiple channels, sandbox/security controls | Cross-device learning and personal-intelligence continuity, explicit native mobile/OEM SDK evaluation |
| [Hermes Agent](https://github.com/NousResearch/hermes-agent) | Skills, persistent execution and experience capture | Portable local-first runtime and objectively evaluated learning policy |
| Platform on-device models (Apple Foundation Models, Gemini Nano) | Free, zero-footprint, OS-integrated inference with guided generation/tool calling | Not competitors: PR3 treats them as backends. Differentiation is user-owned memory, policy, hybrid node and cross-platform continuity on top of them |
| OS agent hooks (Apple App Intents, Android AppFunctions) | System-wide action surfaces with deep OS integration, one platform each | Not competitors: PR3 uses them as connectors ([Sec. 4.7](architecture.md#47-connector-and-plugin-model)) and adds cross-platform policy, user-owned audit and device-bound approval on top |
| [CaMeL](https://arxiv.org/abs/2503.18813) (research, 2025) | Prompt-injection defense by design: control flow comes from the trusted user query, so untrusted data cannot change it; capabilities enforce policy on tool calls. 77% of AgentDojo tasks solved with provable security, against 84% undefended | Nearest research neighbour ([Sec. 2.3](#23-research-positioning)). PR3 makes the idea a portable runtime with device-bound approvals and a formally checked policy core |
| Mobile on-device AI apps | Native mobile inference and conversation | Durable agent state, privacy governance, tested hybrid inference and proactive domain workflows |

The comparison is a research baseline; feature coverage, upstream releases and measured performance must be refreshed at each PR3 milestone.

### 2.2 Differentiators requiring proof

1. **Portable binary integration:** stable C ABI, configurable allocator, optional feature modules; build against multiple OS and OEM toolchains - and a protocol client for toolchains the runtime cannot target.
2. **True local autonomy:** on-device model plus agent, tool calls, local memory and policy checks, with offline-mode test evidence.
3. **Private hybrid and cross-device continuity:** authenticated task delegation to a user-controlled desktop/private node, minimizing data transfer; the phone confirms what other devices prepare; memory follows the user across operating systems; fully functional local fallback for supported workflows.
4. **Outcome-based intelligence:** trace evaluation, adaptive selection of model/tools/workflows and regression-tested policy promotion.
5. **Verifiable privacy:** identifiable data movement, deny-by-default network/tool policies, independent cryptographic key ownership and auditable approval flows.
6. **Domain extensibility:** PR3 is application-neutral; domain applications are extensions built on it ([Sec. 9](applications.md#9-domain-applications)), never dependencies of the core.

### 2.3 Research positioning

The nearest research neighbour is **CaMeL** ("Defeating Prompt Injections by Design", Debenedetti et al., 2025). Like PR3, it separates what the model proposes from what is allowed to run: control flow is derived from the trusted user request, so untrusted data retrieved later cannot change which tools run, and capabilities stop private data from leaving through unauthorized flows. Evaluated on AgentDojo, it solves 77% of tasks with provable security, against 84% for an undefended system.

CaMeL is a research system. PR3's contribution is to make the propose/dispose split a portable runtime:

- structured intents with runtime-stamped envelopes, so the same core runs in apps, games, browsers and embedded hosts ([Sec. 4.2](architecture.md#42-rust-workspace-host-api-c-abi-and-protocol-client), [Sec. 4.6](architecture.md#46-structured-intent-and-execution-sequence));
- approvals bound to the user's own devices - phone-side confirmation and vault-signed, one-use capabilities ([Sec. 6.3](deployment.md#63-private-hybrid-mobile--desktop), [Sec. 7.2](security.md#72-required-controls));
- a policy core checked with Kani and a handoff protocol checked in TLA+ ([Sec. 12](quality-gates.md#12-build-cicd--quality-gates));
- evaluation on the same suite, AgentDojo, so results compare directly with CaMeL's published numbers ([Sec. 8.1](benchmarks.md#81-equal-conditions-openjarvis-comparison)).

## 3. Guiding Engineering Principles

- **Less is more:** every mandatory dependency, abstraction and background service must justify its cost.
- **Rust-first core:** stable Rust toolchain pinned per release, plus a separately pinned nightly used only by the verification jobs that require it (Miri, cargo-fuzz, sanitizers); safe Rust by default; `unsafe` confined to two audited crates (`pr3-llamacpp`, `pr3-ffi`) and to a single `sys` module in each of `pr3-platform-llm`, `pr3-vault` and `pr3-speech` for platform FFI, enforced by CI ([Sec. 4.2](architecture.md#42-rust-workspace-host-api-c-abi-and-protocol-client)).
- **Sans-I/O core:** `pr3-core` is a pure state machine - `step(input) -> actions`. No async runtime, no clock, no filesystem, no sockets, no threads inside it. Adapters drive it. This one property yields the Wasm target, the console path and the C ABI's polling model for free.
- **`no_std + alloc` always, `std` additive:** the core and every interface crate compile without `std`; the `std` feature only adds convenience impls, never behavior or types.
- **Runtime where Rust runs, protocol everywhere else:** do not let the lowest-common-denominator target dictate the core language. Hosts that cannot run the Rust runtime (console SDKs, a game's Emscripten build, MCUs, TEEs) use `pr3-client-c` over `pr3-protocol`.
- **Direct Rust SDK first, C ABI second:** an idiomatic Cargo crate for Tauri and other Rust apps; a stable, versioned C ABI (and UniFFI bindings for Swift/Kotlin) as an optional distribution target. No Rust types, panics or unwinding may cross the C ABI.
- **Platform model where it exists, bundled model where it doesn't:** inference is model-neutral; Apple Foundation Models and Gemini Nano are first-class backends alongside bundled GGUF and the private node. The bundled download is optional.
- **Deterministic first, LLM second:** on battery-constrained devices the LLM runs only after deterministic code has decided something is worth explaining; it is never the first filter.
- **Host-owned resources:** host hooks provide allocator, monotonic clock, entropy, storage, networking, scheduling and device APIs.
- **Core/adapter separation:** no direct Apple, Android, Windows, Linux, SQLite or model-specific headers inside the minimal core.
- **No blocking host loop:** asynchronous/pollable state machine; never block the UI or game render thread for model inference.
- **Privacy defaults:** inference stays local unless the user permits connection to a specific private node; external providers are strictly opt-in and not necessary for basic operation.
- **Model ≠ authority:** every model output - local, platform or node - is an untrusted proposal. Tool responses, emails, web pages, retrieved memory, protocol frames and model outputs are all untrusted input; every consequential action passes through deterministic authorization, and the policy engine never delegates its final decision to an LLM.
- **Propose/dispose split inside the runtime:** the untrusted side (model, retrieved context, tool outputs, UI) proposes; the trusted side (intent validation, policy, capability issuance, vault access, audit, confirmation) disposes. On today's OSes both sides share a process; the boundary is kept explicit so the trusted side can later move behind platform-protected storage or into a protection domain on a verified microkernel without changing semantics ([Sec. 7.3](security.md#73-trust-tiers---dev-protected-secure)).
- **Stay useful when the model is gone:** PR3 must keep working when the provider is unavailable, the user is offline, a subscription lapses, a vendor changes terms, or the AI layer is disabled. Deterministic engines and local memory do not depend on inference.
- **Deterministic consequential values:** amounts, limits, recipients and any other value a consequential action depends on are computed or resolved by validated non-LLM code.
- **Evidence before claims:** collect latency, energy, accuracy, security failures and task outcomes continuously.
