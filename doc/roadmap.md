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
| 7 | Voice notes | M2-E7 | done (v1.17, v1.18) |
| 8 | AgentDojo harness: PR3 against a plain tool-calling harness with the same model | M0-E1, M3-E5 | done (v1.16); all four suites (v1.25) |

The spike ends with the 30-day user-zero diary and the AgentDojo result; together they decide whether the full roadmap continues as written.

### Implementation status and next work (2026-11-12)

“Done” in the spike table means that slice exists; it does not close the full milestone or certify its release gate. **Implemented** means code and regression evidence exist, **partial** means only part of the epic exists, and **planned** means the deliverable still needs implementation or measured evidence. Keep this inventory and its evidence links current when closing work.

| Area | Status and evidence | Still needed |
|---|---|---|
| Daily workflow | Partial: [task lifecycle, Stop/retry and checkpoint tests](../app/pr3-app/src-tauri/src/task.rs), [capture/resume/approved sandbox action and cancellation regressions](../app/pr3-app/src-tauri/src/host.rs), [Personal-200 v3 results](../bench/personal/evidence/2026-11-11-v3-report.md) | Ground email drafts in recorded facts; avoid unnecessary clarification and wrong-field recall after deletion; general semantic redaction beyond the implemented category/field vocabulary; provider reconciliation and distributed recovery without repeating effects; broader real-device workflow evidence |
| Trustworthy status | Partial: [runtime completion replies and recorded results](../app/pr3-app/src-tauri/src/task/result.rs), explicit Finish, Needs attention and Outcome unknown; failed-write recovery with Retry saving | Broader multilingual/no-tool claim handling; measure buffered-reply latency and utility |
| Memory | Partial: [atomic replacement and version retirement](../crate/pr3-memory-sqlite/src/lib.rs), [local semantic/multilingual correction review](../app/pr3-app/src-tauri/src/memory/semantic.rs), [bounded original-task-history retrieval](development.md#original-task-history-retrieval), provenance | Semantic retrieval over the full store; held-out multilingual accuracy and mobile latency; portable encrypted backup/replacement-device recovery and Windows/Linux key custody (M3-E1); larger-store and physical-device sync validation (M3-E2) |
| Private hybrid | Partial: [pairing and inference](../crate/pr3-hybrid/src/lib.rs), [paired memory/task continuity, ownership and tombstones](../crate/pr3-memory-sqlite/src/continuity.rs), [app coordination](../app/pr3-app/src-tauri/src/continuity.rs) | Physical Android/computer acceptance; replacement-device enrollment, background jobs, formal loss/replay model, portable encrypted backup and replacement-device recovery (M2-E2/E4, M3-E1/E2) |
| Models and learning | Partial: bundled llama.cpp and traces | Foundation Models and Gemini Nano backends, measured platform/bundled decision; routing/style/preference learning with held-out improvement and rollback (M1-E4, M2-E6, M3-E3/E6) |
| Actions and connectors | Partial: [approved device email draft handoff](../app/pr3-app/src-tauri/src/device_mail.rs), [opt-in native contacts](../app/pr3-app/src-tauri/src/contacts.rs); sandbox retained for benchmarks | Android device acceptance; optional future account/provider integration for verified sending, calendar, signed manifests and reconciliation (M5-E1/E2/E6); unified inbox remains deferred |
| Platform/SDK | Partial: Tauri hosts, model downloads and platform voice | Stable C ABI/client, Swift bindings and Unreal sample; Windows/Linux platform vaults; desktop speech fallback; production signing and notarization (M1-E7, M4, M6) |
| Evaluation/release evidence | Partial: [completed Personal-200 v3](../bench/personal/evidence/2026-11-11-v3-report.md) (all 1,494 runs; fresh-held-out 4B 88/120, 27B 107/120), immutable [v2 evidence](../bench/personal/evidence/2026-11-09-v2-report.md), [smoke corpus](../bench/task/corpus.toml), [AgentDojo](benchmarks.md), [CI](../.github/workflows/ci.yml) | Remediate observed failures; broaden semantic oracles in a new version and freeze new families before further held-out claims; complete the separate security matrix; calendar/file-sharing/physical-device corpus coverage, matched OpenJarvis comparison, 30-day diary, real-device power/latency, Kani/TLA+/sanitizer gates and independent review |

**11 November current-build workflow rebaseline (completed).** All **1,494 declared runs** completed on the frozen current product build: 200 revised Personal regression tasks, nine unchanged email cases and 40 newly frozen tasks, both models at seeds 1/2/7. Fresh-held-out passes: **Qwen3.5-4B-Q4_K_M: 88/120; Qwen3.8-27B-Q4_K_M: 107/120**. There were **6 task runs with runtime errors**; all failures, exact effects, privacy findings and desktop latency remain in the separate partition results. No runtime/prompt tuning, approval changes or budget increases occurred. [Report and replay](../bench/personal/evidence/2026-11-11-v3-report.md). V1/v2 results remain immutable; only identical cases are used for the matched comparison. This completes the evaluation, not Android, the separate older security matrix, general semantic accuracy or a release gate. Use the recorded failures to prioritize the next implementation.

**10 November multilingual email exclusion follow-up.** A local vocabulary now maps financial, health, contact and access exclusions to related fields in English, French, Spanish, Persian and Arabic. Recognized ambiguity asks for clarification; a named-field reinclusion preserves other category restrictions and places its latest value back in planning context after restart. Original user instructions survive folding/restart and explicit follow-ups without taking rules from summaries or tool output. All **443 workspace tests** pass (17 new; 12 existing opt-in checks ignored), along with 21 Python harness tests, strict Clippy and formatting. The final installed 4B build passes **27/27** new development workflow runs and **18/18** unchanged exclusion/compatibility runs at seeds 1/2/7, with no forbidden card/outbox content. Approval dispatch, core/engine limits, planner grammar and decoding are unchanged; the drafting policy now instructs the model to include requested permitted facts in the body and avoid invented attachments; no model call or schema was added. [Evidence and replay](../bench/task/evidence/semantic-email-2026-11-10.md). This is bounded category/field matching, not general semantic redaction: unsupported phrasing, unlabelled narratives, arbitrary paraphrases/translations of private values, broader held-out evaluation and Android acceptance remain open.

**10 November resumed-email planning follow-up.** Both remaining source-to-email failures now pass. Leading `Now`/`Now,` selects the existing structured planner; after any email attempt the adapter offers no further tools and uses the existing protected answer pass. All **24/24** unchanged source-workflow runs pass on 4B/27B at seeds 1/2/7, with no rereads after restart, all 24 source labels and exactly one approved sandbox email per case. Nine existing 4B restart/speech/approval controls pass too. Six new regressions bring the workspace to **426 passes** (12 existing opt-in checks ignored), with strict Clippy and formatting. The first candidate fixed both rereads but produced two approved sandbox emails in one case; its raw evidence is preserved. Core/host approval checks, source guards and execution limits are unchanged. [Evidence and replay](../bench/task/evidence/source-planning-2026-11-10.md). This is reused regression evidence; semantic/multilingual exclusions, broader current-build/held-out evaluation and Android acceptance remain open.

**10 November approval-source follow-up.** Host origin metadata now persists with tool results through folding/restart, pending-card recovery, explicit follow-ups, splitting/deletion and paired-task transfer. All **22/22 shown approval cards** retain the source site after restart; **22/24** full workflows pass on 4B/27B at seeds 1/2/7. Two 27B seed-1 workflows reread the page after restart, triggered the unchanged same-turn action block and produced no card or email effect. The resumed-email planning follow-up above resolves these two failures; the original observations are retained. Older records show an explicit unknown-source label. Eleven new regressions and all 420 workspace tests pass (12 existing opt-in checks ignored); approval checks and execution limits are unchanged. [Evidence and replay](../bench/task/evidence/source-provenance-2026-11-10.md). Next: semantic/multilingual email exclusions, broader current-build/held-out evaluation and Android acceptance.

**10 November planning/status follow-up.** The six remaining failures from the email-exclusion run below are fixed: structured unapproved email proposals, nonempty subjects, current-turn result status and the existing protected answer fallback after repeated reads. All 51 unchanged focused cases pass, versus 45/51; 14 new regressions bring the workspace to 409 passes (12 existing opt-in checks ignored). The same build passes 21 source-boundary checks and all 12 seeded travel workflows, with no successful attacks. Approval and execution limits remain intact. [Evidence](../bench/task/evidence/planning-status-2026-11-10.md). The approval-source follow-up above closes the missing-label defect; broader current-build/held-out and Android acceptance remain open. Direct-English routing and the existing bounded exclusion parser do not close semantic/multilingual redaction or general conversational quality; the speech control still asks unnecessary clarifying questions.

**10 November email exclusion follow-up.** The observed private-reference disclosure is addressed with task-scoped content checks before approval, including restored drafts, and deterministic acknowledgements for recognized standalone corrections. Restrictions survive folding/restart and explicit retained follow-ups; failed saves cannot silently revoke them. Eleven new regressions and all 395 workspace tests pass (12 existing opt-in checks ignored). Final model checks pass 22/24 original removal cases, 15/18 supplementary cases and 8/9 compatibility controls, with no forbidden card/outbox content. 6 runs failed their frozen checks; the JSON retains every failure and raw observation. [Evidence](../bench/task/evidence/email-exclusion-2026-11-10.md) preserves both interrupted candidates and their observed failures. This bounded English/literal-field guard does not close broader semantic or multilingual redaction. The planning/status follow-up above addresses these six known failures; the approval-source follow-up above closes missing source labels after restart. Android acceptance and wider evaluation remain open.

**10 November task isolation follow-up.** New task now persists an explicitly empty conversation across restarts. Selection is device-local; old histories remain available through explicit return, while deletion cannot select a different task. Failed navigation does not claim success. Eight new regressions and the 384-test workspace pass (12 existing opt-in checks ignored). The unchanged four Personal-200 isolation cases pass all 24 runs on 4B/27B at seeds 1/2/7 with zero forbidden recalls; nine 4B resume/follow-up/approval controls pass too. [Evidence](../bench/task/evidence/new-topic-2026-11-10.md). This closes the observed isolation defect as a focused regression, not a broader release gate or fresh holdout. The email exclusion follow-up above addresses the observed draft disclosure; the approval-source follow-up above closes missing approval provenance. Android acceptance remains with the user.

**9 November Personal-200.** V2 freezes 200 synthetic desktop tasks in ten categories, split by scenario family before inference: 120 development and 80 held-out tasks. Both installed 4B/27B models completed seeds 1/2/7 against the same frozen current build (1,200 runs). Development passes are 247/360 and 351/360; held-out passes are 160/240 and 199/240. The scorer independently checks raw observations, exact recipients and lifecycle routes and retains all failures, including 20 task runs with runtime errors. The evaluation found task isolation after New task plus restart, requested private-detail removal from drafts, and source provenance on restored approval cards; the 10 November follow-up above fixes the isolation issue while keeping approval/execution limits intact. Numeric formatting and ambiguous email purposes need a new corpus version; do not rewrite v2 scores. Future tuning makes v2 regression evidence, so a fresh holdout needs unobserved families. Calendar/file-sharing, physical-device transport and live delivery remain uncovered. Android testing stays with the user. The older security matrix resumed with its own frozen inputs and results. [Report and evidence](../bench/personal/evidence/2026-11-09-v2-report.md).

**7 November security/storage work.** The old frozen matrix stopped at 4,270/19,278 rows and cannot resume after the macOS 27.0.1 → 27.2 upgrade. Preserve it as partial evidence and run the separately frozen v4 implementation under `bench/rebaseline/result/2026-11-07/`; neither is a completed release gate. Focused triage closes the scripted approver's embedded-link gap. A subsequent [protected-answer implementation and regressions](../bench/agentdojo/evidence/answer-boundary-2026-11-07.json) close both observed travel answer failures and pass 18 additional seeded source-boundary cases. A subsequent [bounded planning/recovery follow-up](../bench/agentdojo/evidence/planning-2026-11-07.json) resolves both original seed-2 task-12 failures. That v6 expanded seeds 1/2/7 travel gate passed 11/12 cases, with 0/6 successful attacks; the seed-7 benign task-12 case still reached the ten-round limit at that revision. The 8 November follow-up below closes the remaining focused failure. The frozen full matrix predates these fixes; a full current-implementation/held-out evaluation remains open. SQLCipher memory/task encryption and recovery-code access are implemented on Apple/Android; [storage acceptance](storage-recovery.md) and Windows/Linux adapters remain open. Finish the frozen matrix, evaluate the current planning and answer changes more broadly, and validate Android latency/recovery/continuity before considering the 30-day diary. Unified inbox remains deferred.

**8 November focused planning and history follow-ups.** The [execution-budget fix](../bench/agentdojo/evidence/planning-budget-2026-11-08.json) closes the remaining seed-7 planning-limit failure: all 12 travel cases pass within the original ten-round budget, with required source lookups, exact approved effects and no successful attacks. The existing final answer opportunity becomes tool-free after exhaustion; approvals, round/call limits and retry guards remain intact. Nine planning/approval fixtures and 21 source-boundary cases pass. Nine email/restart compatibility runs pass on this build. An earlier 27-case history/parking/email run passed before the final declined-action wording correction and is retained separately. The [task-history implementation](../bench/task/evidence/history-2026-11-07.json) retains corrected details through long conversations and restarts, with deletion and isolation checks. These focused desktop development results do not close broader workflow/security, held-out accuracy or Android gates; the separately frozen full matrix is still incomplete.

**Next validation sequence, agreed 3 November.** First finish email using the device's mail app: capture → correct → restart/resume → approve draft handoff → user reviews/sends in the mail app → explicitly finish. This choice keeps account credentials in the mail app; PR3 can verify handoff only, not provider acceptance or delivery. Run the [Android acceptance checklist](android-email-checklist.md) on the user's phone. Then rebaseline the full workflow/security suites across multiple seeds with contradiction-aware answer checks and measured phone latency. A targeted three-seed email workflow test is development evidence, not that full rebaseline.

Paired-device continuity now has an initial implementation: a one-peer scope enabled by pairing and disabled by unpairing, user-selected memories/tasks, durable single-writer handoff, permanent deletion tombstones, conflict refusal and interrupted/duplicate delivery recovery. Recovery forks a local copy and retires the old shared identity; it does not restore an approval. Run the [phone/computer acceptance checklist](continuity-checklist.md). Contact sync remains a separate device-address-book feature and does not replicate contacts. Encryption/key recovery and release hardening precede wider deployment. Finally, collect the planned 30-day user-zero diary (completed tasks, corrections, interruptions, time saved); this is a real-use evaluation, not a subscription trial, and has not started. Do not mark the calendar study or cross-device milestone complete based on local tests.

**Implemented recovery slice (M1-E1, M5-E2).** Stop requests cancellation and waits for any in-flight operation's actual result; it never implies an action was undone. A stopped, failed or interrupted task stays visible as **Needs attention**, with its recorded results and an explicit retry of the last request where safe. Retry revalidates actions and requires fresh approval; successful or uncertain consequential effects are excluded. With task storage enabled, a checkpoint is persisted before execution; if the app closes before a result is recorded, the task shows **Outcome unknown** with no one-click retry or automatic approval. Full provider reconciliation and distributed checkpoints remain separate deliverables. Evidence: the task/host regressions above, [device-check cancellation and checkpoint-failure tests](../app/pr3-app/src-tauri/src/actions.rs), and manual UI checks using synthetic data.

**Implemented memory/status slice.** Chat updates offer explicit replacement or separate storage; exact-id corrections supersede old versions and identical proposals, with atomic summary invalidation. Model proposals cannot silently confirm a possible conflict. Operation completion messages are composed from runtime results and stored consistently in chat/history/exports; a bounded English fallback also catches common completion claims without an operation. Evidence: [memory and host regressions](../app/pr3-app/src-tauri/src/host.rs), [storage rollback/retention/erasure tests](../crate/pr3-memory-sqlite/src/lib.rs), and manual UI checks using synthetic data. Semantic/multilingual correction review now adds bounded local-model comparison and localized explicit commands. A 10-case development corpus passes on the installed Qwen3.5 4B model; full-store retrieval, held-out multilingual accuracy and device-wide latency remain open. The completion-claim fallback is still bounded English; semantic memory matching does not provide general multilingual factual verification.

Automated native UI workflow regressions are now implemented, with real Tauri IPC/Host/SQLite and isolated model/device adapters. The [rebaseline runner](development.md#full-reproducible-rebaseline) freezes inputs, checks completion and supports the full 4B/27B security matrix at seeds 1/2/7, plus 2B/4B/27B workflow and multilingual memory checks. It rejects listed contradictions, checks exact approved effects and records first-visible/whole-turn desktop latency. Implementation of this harness does not mean the long matrix has finished: use its progress/report artifacts for current status. Real Android latency/permissions, held-out accuracy, the 30-day diary and wider release gates remain open. Unified inbox implementation remains deferred.

**Post-baseline reliability fixes (4 November).** The first resumed question and task recall now include bounded current-task user excerpts and excludes other tasks’ unconfirmed proposals from both search and fallback. Email prompting honors corrected recipients and deferred drafting; local memory review uses validated, grammar-constrained JSON with stable batch ordering. The [focused restart/recipient regressions](../bench/task/reliability.toml) are separate from the frozen full rebaseline; neither they nor the native UI tests close Android acceptance, encryption/key recovery or the broader release gates.

### Unified inbox: one input for messages across channels (planned)

Bring requests from email, Telegram, WhatsApp and later supported channels into one inbox, where the user can capture a task, retain relevant details, resume it and explicitly finish it. This is roadmap work only; no live connector is enabled by adding this entry.

- **INBOX-E1 — Common incoming-message contract:** channel, account, provider message/thread identifiers, sender, timestamps, text, attachment references and provenance. Deduplicate redelivery and retain ordering/cursors across restarts. Keep incoming content untrusted; it cannot approve actions or become an instruction from the owner merely by arriving.
- **INBOX-E2 — Read-only first connectors:** start with email and a Telegram bot using supported APIs and minimum permissions. Record exactly which messages each connector can access. Telegram bot coverage is limited by chat membership/privacy settings; Gmail reading requires OAuth scopes and applicable verification. Evaluate WhatsApp Business Platform separately; do not promise access to all personal WhatsApp chats. Confirm current provider constraints at implementation time ([Telegram](https://core.telegram.org/bots/faq#what-messages-will-my-bot-get), [Gmail](https://developers.google.com/workspace/gmail/api/auth/scopes), [WhatsApp](https://whatsappbusiness.com/developers/developer-hub/)).
- **INBOX-E3 — Inbox-to-task workflow:** show source and sender, create or attach to a task with user control, suggest relevant memory for review, and link back to the original conversation. Do not merge identities across channels by display name. Prevent context from one account/contact leaking into another.
- **INBOX-E4 — Operations and privacy:** connect/revoke accounts, protect credentials, implement backoff, bounded attachment processing, retention/deletion and visible connection health. Decide polling/webhook hosting and offline catch-up explicitly; any relay and data it sees require an architecture decision consistent with private deployment.
- **INBOX-E5 — Approved replies, later:** outbound adapters remain separate from ingestion and pass through the trusted action path. Confirm exact account, channel, recipient and content; persist idempotency and delivery evidence; distinguish queued, provider-accepted and delivered. External messages never authorize outgoing actions themselves.

**Gate:** ingest the same event twice without creating duplicate requests; recover after disconnect/restart; show accurate source and connection state; convert an incoming request into a resumable task; keep accounts isolated; pass injection tests; send nothing without explicit approval. Schedule after recovery and status work. Start with one connector and prove this workflow before adding channels; outbound work depends on M5-E2/E6.

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
- M1-E1: task state lifecycle, cancellation, retry, checkpointing in `pr3-core`/`pr3-agent`, on the task record of [Sec. 4.8](architecture.md#48-tasks-and-conversation-continuity).
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
- M2-E4: task-state delegation protocol, moving the task record of [Sec. 4.8](architecture.md#48-tasks-and-conversation-continuity). The first continuity slice explicitly shares the selected task's chat so it can resume; pairing enables continuity automatically and unpairing disables it, while the user selects each item to share or move. Further gate: a TLA+ (or Quint) model checked under node loss, message duplication, reordering and replay; interruption/duplicate-action tests derived from the model; `pr3-client-c` exercised as a second client type.
- M2-E5: notification through the publisher push relay with sealed wake-ups, and UnifiedPush ([Sec. 6.3](deployment.md#63-private-hybrid-mobile--desktop)); power profiling, deterministic-pre-filter-before-LLM policy; onboarding usable before the optional model download.
- M2-E6: Gemini Nano Prompt API backend on one supported Android device (moved from M1), then the **platform model vs bundled GGUF benchmark** on PR3-Personal tasks; decide per platform whether the bundled download is default, optional or omitted.
- M2-E7: voice notes in the Android and desktop apps - platform speech-to-text, replies as text or platform speech by the user's choice; conversation → memory proposals.

**Gate:** Local MVP on Android and desktop, and Hybrid beta ([Sec. 8.5](benchmarks.md#85-proposed-acceptance-gates-targets-not-measured-results)): same task works locally, delegates privately when authorized, and recovers from node loss without duplicate actions; platform-vs-bundled decision documented.

### Milestone M3 - Privacy Memory & Adaptive Learning (Weeks 15–20)

**Deliverables:** encrypted personal memory, provenance, limited cross-device synchronization, trace-based routing (including backend-class routing) and held-out evaluation.

**Epics/issues:**
- M3-E1: SQLCipher/keychain/keystore integration with key recovery design. **Partial:** Apple/Android database encryption, verified plaintext migration, locked-state handling and existing-database recovery codes are implemented. [Scope, acceptance and remaining gates](storage-recovery.md): physical-device validation, Windows/Linux key custody, portable backup/import and replacement-device recovery.
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
