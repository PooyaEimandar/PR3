# Online research repair — 20 November 2026

The Android report showed a rejected excerpt reference and no completed answer. The supplied request was **“Who is Pooya Eimandar, search him.”** A live macOS reproduction with Qwen3.5-4B and the existing encrypted Tavily configuration confirmed a separate planning failure on that exact request: Tavily returned five candidates, but the model tried to cite candidate IDs and search snippets without reading any page. Both reports were correctly rejected; the final report had no supported finding. This establishes a failed end-to-end answer, not a provider outage, and does not prove that the Android trace followed the identical sequence.

## Changes and regressions

- Discovery without saved evidence now offers unread candidates before reporting. All eligible task candidates remain selectable; a relevant result after the first three cannot be hidden by a presentation cap. Candidate storage and actual call limits remain bounded.
- With saved evidence and no unattempted explicit user links, the penultimate round offers reporting, preserving the final round for repair. A rejected excerpt reference receives explicit source-plus-literal-quotation guidance. The runtime never silently accepts or substitutes a citation.
- The first live repair read pages but mixed a namesake into the biography. Simple English full-name identity questions now prefer matching candidates and require the cited page's live title/body to identify the full name. Other saved parts of that same page can establish the name. A matching name on a different page cannot.
- The next candidate removed the namesake but still produced a heading-only finding and a paraphrase unsupported by its attached quotation. The full page separately mentions AI products; this was a citation-support problem, not proof that the fact itself was false. Simple identity questions now present checked quotations directly. Optional model restatements are omitted, and a heading alone cannot form a finding. Explanation/comparison requests are outside this narrow extractive mode.
- A further live test repeated a rejected heading until the budget expired. Inference choices now omit headings and bare dates while preserving exact offsets and stored bytes. The next online test passed, but its restart answer included a GitHub “Follow” label. The final revision also excludes name/handle-only follower labels. A heading can accompany a substantive quotation; the report still validates each citation normally.
- Explicit continuations retain the subject; changed questions do not inherit an obsolete name restriction. The report checks run again after storage is reopened and apply to literal and excerpt-reference citations. Existing ownership, deletion, scope and approval checks remain intact.

Limits remain **four tool rounds, four calls per round, twelve calls per turn, 4096 context tokens and a 512-token output reservation**. No network permission, provider fallback or additional retry budget was added. The full-name safeguard is lexical and deliberately narrow. It does not provide general multilingual entity resolution, alias verification, proof of page truthfulness or semantic entailment.

Verification on the final source revision: **446 Rust tests passed, 10 ignored**, strict workspace/all-target Clippy and formatting passed, and **nine native macOS workflows** passed. Four Stop tests and nine TypeScript research-harness tests also passed. The regression for search → batched reads → rejected reference → explicit repair was first observed failing before the repair. Further checks cover later search results, actual composer Web permission, unchanged budgets, source ownership, same-name contamination, heading/profile-label rejection, exact offsets after context fitting, suppressed unsupported restatements, changed questions and reopened storage.

## Live review and remaining acceptance

The [attempt summary](2026-11-20-online-repair-summary.json) retains **all nine live turns**, including five answer-quality failures across earlier candidates. Valid runtime quotations are not counted as answer-quality passes when the answer identifies the wrong person, adds unsupported interpretation or promotes profile controls into findings. The seven earlier turns remain separate from the two final-build checks; successful attempts are not substituted into earlier failures.

**Both final-build checks passed with Qwen3.5-4B-Q4_K_M in the native macOS app and the existing saved Tavily key.** The executable SHA-256 is `fb237028c7bee96b8e78c9037d320243c66fad1eb17c561cc26795d2cdede41d`; its 138 source hashes were frozen before inference.

| Final check | Result | Recorded runtime |
|---|---|---|
| Exact request, fresh task, Web on | Tavily discovery completed; two pages read; two substantive biographical quotations; all four tool calls succeeded in three rounds | 13.43 s |
| Full app restart, same task, Web off, “Continue.” | Four substantive quotations; one successful report call; no SEARCH/READ and no rejected call | 9.97 s |

The previously cited source retained its canonical identity and displayed capture time, **20 November 2026 at 13:44:53 Dubai time**, across the restart. Its passage remained inspectable. The final online quotations match the saved biography passage inspected during the live work. Maximum prompt counts were 2,397 and 3,366 tokens; the 512-token output reservation and original context/call limits stayed intact. One cosmetic defect remains: the final continuation displays `/gaps` in the unresolved-questions field. It does not add a factual claim and is recorded separately from citation support.

These are single-task observations, not a controlled speed comparison, multi-seed model evaluation or Android latency measurement. Builds/tests had finished before these two turns, but hardware load, thermal state, provider responses and seeds were not experimentally controlled. The API key was neither extracted nor written into evidence. The isolated app was closed and its original disabled-tracing configuration restored after testing.

The earlier [six-run quotation/interpretation matrix](2026-11-20-support-report.md), its source hashes, scores and archive are unchanged and **predate this follow-up**. They do not certify these later orchestration and identity-answer changes. This is focused development work on an exposed user-reported case, not a new holdout, full model rebaseline or matched OpenJarvis comparison. Broader live acceptance, independent review, alias/multilingual cases and controlled Android latency remain open.

## Android update

The matching non-debuggable ARM64 release APK was installed in place on the authorized **Samsung SM-S938B / Android 17**, without uninstalling or clearing app data. The [final installation record](2026-11-20-android-online-repair-v2.json) verifies all 138 frozen source hashes, the existing local Android Debug certificate, 16 KiB ZIP/native-load alignment and the installed APK's exact hash. Launch returned success, the process was running and the activity was in the foreground. The [earlier installation](2026-11-20-android-online-repair.json) is retained as a superseded candidate, not final acceptance evidence.

Final APK SHA-256: `f0c6e2d6cf5f8ebf0750e9c5c2b9cc544955d569acbc40c0dc171a8c98e7a0df`. The local artifact is `target/pr3-release-2026-11-20-online-repair-v2.apk`. The existing development certificate preserves update compatibility; this is not production distribution signing. The user's on-device search and latency acceptance remains open.

The [evidence archive](2026-11-20-online-repair.tar.gz) retains public test traces, final and intermediate source overlays, source/binary manifests and verification logs. [Extraction verification](2026-11-20-online-repair-archive-check.json) checks all 138 final source hashes and archived artifact bytes. Credentials, unrelated personal traces, app databases and original app configuration are excluded.
