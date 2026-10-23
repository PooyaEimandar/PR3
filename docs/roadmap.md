# Roadmap

Part of the [PR3 documentation](README.md). Section numbers are stable; code comments cite them as “docs Sec. N”.

## 11. Delivery Roadmap, Epics & Issues

Dates and effort are planning estimates for an experienced founder using AI coding tools plus focused specialist review, **not delivery guarantees**.

**How the gates fit together.** The release gates in [Sec. 8.5](benchmarks.md#85-proposed-acceptance-gates-targets-not-measured-results) are the pass criteria. Each milestone gate below restates the release gate it must meet plus milestone-specific deliverables; the [Sec. 14](#14-success-metrics--gono-go-gates) go/no-go questions are the scope decisions taken on those results. Where they differ, [Sec. 8.5](benchmarks.md#85-proposed-acceptance-gates-targets-not-measured-results) governs.

| Milestone | Release gate ([Sec. 8.5](benchmarks.md#85-proposed-acceptance-gates-targets-not-measured-results)) | Go/no-go ([Sec. 14](#14-success-metrics--gono-go-gates)) | Release channel ([Sec. 12](quality-gates.md#12-build-cicd--quality-gates)) |
|---|---|---|---|
| M0 | Core alpha | After M0 | `dev` |
| M1 | - (internal; M1 gate below) | - | `dev` |
| M2 | Local MVP (Android, desktop); Hybrid beta | After M2 | `dev` |
| M3 | Security & quality beta | After M3 | `dev` |
| M4 | Local MVP on iOS | - | `dev` |
| M5 | Actions beta | After M5 | `dev` |
| M6 | - | - | `stable` |

### Spike build order - simulators and macOS first

The first 6–8 weeks build the spike in small, reviewable steps rather than milestone by milestone. Every step has the same gate: it runs in the macOS app, on the iOS simulator and on the Android emulator, and its tests pass. Work the spike does not need - `pr3-client-c`, Unreal/Emscripten, the full protocol v0 and formal methods beyond the policy core - waits until the spike's results are in.

| Step | Builds | Milestone epics it advances | Status |
|---|---|---|---|
| 1 | Cargo workspace; `pr3-core` as a sans-I/O `no_std` `step()` machine; Tauri 2 app on macOS, iOS simulator and Android emulator with a chat screen driven by `step()` and a mock model | M0-E3, M1-E1 | done |
| 2 | Qwen3 through `pr3-llamacpp`: macOS app first, then simulator and emulator | M1-E3, M2-E1 | done (v1.8) |
| 3 | Agent loop with structured tool calls and OpenJarvis-compatible traces | M1-E1, M1-E2 | done |
| 4 | SQLite memory with provenance | M1-E5 | done (v1.11) |
| 5 | Trusted side for one action (sandbox email): intent → policy → phone confirmation → one-use capability → audit | M1-E7, M2-E3 | done (v1.13, v1.14) |
| 6 | The macOS app as the user's node: pairing and handoff | M2-E2, M2-E3, M2-E4 | done (v1.15) |
| 7 | Voice notes | M2-E7 | done (v1.17) |
| 8 | AgentDojo harness: PR3 against a plain tool-calling harness with the same model | M0-E1, M3-E5 | done (v1.16); more suites to run |

The spike ends with the 30-day user-zero diary and the AgentDojo result; together they decide whether the full roadmap continues as written.

### Milestone M0 - Benchmark & Architecture Spike (Weeks 1–3)

**Deliverables:** upstream OpenJarvis version pin, representative workflows, evaluation harness, platform feasibility matrix, Rust workspace with `no_std` core and CI check targets, `pr3-protocol` v0 schema, `pr3-client-c` skeleton, [ADR-001](adr/001-core-language.md).

**Epics/issues:**
- M0-E1: freeze OpenJarvis baseline and the AgentDojo version, record model/quantization, create reproducible container/desktop environment.
- M0-E2: implement 30–50 task smoke-suite and evaluation taxonomy.
- M0-E3: `pr3-core` as sans-I/O `no_std + alloc` crate; CI `cargo check --no-default-features` on `thumbv7em-none-eabihf` and `wasm32-unknown-unknown`; forbid-unsafe and panic-site allowlist checks; builds for macOS/Linux/Windows and a minimal Android ARM64 host.
- M0-E4: `pr3-protocol` v0 (CBOR framing, versioning, handoff/sync message families); `pr3-client-c` skeleton compiling on Linux. The freestanding and Emscripten builds serve deferred targets and move to M4-E3 with the Unreal sample.
- M0-E5: dependency inventory (`cargo deny`, SBOM); threat model and security invariants ([Sec. 7.1](security.md#71-threat-model)) as a checked-in document; [ADR-001](adr/001-core-language.md) and [ADR-002](adr/002-trust-architecture.md) ([the ADRs](adr)); the intent envelope, capability-receipt and audit-record schemas land in `pr3-protocol` v0 - they are protocol, not implementation.

**Gate:** identical core sources compile `no_std` and `std` across targets and reproduce deterministic mock-agent behavior; protocol round-trips between a Rust runtime and the C client.

### Milestone M1 - Offline Intelligence Kernel (Weeks 4–8)

**Deliverables:** bounded agent state machine, structured tools, `pr3-llamacpp` with prebuilt libraries for desktop and Android, SQLite memory, Foundation Models spike on macOS, PR3-Personal corpus, CLI and integration tests.

Scope note: M1 was trimmed in v1.4 to fit five weeks. iOS prebuilts move to M4 (with the iOS host), the Gemini Nano spike and Android Keystore move to M2 (with the Android host), and Kani harnesses move to M3; Proptest covers the trusted-side state machines until then.

**Epics/issues:**
- M1-E1: task state lifecycle, cancellation, retry, checkpointing in `pr3-core`/`pr3-agent`.
- M1-E2: tool schema, validation, registry, authorization callbacks; cargo-fuzz targets for every parser.
- M1-E3: **`pr3-llamacpp` - the milestone's headline risk:** vendored pinned submodule, frozen C shim, bindgen on the shim, prebuilt static libs for macOS, Linux, Windows and Android arm64 with the checksummed fetch step ([Sec. 4.3](architecture.md#43-inference-strategy)), feature-per-backend, safe public API, ASan/UBSan + fuzz on the boundary.
- M1-E4: `pr3-platform-llm` spike: Apple Foundation Models on one macOS 26 Apple Intelligence Mac, behind the shared `InferenceBackend` trait with declared `ModelCapabilities`.
- M1-E5: SQLite store (`pr3-memory-sqlite`), source provenance, FTS5, migration tests.
- M1-E6: error and crash handling; Miri on core; sanitizers/fuzzing CI.
- M1-E7: trusted-side modules - intent envelope and validator, policy decision, capability issuance/expiry, hash-chained audit - with Proptest properties ([Sec. 12](quality-gates.md#12-build-cicd--quality-gates)); `pr3-vault` adapter over desktop key storage (macOS Keychain, Windows CNG/TPM, Linux Secret Service); CI check that agent/inference modules never become dependencies of trusted-side modules.
- M1-E8: grow the M0 smoke suite into the PR3-Personal corpus ([Sec. 8.3](benchmarks.md#83-ai-benchmarks-and-external-participation)) - 200 consent-safe synthetic tasks with a held-out split - frozen before the M2-E6 and M3-E5 runs.

**Gate:** the M0 smoke suite, run fully offline on desktop, reaches ≥75% task success (provisional; fixed before the final run per [Sec. 8.5](benchmarks.md#85-proposed-acceptance-gates-targets-not-measured-results)) with traceable tool calls on the bundled backend and on Foundation Models on macOS; every consequential action in the suite passes through envelope → intent → policy → capability → audit.

### Milestone M2 - Android + Desktop Hybrid (Weeks 9–14)

**Deliverables:** Android host; macOS/Linux `pr3-node`; authenticated device pairing; handoff, resume and offline fallback; platform-vs-bundled decision; voice notes.

**Epics/issues:**
- M2-E1: direct Tauri Rust integration (Kotlin/JNI or UniFFI only for platform-specific capabilities); on-device 1B–4B inference benchmarks, CPU first, Vulkan measured per device class.
- M2-E2: `pr3-node` with local 8B–32B reference model; proactive/background jobs run here.
- M2-E3: pairing, identity registration and revocation, authenticated encrypted transport and replay control; **phone-side trusted confirmation** of node-prepared actions with biometric approval and vault-signed receipts ([Sec. 6.3](deployment.md#63-private-hybrid-mobile--desktop)); `pr3-vault` on Android Keystore/StrongBox (moved from M1).
- M2-E4: task-state delegation protocol with a TLA+ (or Quint) model checked under node loss, message duplication, reordering and replay; interruption/duplicate-action tests derived from the model; `pr3-client-c` exercised as a second client type.
- M2-E5: notification through the publisher push relay with sealed wake-ups, and UnifiedPush ([Sec. 6.3](deployment.md#63-private-hybrid-mobile--desktop)); power profiling, deterministic-pre-filter-before-LLM policy; onboarding usable before the optional model download.
- M2-E6: Gemini Nano Prompt API backend on one supported Android device (moved from M1), then the **platform model vs bundled GGUF benchmark** on PR3-Personal tasks; decide per platform whether the bundled download is default, optional or omitted.
- M2-E7: voice notes in the Android and desktop apps - platform speech-to-text, replies as text or platform speech by the user's choice; conversation → memory proposals.

**Gate:** Local MVP on Android and desktop, and Hybrid beta ([Sec. 8.5](benchmarks.md#85-proposed-acceptance-gates-targets-not-measured-results)): same task works locally, delegates privately when authorized, and recovers from node loss without duplicate actions; platform-vs-bundled decision documented.

### Milestone M3 - Privacy Memory & Adaptive Learning (Weeks 15–20)

**Deliverables:** encrypted personal memory, provenance, limited cross-device synchronization, trace-based routing (including backend-class routing) and held-out evaluation.

**Epics/issues:**
- M3-E1: SQLCipher/keychain/keystore integration with key recovery design.
- M3-E2: append-only sync log, conflict reconciliation and tombstones.
- M3-E3: trace evaluation, policy routing across platform/bundled/node, rollback and held-out test harness.
- M3-E4: privacy egress tests, prompt-injection attacks, hostile-protocol-frame tests and redaction audit; Kani harnesses on the policy, capability and approval-receipt state machines (moved from M1, [Sec. 12](quality-gates.md#12-build-cicd--quality-gates)).
- M3-E5: compare PR3 vs OpenJarvis under matched scenarios; publish honest report.
- M3-E6: personal model - style profile and preference model with held-out evaluation ([Sec. 4.5](architecture.md#45-learning-architecture), [Sec. 8.2](benchmarks.md#82-evaluation-dimensions)); a 30-day user-zero diary (daily: time saved, confirmations asked, drafts accepted) as gate evidence.

**Gate:** Security & quality beta ([Sec. 8.5](benchmarks.md#85-proposed-acceptance-gates-targets-not-measured-results)) met, adaptive routing beats fixed routing on held-out tasks ([Sec. 8.1](benchmarks.md#81-equal-conditions-openjarvis-comparison)), critical privacy gates passed, and the user-zero diary (M3-E6) is recorded. If adaptive routing shows no held-out gain, the fixed policy ships and the gate records learning as not yet earning its cost - a scope decision under [Sec. 14](#14-success-metrics--gono-go-gates), not a pass.

### Milestone M4 - iOS & SDK (Weeks 21–28)

**Deliverables:** iOS host, packaged native library with UniFFI Swift bindings, hardened `pr3-client-c`, Unreal C++ sample (native and HTML5 via the client), memory/CPU profiling and documented toolchain support.

**Epics/issues:**
- M4-E1: signed iOS framework / Swift bindings; Foundation Models backend production-ready; iOS arm64 + simulator llama.cpp prebuilts (moved from M1) and the bundled GGUF Metal path per the M2 decision.
- M4-E2: platform background execution and notification design on iOS.
- M4-E3: C ABI versioning and smoke tests in a minimal C host and Unreal C++ host; select and record the Unreal HTML5 fork (repository and commit) and the emsdk version it pins - every reference to "the Unreal fork" in this document means that record; `pr3-client-c` fuzzed and sanitized, console-style freestanding build, pinned-emsdk CI job (both moved from M0); **console targets remain out of scope** ([RFC-001](rfc/001-console-cognition-tiers.md) maintained as RFC).
- M4-E4: asset/model packaging and download integrity checks.
- M4-E5: release artifacts, SBOM, docs and reproducible examples.
- M4-E6: voice on iOS; spoken replies (platform voices) on iOS and Android; whisper.cpp fallback on desktop.

**Gate:** Local MVP on iOS ([Sec. 8.5](benchmarks.md#85-proposed-acceptance-gates-targets-not-measured-results)); fully offline reference workflow on Android/iOS/desktop; cross-platform SDK examples build; Unreal HTML5 sample links with the fork's emsdk.

### Milestone M5 - Consequential Actions in Sandbox Mode (Weeks 29–36)

**Deliverables:** end-to-end consequential workflows through sandbox connectors - outbound mail, calendar changes and a simulated payment provider - with phone-side confirmation, one-use capabilities, idempotent execution and audit; proactive workflows on `pr3-node`; a red-team report.

**Epics/issues:**
- M5-E1: sandbox connectors with signed manifests ([Sec. 4.7](architecture.md#47-connector-and-plugin-model)) for mail, calendar and a simulated payment provider.
- M5-E2: deterministic per-action limits, expiry, kill switch, idempotency keys and reconciliation against the sandbox provider's status.
- M5-E3: confirmation UX for each action class on Android and iOS; *no action* as a valid, audited outcome ([Sec. 9.1](applications.md#91-rules-for-applications-that-take-consequential-actions)).
- M5-E4: proactive workflows on `pr3-node` with explanations and notification throttling through the push relay ([Sec. 6.3](deployment.md#63-private-hybrid-mobile--desktop)).
- M5-E5: red-team pass - a lying node, an over-reaching plugin, a spoofing UI, replayed receipts, and prompt injection through mail and documents ([Sec. 12](quality-gates.md#12-build-cicd--quality-gates)).
- M5-E6: decide whether live execution needs the `protected` tier ([Sec. 7.3](security.md#73-trust-tiers---dev-protected-secure)) and, for each candidate live provider, where its credential and execution adapter live ([Sec. 6.3](deployment.md#63-private-hybrid-mobile--desktop)).

**Gate:** Actions beta ([Sec. 8.5](benchmarks.md#85-proposed-acceptance-gates-targets-not-measured-results)); red-team findings closed or accepted with a documented reason; zero critical authorization failures; the `protected`-tier and credential-location decisions recorded.

### Milestone M6 - Public Open Source Release & Expansion (Post-M5)

Release signed SDKs, `pr3-client-c`, portable samples, benchmark report, contribution guidelines, versioned protocol and C ABI, and the supported-target matrix ([Sec. 6.5](deployment.md#65-what-ships-where)). Expand hardware targets only after measured need; revisit console scope per [RFC-001](rfc/001-console-cognition-tiers.md)'s trigger. The optional WebGPU digital face ([Sec. 6.6](deployment.md#66-browser-wasm-and-unreal-html5-integration)) is post-M5 work.

## 14. Success Metrics & Go/No-Go Gates

**Technical North Star:** completed, correct and authorized real-world tasks *per joule and per user intervention*, with privacy violations treated as hard failures.

Track monthly: AgentDojo attack success rate, offline task success, matched OpenJarvis comparison delta, private hybrid fallback success, average and p95 response time, joules/task, mobile RAM, model download footprint, platform-vs-bundled routing share, tool-schema error rate, unauthorized action rate, recovered task rate, user corrections per task, drafts accepted without edits, and approval-time trend.

Reliability targets carried into every release gate: unauthorized high-risk execution 0; capability replay acceptance 0; secret exposure incidents 0; incorrect object or recipient resolution ≈0; low-risk workflow success ≥95% by M4; recovery from node loss ≥99%.

**Go/no-go gates:**

- **After M0:** Can the architecture build `no_std` and `std` on supported targets, round-trip the protocol to the C client, and benchmark against OpenJarvis fairly?
- **After M2:** Can the phone truly operate offline (platform or bundled model), recover gracefully from private-node unavailability, and has the bundling decision been made on evidence?
- **After M3:** Does the security result hold on AgentDojo ([Sec. 8.5](benchmarks.md#85-proposed-acceptance-gates-targets-not-measured-results)), and does learning produce reproducible quality gains or at least parity with materially better efficiency?
- **After M5:** Do consequential workflows run end to end in sandbox mode with zero critical authorization failures, and does any live provider justify the `protected` tier?

If any gate fails, reduce product scope or reuse upstream components rather than automatically expanding engineering effort.
