# Research report contracts and evidence selection — 21 November 2026

**Completed: 4B and 27B, seeds 1/2/7, all 240 turns reviewed.** All eight previously failing configurations pass the main review in this rerun; six pass strict review and two retain wording concerns. The overall result is **232/240 main, 212 strict and 233 lenient**, with eight different failures retained. The preceding result was 232 main, 218 strict and 232 lenient: this does **not** demonstrate an overall answer-quality improvement. No new OpenJarvis comparison or independent held-out claim is made.

## Changes

The [preceding evaluation](2026-11-21-evidence-repair-report.md) passed 232/240 main reviews and all 84 restart turns. Its eight failures involved a missing SQLite commit condition, malformed Python citations and Cargo report fields, unsupported identity conclusions, an undated Spanish price treated as current, and omitted or misinterpreted Persian recovery conditions.

Non-thinking research generation now uses a report grammar derived from the exact fitted, host-marked evidence. It requires findings and gaps, constrains citations to displayed source-bound references with matching anchors, and escapes source text so quotation content cannot become grammar rules or XML parameter delimiters. The model selects the reference before its anchor. Literal quotations remain available in the trusted quotation-only mode. Existing runtime validation still checks every submitted source, range, quotation, task, deletion and website restriction. String byte caps remain in runtime validation instead of being expanded into hundreds of optional grammar branches; the token generation cap remains unchanged. The grammar applies to 1–32 valid fitted excerpts; otherwise ordinary runtime validation remains in charge. It does not certify relevance, entailment or completeness, and cannot guarantee completion before that limit.

Questions asking for a current fact or an identity conclusion receive a required conclusion field once saved evidence exists. The conclusion is validated against the report's quotations, including the identity, current-validity and support checks. An uncited or empty conclusion is rejected. Quotation-only reports suppress optional conclusion prose even for direct tool callers, including after reopening storage. No-evidence reports can still explain missing evidence in gaps. A correction rebuilds the inference schema rather than retaining the preceding question's conclusion requirement; source text cannot activate it.

Long-page selection keeps primary conditions and removes redundant background excerpts in comparisons between different origins, while preserving condition-bearing excerpts, option/default scope and short procedures. Explicit function syntax narrows parts of a matching origin to the requested function, preserving comparisons, other origins and pinned reads. These are bounded lexical selection hints, not semantic proofs or changes to stored evidence. Unknown functions retain the ordinary selection path. Exact quotations and their offsets survive refitting and focused restart corrections.

Narrow interpretation checks reject additional categorical identity variants, recognize the tested Spanish expired/undated price language, and distinguish approval from a device owner from verification of device ownership in English and Persian. Negation in one sentence cannot exempt a contradictory ownership assertion in another. Spanish current-price uncertainty receives a Spanish notice. These checks deliberately do not claim general multilingual entailment or entity resolution.

Approval policy and execution limits remain unchanged: four tool rounds, four calls per round, twelve calls total, 4,096 context tokens and a 512-token answer reservation. Automatic report grammar is disabled for thinking mode or a tool set containing a consequential action. Explicit caller grammars retain precedence. All tool execution and approval decisions remain in the existing runtime.

## Regression validation

- Rust workspace: **481 passed, 12 ignored**. Three ignored native tokenizer/grammar checks passed separately.
- Strict workspace Clippy, formatting, whitespace checks and the research evaluator build passed.
- Native grammar checks cover both tool formats, required fields/conclusions, changed quotation punctuation, foreign references, source XML delimiters, truncated reports and action-tool substitution.
- Evidence regressions retain SQLite commit conditions, the requested technical rules, function scope, complementary Git evidence under real tokenizer pressure, multilingual corrections and exact UTF-8 ranges.
- SQLite reopen tests verify identity/date/owner-approval rejection and valid repairs, conclusion validation, unchanged saved text, task isolation and deletion. Existing approval, cancellation and execution-budget tests remain green.

Earlier prototypes and failed validation attempts remain separate. In particular, broader excerpt pruning lost Git's outside-repository condition in a real-tokenizer test; the delivered refinement is limited to multi-origin comparisons and passes that test. An earlier focused 27B attempt used an incorrect model filename and did not load a model. A superseded diagnostic was stopped with its completed rows preserved. None of these attempts substitutes for an answer in the final matrices.

## Frozen evaluation scope

Final executable `contract8` has SHA-256 **c4d92562e2f7b176a0ba22d572583f5657c4487b0f7695381e4410d297a89459**, with 141 frozen source files. Models are `Qwen3.5-4B-Q4_K_M.gguf` and `Qwen3.8-27B-Q4_K_M.gguf`, each at seeds 1/2/7. The development matrix contains 162 turns and the exposed regression matrix contains 78; their existing cases and page bytes are unchanged.

