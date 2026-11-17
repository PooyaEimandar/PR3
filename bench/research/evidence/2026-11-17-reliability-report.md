# Research reliability and remaining OpenJarvis gaps — 17 November 2026

This implementation adds user-selected website restrictions, compact task-bound source references, source-bound excerpt quotations, bounded report planning and regression coverage. **OpenJarvis feature parity and live research answer-quality acceptance remain incomplete.** The [capability checklist](../../../doc/openjarvis-parity.md) separates implemented mechanisms from unmeasured or missing workflows.

## Implementation and deterministic verification

- Website scope applies to candidates, every redirect hop, cached evidence, inference views and citations. Original user requests determine scope; later corrections can narrow it. Invalid dialog scope is rejected before creating a task. Ambiguous English “official sources only” requests ask for websites instead of trusting search-result labels.
- Short source/candidate references preserve canonical storage and provenance. Excerpt references bind their byte range, text and source identity through a checksum. Guessing a range or pairing a reference with another source fails validation. Omitted leading numeric padding is accepted only when the exact range, complete checksum, source and quotation still agree. A displayed excerpt reference can identify its own source within the bounded live evidence of this task; an explicitly supplied mismatched source is still rejected. References cannot authorize access or recover deleted sources.
- Inference retains relevant passages from distinct origins, preserves short procedural context, excludes URLs/scope selectors from question-term ranking, and avoids pinning a fetched page's navigation above later evidence. Token fitting tries shorter four-source views before dropping sources. The last research tool round is reserved for reporting; its inference copy omits completed retrieval calls while retaining saved evidence and failures. The actual history and core counters remain authoritative: four rounds, four calls per round, twelve calls per turn; approval checks are unchanged.
- Recognizable unrelated commands addressed to the assistant are rejected from reports. Explicit instruction-inspection requests and legitimate operating instructions remain supported. This bounded quality guard is **not** a general relevance or entailment classifier.

Verification: **412 Rust tests passed, 10 ignored** in the complete workspace suite; strict Clippy, doctests, formatting and whitespace checks passed. The ignored real-tokenizer budget test was run separately and passed with 4096 context / 512 output reservation. TypeScript checks, four Stop tests and nine research-harness tests passed. The trusted-side dependency check, memory's freestanding Thumb build and core's WASM build passed. Local-socket tests initially failed inside the sandbox and passed when rerun with the required localhost access. The macOS bundle is an ad hoc signed debug build, not a notarized release. Physical Android acceptance remains user-run.

## Live Tavily and restart check

Tested the isolated macOS app with the user's already-saved key and public SQLite pages. No credential, private database or plaintext application trace was exported. Web access was restored to its original enabled setting and the app was closed before model evaluation.

| Final-build test | Runtime/recovery result | Answer-quality review |
|---|---|---|
| Compare WAL/rollback commits and host restriction | Search, two reads and a validated report within four rounds; no context overflow or citation-format error | **Fail:** WAL readers and host restriction are correct, but rollback reader behavior is unanswered and the journal-deletion statement lacks support from its attached quotation |
| Correct request to quote only the same-host restriction | Direct use of the saved source; no search/read; source viewer opens | **Pass:** correct restriction and relevant exact quotation |
| Quit/reopen with web disabled, then repeat the corrected request | Task restored; direct saved-source answer; original capture time retained; no search/read or claimed live recheck | **Pass:** relevant exact supporting quotations and correct same-host restriction; one finding redundantly repeats a relevant quotation |

The final v17 source viewer retained **11/17/2026, 12:10:30 PM** after restart. UI observation upper bounds were **28.4 seconds** for the fresh report, **18.0 seconds** for the warm correction and **29.5 seconds** for the offline turn after restart. These are upper bounds from UI polling, not precise runtime latency or app-launch timings. They are not Android or controlled-power measurements. Earlier live failures, including irrelevant extra quotations and unoffered final-round reads, remain in the raw observations rather than being replaced by this successful recovery check.

