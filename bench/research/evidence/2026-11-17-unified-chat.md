# Unified chat implementation check — 17 November 2026

Research now starts from the normal composer and stays in the current task. The separate Research dialog is removed. Web and optional website choices remain explicit user controls; ordinary conversation and tools work on later turns. New task separates source storage. Restart defaults Web off; Retry may revoke, but cannot upgrade, the original request's permission. Narrowed website scope survives a new question or a cleared field.

The production Host still uses the existing core budgets: four tool rounds, four calls per round and twelve calls per turn. Selecting research restricts the entire proposed tool batch before execution. Source quotations still pass through the research validator, and consequential actions retain the approval boundary.

## Regression checks

- Rust workspace: **418 passed, 10 ignored**, zero failures. This includes six new chat-routing, scope, retry, restart and deletion regressions and the existing policy/security suites. Ignored tests are not acceptance evidence.
- Native macOS workflows: **9/9 passed**, using the actual webview, Tauri IPC, Host and SQLite with scripted model/device adapters. Covers ordinary chat → research → ordinary chat in one task, process restart, saved quotations, deletion, inline domain validation, New task and provider/key lifecycle.
- TypeScript checks and **13 unit/harness tests** passed. Workspace Clippy passed with warnings denied.
- The native 420-pixel window screenshots were visually inspected. Physical Android, keyboard behavior and accessibility acceptance remain manual checks in the [Android checklist](../../../doc/android-documents-checklist.md).

## Local-model smoke

The exposed [chat corpus](../chat-regressions.json) ran on Qwen3.5-4B Q4_K_M at seeds **1/2/7**: **9 scenarios, 15 turns, 15 automatic passes**. The implementation assistant read every answer against its saved source text: the comparisons correctly identify 25 credits as cheaper than 40; the narrowed restart answers quote Alpha only; the public-link fixture and its restart report a maximum group size of eight.

All six restarted turns called only `answer_research`, with no additional search or read. Each web scenario fetched its offline fixture once. This is synthetic development coverage, **not a live Tavily test, held-out evaluation, 27B result or new OpenJarvis comparison**.

Observed warm-turn times on an Apple M2 Max, on battery: first answers **6.67–8.68 s**, resumed answers **2.15–2.39 s**. Model loading is excluded; restart recreates the Host/database connection while retaining the loaded model. These are diagnostic measurements, not controlled-power performance claims or Android latency.

Remaining presentation issues are retained in the raw results: seed 1 repeats price evidence, and two resumed web answers print a model-written “None” gap. One comparison finding cites only Alpha, while a separate finding in the same report cites both prices; this is not evidence of complete claim-level citation support. Broader routing, synthesis and temporal-uncertainty gaps remain as recorded in the [reliability report](2026-11-17-reliability-report.md).

The [evidence archive](2026-11-17-unified-chat.tar.gz) contains raw answers, source snapshots, tool/core traces, corpus, source/model/executable hashes, test logs and native screenshots. The base commit is `386c098e9f8b2eccc3faa706edbdf5bbdf05ab95`, with the uncommitted implementation identified by source hashes. No API credential, personal document or live provider request was used.