An early `contract6` matrix attempt was stopped to investigate grammar overhead. Its 21 completed turns, all 21 main-pass reviews (20 strict), clocks and load samples are retained separately. It is not combined with the final matrix. A later `contract7` attempt was stopped for the direct-call quotation-only conclusion guard; its completed rows and eight recorded reviews are also retained separately. A two-turn spot check with compact grammar took 9.11/5.73 seconds versus 21.08/10.97 seconds in that earlier attempt, with both answers passing. The initial wording differed and the machine conditions were not controlled, so this is observational evidence, not an isolated causal speed estimate. The same native grammar acceptance test took 5.47 seconds before and 4.66 seconds after; both passed. Runtime UTF-8 byte-cap regressions remain active.

Every actual answer receives unblinded implementation-assistant review against the latest request and its cited evidence. Material unsupported claims, missing requested facts/conclusions and incomplete reports fail. Borderline judgments are recorded separately for strict/main/lenient sensitivity. This is exposed regression evidence, not independent review, fresh generalization evidence or a new matched OpenJarvis comparison. Runtime-generated references and capture times mean a seed does not reproduce a bitwise-identical prompt across runs.

The production Host, SQLite and local models use offline page fixtures. Restart cases reopen storage and create another Host while retaining the loaded model. This does not establish live Tavily, native UI, cross-device or Android acceptance. Final-run timing includes repairs and is accompanied by power and background-load samples; it is not a controlled performance comparison or a mobile latency measurement.

## Results and preserved evidence

| Matrix | Model | Main | Strict | Lenient |
|---|---|---:|---:|---:|
| Development | 4B | 79/81 | 75/81 | 79/81 |
| Development | 27B | 80/81 | 78/81 | 80/81 |
| Exposed regressions | 4B | 37/39 | 31/39 | 37/39 |
| Exposed regressions | 27B | 36/39 | 28/39 | 37/39 |
| Total | Both | **232/240** | **212/240** | **233/240** |
| Restart turns, included above | Both | **83/84** | **80/84** | **84/84** |

Main scores by seed 1/2/7 are 27/25/27 for 4B development, 27/26/27 for 27B development, 12/13/12 for 4B exposed and 12/12/12 for 27B exposed. Each model totals 116/120 main passes. There are 20 accepted borderline answers and one borderline failure; the archived decisions explain each judgment. Strict review rejects all borderline answers, while lenient review accepts the borderline failure too. Review is unblinded and performed by the implementation assistant, not an independent assessor.

### The eight preceding failures

| Configuration | Previous failure | Current result |
|---|---|---|
| Development, 27B seed 7, SQLite comparison | Missing rollback reader exclusion | Main and strict pass; all three requested conditions quoted |
| Exposed, 4B seed 2, Python | Invalid excerpt reference/anchor; no report | Main and strict pass; truncation, strict=True and laziness supported |
| Exposed, 4B seed 2, Cargo | Missing gaps field; no report | Main and strict pass; all three requested rules supported |
| Exposed, 4B seed 2, identical name | Repeated unsupported identity claims; no report | Main and strict pass after one runtime rejection and repair within the existing budget |
| Exposed, 4B seed 7, identical name | Categorical different-person conclusion | Main pass, strict fail: uncertainty is explicit, but opening with “people” instead of “profiles” can imply distinct identities |
| Exposed, 4B seed 7, Spanish dates | Undated price certified current without an uncertainty conclusion | Main pass, strict fail: current price is explicitly unconfirmed, but the prose still calls the undated catalog “actual” |
| Exposed, 27B seed 1, Persian recovery | Missing 45-day retention | Main and strict pass; retention, code and owner approval included |
| Exposed, 27B seed 7, Persian recovery | Ownership verification substituted for owner approval | Main and strict pass; prose and quotation preserve approval from the owner |

These are outcomes for the same exposed cases and seeds. They do not prove that the failure classes are eliminated generally. The archive's exploratory/prior-comparison.json records the exact old and new decisions without changing the preceding archive.

### Failures in this rerun

| Matrix / configuration | Case / turn | Failure |
|---|---|---|
| Development, 4B seed 2 | replay-h08, initial | Persian prose reverses the resident/fee condition despite a correct quotation |
| Development, 27B seed 2 | replay-h08, initial | Calls a 28-unit fee cheaper than free membership |
| Development, 4B seed 2 | public-sqlite-comparison-restart, initial | Omits rollback reader behavior; all required evidence was fitted |
| Exposed, 4B seed 1 | fresh-git-diff, initial | Omits the meanings of exit statuses 0 and 1 |
| Exposed, 27B seed 1 | fresh-git-diff, initial | Omits outside-repository explanation and exit-code meanings |
| Exposed, 4B seed 7 | fresh-similar-name, initial | Returns ordinary prose without a validated report; runtime correctly marks the turn incomplete. Continue after restart succeeds, but does not replace this failure |
| Exposed, 27B seed 2 | fresh-cargo-features, restart | Adds an exclusive dep: activation rule unsupported by its quote; the saved source also documents package-name/feature-name activation. Borderline failure: a loose reading as explicit feature wiring generally passes lenient review |
| Exposed, 27B seed 7 | fresh-cargo-features, initial | Explicitly says only dep:name references can enable the optional dependency, contradicting the documented alternative |