The expected behavior was checked against the primary [WAL documentation](https://www.sqlite.org/wal.html) and [rollback locking documentation](https://www.sqlite.org/lockingv3.html). Exact quotation checks cannot establish that the selected quotation supports the associated conclusion. The remaining problem is evidence coverage and synthesis, not just formatting.

## Frozen development matrix

**Evaluation complete.** The final diagnostic binary and corpus were frozen before the full run: 4B and 27B, seeds 1/2/7; 16 scenarios / 23 turns per model-seed, **96 scenario runs / 138 turns**. This is development evidence, not a new held-out or matched OpenJarvis result. It combines context/restart regressions, instruction-quotation regressions, four exposed v1 reasoning cases, website-scope checks and a replay of the observed public-page failure.

The final diagnostic executable is v17, SHA-256 `824385b472cd6e8bc35b49f9c496398e18a60abef2e0fddbb1e3d469f0b0fb39`. The native app executable is `c3e9095274fa03d612826b3fcbeeac1ad753b972328678373758aa5cfe736d05`. Earlier v14 and v16 candidate matrices were stopped after reproducing citation-representation failures; their partial results are preserved separately and never combined with the final matrix.

The public HTML is downloaded once during preparation and passed through the production parser in subsequent offline fixture reads. This replay does not reproduce Tavily's search-result inventory or provider latency. The unchanged v1 corpus and historical matched comparison remain separate.

The machine is an Apple M2 Max with 96 GiB memory. Power state is sampled per run. Fixed seeds do not make byte-identical reruns: runtime task IDs and capture timestamps vary, and the complete observed tool traces/source snapshots are retained. The source overlay and executable/model hashes identify what was tested; model weights and executables are not redistributed in the evidence archive.

The diagnostic `pass` checks validated reports, citation counts and selected text constraints. Stronger reasoning checks and source review are reported separately. Source review was expanded to **every turn** after the narrower checks missed material errors in routine continuations. This is unblinded implementation-assistant review, not independent human validation. Material inaccuracies, missing requested facts and unrequested findings fail; harmless redundancy or formatting does not. Borderline judgments and criterion calibration are exposed in the review file. The originally selected four reasoning replays and both public-page turns remain a separate subset. Failed attempts are retained and are not automatically rerun.

## Results and acceptance decision

| Final development build | Validated reports | Basic diagnostics | Source-reviewed answers | Terminal errors | Median / p95 turn time |
|---|---:|---:|---:|---:|---:|
| Qwen3.5 4B Q4_K_M | 69/69 | 68/69 | **58/69** | 0 | 10.5 / 19.7 s |
| Qwen3.8 27B Q4_K_M | 69/69 | 69/69 | **65/69** | 0 | 59.0 / 131.0 s |

Every model/seed run finished; no failed case was retried. All **42 restart turns** used saved evidence without SEARCH/READ, although four 4B continuations still failed the content review. Independent archive replay verified **219 displayed quotations**, all 132 source-overlay hashes and complete, unique review coverage. Recorded tool use stayed within four rounds and at most six calls; the configured limits remain four rounds, four calls per round and twelve calls per turn.

Five intermediate tool calls failed and recovered within the same turn: three 4B reports were rejected for unrequested source instructions, and two 27B reads requested URLs absent from the offline fixture. These are retained separately from terminal errors. The single basic diagnostic failure was a 4B documentation-exception correction with the wrong supporting quotation; all reports still passed runtime citation-integrity validation.

| Source-reviewed suite | 4B | 27B |
|---|---:|---:|
| Context and restart | 16/18 | 18/18 |
| Source instructions and legitimate procedures | 22/24 | 24/24 |
| Exposed reasoning and website restrictions | 17/21 | 20/21 |
| Public SQLite comparison, initial turn | **0/3** | **0/3** |
| Public SQLite comparison, offline correction | 3/3 | 3/3 |

The larger model is more reliable on this development set but does not close the research-quality gate. Both models miss or incorrectly infer rollback-reader behavior on all three real-page comparisons. The 4B model fails to explicitly resolve current-price uncertainty in all three seeds; the 27B seed-7 answer incorrectly promotes an undated listing to a current price, then contradicts that conclusion in its gaps. Other 4B failures include an incorrect correction citation and separate obsolete findings after a changed request.

Two of the 4B failures are explicitly borderline: an unsupported “public-domain license” characterization and unverified examples presented as official websites. A more permissive reviewer could score those two as passes, giving 60/69 instead of the reported strict 58/69. The review also records criterion calibration and a wording concern in the passing 27B seed-2 current-price answer. This is not independent validation or an estimate of general product accuracy.

Power samples at the start and after every run showed AC power and 100% charge. The native app was closed during evaluation. Timings include failed attempts, exclude model loading, and use offline page fixtures; thermal conditions, background work and energy were not controlled or measured. They do not establish Android performance or a current OpenJarvis latency comparison.

The acceptance decision is **runtime/citation mechanisms verified; research answer quality still incomplete**. Keep the public-page replay and exposed reasoning cases as regressions. A fresh matched native-retrieval comparison and independently reviewed holdout remain outstanding.

## Inspectable evidence

- [Full archive](2026-11-17-reliability.tar.gz): six complete model/seed outputs, frozen corpus, all-turn source review, earlier failed/interrupted candidates, native observations, source overlay and verification logs.
- [Recomputed summary](2026-11-17-reliability-summary.json) and [archive verification](2026-11-17-reliability-archive-check.json).
- Extract the archive and run `node replay.ts` with Node 24 or later to reproduce aggregate counts, exact-quotation checks, source hashes and execution-limit checks without model weights. `node check-replay.ts` tests rejection of incomplete runs, missing/duplicate reviews, a changed quote and a changed corpus using disposable copies. The archive README documents inference reproduction against the exact frozen corpus.

## Remaining work

1. Improve coverage of all parts of a multi-part question across long sources, and distinguish missing excerpts from missing evidence.
2. Check claim-to-evidence support and explicitly answer temporal uncertainty. A correct quotation alone is insufficient.
3. Repeat live acceptance, collect Android latency/continuity evidence and run a fresh independently reviewed holdout before making broader quality claims.
4. Then advance PDF/folder ingestion, semantic retrieval, approved local exports and the other items in the [OpenJarvis checklist](../../../doc/openjarvis-parity.md). The prior shared-tool comparison is not native retrieval or overall product parity.