Other strict-review concerns include cross-finding citation support, speculative authority examples, treating undated information as historical/current, overstating differences as conflicts, extra implicit-feature detail absent from the attached excerpt and claiming a whole saved source lacks information when only the selected excerpt is limited. A 4B Cargo restart answer echoes report-writing guidance in its unresolved-questions field. The narrow automated placeholder check reports zero, but that does not establish clean presentation. Repeated findings also remain.

### Runtime and evidence audit

The complete run produced **239 validated reports** and one honestly incomplete turn. The only rejected tool call was the unsupported 4B seed 2 identity conclusion, repaired within the existing budget. No malformed-citation or missing-field rejection occurred in this final matrix. These observations do not guarantee that grammar applies to every planning path or that every future generation completes.

Replay checks verify **455 displayed quotations**, **480 fitted inference excerpts across 471 inference passes**, all **84 unchanged, nonempty restart snapshots**, and no SEARCH/READ on any restart turn. All 24 long-page source snapshots retain their required facts. Every audited SQLite, long-page, Python, Git and Cargo initial report attempt contains all required literal evidence; every restart report attempt contains its focused required evidence. Evidence presence still does not establish correct interpretation or use, as the failures above show.

The maximum fitted prompt is **3,498 tokens**, with the **512-token answer reservation** intact. Observed maxima are four tool rounds and five total calls, within the unchanged four-round, four-calls-per-round and twelve-total-call limits. All executions use the offered research tools. Approval policy is unchanged, and its existing regression tests remain green; this is not a new full security benchmark.

The final archive was extracted into a fresh directory and both replay summaries and inference audits recomputed identically. **All 39 verifier controls pass**, including altered quotations, foreign sources, changed offsets/corpus/snapshots, missing reviews, context overflow, reduced answer reservation and exceeded execution limits. Current source files, the copied executable and both complete GGUF files match their recorded hashes.

### Observed performance

Apple M2 Max / 96 GiB, production Host and local models with offline page fixtures. Model loading is outside these turn timings; repair work is included. Values below use wall-clock intervals; no turn differs from its recorded elapsed time by more than one second.

| Matrix / model | Median | p95 | Maximum |
|---|---:|---:|---:|
| Development / 4B | 12.90 s | 21.90 s | 29.12 s |
| Development / 27B | 65.89 s | 123.11 s | 138.26 s |
| Exposed / 4B | 15.42 s | 25.37 s | 30.72 s |
| Exposed / 27B | 81.67 s | 116.68 s | 121.31 s |

The final sampler records 353 observations from 11:33:43 to 14:29:20 UTC, with no sampling gap over 60 seconds. Power changes from AC to battery at 13:05:13, then back to AC at 13:20:13; continuation on battery was authorized by the user. Background compiler activity appears in seven samples. Other CPU activity peaks at approximately 1,154% across cores, and one-minute system load reaches 49.44. A separate process snapshot also records a virtualization workload; it does not identify the cause of every peak. Power, contention, differing generated text and repairs confound speed comparisons. These are descriptive desktop measurements, not controlled speed improvements or Android latency evidence.

### Artifacts and remaining acceptance

- [Raw traces, source overlays, reviews, validation logs, prior attempts and replay helpers](2026-11-21-research-contract.tar.gz)
- [Recomputed summary](2026-11-21-research-contract-summary.json)
- [Extracted replay and hash verification](2026-11-21-research-contract-archive-check.json)

Archive SHA-256: **6908efda5d5a97746389eb41d28054b5d79fa8602d2cadddf1a394e47bbad209**. The preceding immutable archive remains at SHA-256 **e541c92bec95034acfaedd844fc9c9eac690ec5beaf00a8c9b40a4792649be58**. After extraction into an empty directory, run node development/replay.ts and node exposed/replay.ts; their check-replay.ts companions exercise the negative controls. The inference helpers accept the corresponding matrix directory. Replaying the evidence does not require model weights or another inference run.

The next work is reliable coverage of every requested part, bounded recovery when the model returns prose without a report, and preventing unsupported additions to otherwise correct quotations—including after restart. Live Tavily, native UI, physical Android acceptance, controlled latency and a fresh independently reviewed holdout remain open. This build was not newly deployed to Android; the last documented installation is the earlier online-search repair. Research-product parity remains open.
