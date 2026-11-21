# Evidence coverage and complete resumed answers — 21 November 2026

**Completed intermediate evaluation:** 227/240 main passes, 214/240 strict and 227/240 lenient. All 84 restart answers pass. All 240 actual answers were reviewed and the archive was extracted and replayed successfully. The run exposed a remaining deterministic loss of complementary evidence during report repair; the subsequent [repair and full rerun](2026-11-21-evidence-repair-report.md) are complete at 232/240 main and 218/240 strict. This is exposed regression evidence, not a fresh acceptance test.

## Implementation

The [intermediate selection evaluation](2026-11-20-evidence-selection-report.md) verified that evidence reached inference, but still found incomplete comparisons, unsupported identity conclusions and corrections that repeated obsolete details. The follow-up keeps the latest substantive user request when continuing a task, including after reopening SQLite. It rebuilds the inference evidence from live task-owned sources; stored assistant text cannot change that request. Narrow French and Persian corrections now select their requested restoration conditions without carrying over retention facts.

Continuation recognition uses supported English forms such as Continue, Resume and Retry. Focused correction and current-question hints cover the tested language patterns; they are not general multilingual semantic understanding.

Both comparison guidance and focused source pruning now use the positive request. An excluded tail such as “do not repeat the comparison” no longer revives the old comparison. Genuine narrowed comparisons still retain both subjects, with tests for positive and negative language patterns, source-address exclusion and reopened task storage.

Evidence ranking preserves requested phrases, command-option scope and contiguous context, including examples and the conditions that explain them. It balances sources and passages within the existing token budget. A saved SQLite fixture exercises both evidence projection stages and retains WAL reader behavior, the rollback commit condition with its antecedent, and the same-host restriction. A title identifier no longer outweighs a requested body condition in a narrow quotation request. Comparison questions still retain both subjects.

Report guidance asks for every requested part. Where a source explains the fact directly, exact quotations can form the answer without a paraphrase that changes the condition. Questions requesting current validity or identity analysis receive an inference-only instruction to state the supported conclusion or uncertainty. Separate occupations, locations and missing mentions do not establish whether two profiles describe one person. A narrow English report-quality check rejects recognized categorical identity claims without an explicit supporting relationship in the cited text. This is neither general entity resolution nor semantic entailment verification.

For simple English full-name biography questions, the inference view applies the existing runtime name check before showing passages. This avoids presenting a namesake citation that the answer tool will reject. Same-origin continuation passages remain available; compound identity questions retain both profiles. Filtering affects the inference view, not the saved source snapshot or access permissions.

Runtime quotation, source/task binding, deletion, Web permissions and approval checks remain active. No execution limit changes: four tool rounds, four calls per round, twelve calls total, 4,096 context tokens and a 512-token answer reservation. Report repair uses the existing remaining budget. The benchmark captures the evidence that actually fits each inference prompt through an opt-in diagnostic; capture is off by default in the app and does not alter the prompt.

## Frozen evaluation and review

The frozen executable is `completeness4`, SHA-256 `caf0999241365b0402079cb41f3c05ae410314bd3c280e9dd6fb04271f087d9b`, with a 140-file production source snapshot. Both `Qwen3.5-4B-Q4_K_M.gguf` and `Qwen3.8-27B-Q4_K_M.gguf` ran seeds 1/2/7. Development has 18 scenarios and 27 turns per run (162 turns); exposed regression has eight scenarios and 13 turns per run (78 turns). The original case objects and page bytes are unchanged. Failed answers are not replaced by retries.

These cases, including the formerly fresh set, have been used during implementation and are now exposed regression evidence. Every actual answer receives unblinded implementation-assistant review against its current request and sources. Main, strict and lenient scores retain borderline judgments separately from automatic checks. This is not independent review, a fresh holdout or a new matched OpenJarvis comparison. Runtime-generated task identifiers, capture timestamps and bound references mean fixed seeds do not make prompts bitwise identical; small score differences do not establish statistical significance.

The main score requires a runtime-validated answer covering the current request without a substantive unsupported conclusion. The strict score subtracts accepted borderline answers; the lenient score adds rejected borderline answers. Borderlines include incomplete per-finding attribution, a missing quotation antecedent despite a correct overall explanation, speculative examples and misleading gap wording. Missing requested facts, unsupported identity certainty, wrong-source claims and failed reports fail the main review. Cosmetic duplication or a harmless empty-status word alone does not change the score. Each judgment and its reason remain in `source-review.json`.

The harness uses the production Host, tools, SQLite and real local inference with offline page fixtures. Restart cases reopen SQLite and create another Host while keeping the model loaded. This does not test live Tavily delivery, native UI behavior, cross-device source transfer or physical Android performance.

The inference audit separately checks exact task-bound quotations and the literal availability of requested facts after restart. Its post-hoc restart checks follow the already-exposed rubrics and do not grade answer meaning or modify the frozen cases. A control removes only restart views in disposable copies and verifies that restart coverage falls while initial-turn coverage remains unchanged.

## Timing conditions

The matrix began on AC power, but macOS repeatedly returned to maintenance sleep despite the process-bound idle-sleep assertion. Power logs and long sampling gaps are preserved. A temporary five-second user-activity assertion woke the display at 06:09:49 Dubai time on 21 November; the user subsequently confirmed the Mac was awake and plugged in. Background desktop load continued to vary. Isolated CPU tokenizer tests and compilation for a subsequent repair overlapped the exposed 27B runs; this is another timing confound. Both monotonic turn duration and wall-clock start/end timestamps are retained because sleep affects them differently. Model loading is outside the reported warm-turn timings. These observations do not establish a speed improvement or Android latency.

## Development results

| Model, seeds 1/2/7 | Main | Strict | Lenient |
|---|---:|---:|---:|
| 4B | 79/81 | 76/81 | 79/81 |
| 27B | 80/81 | 77/81 | 80/81 |

The three main failures are the initial SQLite comparison at 4B seeds 1 and 2 and 27B seed 7. They omit or misinterpret rollback reader exclusion during commit, despite correctly reporting WAL reader continuity and its host restriction. All six SQLite inference views contain the full commit condition and its antecedent; the remaining failure is in using that evidence. The long-page Orchid/Reed comparison and Delta eligibility/quota answers pass all six runs, as do all 54 development restart answers.

Six accepted borderlines explain the stricter scores: speculative authority examples in the 4B official-source clarification at all three seeds; a rollback quotation missing its antecedent in 27B seed 1; and an opt-in participation claim cited only in another finding in the 27B seed 2 and 7 causation answers. Redundant supported findings remain a presentation issue.

The fitted-evidence audit verifies 285 quotations across 321 inference passes. All 18 initial technical/late-page coverage checks and all 54 restart checks have their required literal facts in every report-pass view. The largest prompt is 3,424 tokens with the unchanged 512-token answer reservation. Literal availability does not prove that the generated answer used the fact correctly.

## Exposed regression results

| Model, seeds 1/2/7 | Main | Strict | Lenient |
|---|---:|---:|---:|
| 4B | 31/39 | 29/39 | 31/39 |
| 27B | 37/39 | 32/39 | 37/39 |

All 30 restart answers pass. Across both matrices, 236/240 turns produced validated reports; validation alone does not establish answer completeness or interpretation.

The eight 4B failures are Python seed 1 (irrelevant passages replace two requested behaviors), Git seeds 1 and 7 (missing outside-repository/`--no-index` explanation), Cargo seed 1 (no validated report after schema failures), the initial biography at seed 7 (ordinary prose instead of the required report call), and all three ambiguous-identity answers. At identity seed 1 the narrow guard rejects unsupported relationships, but no corrected report follows; seeds 2 and 7 use categorical “establish different people” wording that the guard does not catch. Uncertainty elsewhere in an answer does not excuse that unsupported assertion. The seed 7 biography succeeds on Continue, but the failed initial turn remains in the denominator.

Two accepted Spanish borderlines explain the stricter score: seeds 2 and 7 explicitly express current-price uncertainty but call the undated catalog or its price “current”; seed 2 also calls the promotion the only available evidence despite quoting the catalog. French and Persian restoration corrections retain only their requested conditions. Duplicate supported findings remain cosmetic clutter.

The two 27B failures are Persian seed 1, which omits the requested retention period, and Git seed 7, which produces no validated report after quotation and restatement rejections. Five accepted borderlines lower its strict score: Cargo initial findings at seeds 1 and 7 have incomplete local citation support for extra explanations; Python seed 2 omits the strict-mode quotation antecedent; Cargo's seed-2 correction incorrectly says the saved source does not define the implicit feature; and the seed-2 French comparison cites Ardoise retention only in another finding.

All 18 initial technical checks contain the requested facts in at least one report-pass view. However, the Git view loses the outside-repository passage during repair in 4B seeds 1/7 and 27B seed 7: only 3/6 Git turns retain complete evidence in every report pass. The first prompt has three complementary passages, while the error-bearing retry fits a two-passage candidate before trying a better balanced view. A real-tokenizer regression reproduces this loss while a complete three-passage view still fits the same context and answer reservation. The next change addresses this fitting order without changing the source ranking globally or relaxing runtime checks.

The exposed inference audit verifies 340 quotations across 172 passes, with all 30 restart checks complete in every report-pass view. Combined totals are 493 inference passes, 625 verified fitted quotations and 424 verified displayed quotations. All 84 restart snapshots are unchanged and nonempty, with zero restart retrieval turns. The largest prompt is 3,501 tokens plus the unchanged 512-token reservation. Runtime traces stay within four tool rounds and at most five total calls, below the unchanged twelve-call cap.

## Observed timing

| Matrix / model | Median seconds | p95 seconds | Maximum seconds |
|---|---:|---:|---:|
| Development / 4B | 6.31 | 11.44 | 14.27 |
| Development / 27B | 35.51 | 84.91 | 106.12 |
| Exposed / 4B | 10.38 | 25.51 | 35.10 |
| Exposed / 27B | 55.67 | 151.89 | 164.31 |

These are warm-turn monotonic measurements. Development 4B wall-clock p95 is 827.80 seconds and maximum 1,017.68 seconds because of sleep; eleven turns show material clock divergence. Sampling spans 00:23:31–03:54:11 UTC on 21 November, with 232 AC-power samples, nine gaps over sixty seconds and four compiler-active samples. Neither these timings nor comparisons with earlier runs isolate a code speed change.

## Regression validation

- Complete Rust workspace suite: 467 passed, 10 ignored. The ignored real-tokenizer budget test passed separately after the report-prompt change.
- Strict workspace Clippy, formatting and whitespace checks passed. TypeScript checks, four Stop tests and nine research harness tests passed.
- Regressions cover evidence offsets through both projection stages, option conditions and examples, saved real-page variants, and disk-backed restart/correction/isolation/deletion.
- French/Persian correction tests reopen storage twice, ignore a misleading assistant message, retain only the requested condition on Continue, preserve source bytes and verify deletion.
- Biography regressions retain substantive same-origin evidence, exclude invalid namesakes only for simple questions, keep both sources for compound questions, and reject unsupported identity assertions in findings and gaps after restart.
- Trusted prompt-mode tests cover supported prompt formats, ordinary-turn reset and source/user text that cannot activate trusted schema metadata.

## Preserved diagnostic attempts

All preceding attempts remain separate from the final matrix. `completeness1` passed 10/12 targeted turns, with one accepted borderline status judgment; SQLite report mismatches and categorical identity assertions failed. `completeness2` passed 11/12, with two accepted borderline wordings; its restarted biography hit the existing loop detector after repeating a rejected namesake citation. The `completeness3` targeted biography diagnostic passed all three turns without rejected calls, including identity uncertainty and continuation. Duplicate quotations in one answer remain a cosmetic issue.

The completeness3 development matrix, frozen at executable SHA-256 `e60b538a3451cbc6d4dede1178796b5154dbb8a88d367ab084fea8fd94427b9e`, was interrupted after 112 complete turns (all 4B seeds, 27B seed 1, and four turns of 27B seed 2). All were reviewed: 108 main passes, 103 strict passes. Three SQLite initial answers and one long-page restart correction failed. The latter exposed a deterministic bug: the excluded phrase “do not repeat the comparison” activated comparison guidance and disabled focused source pruning. A failing regression reproduces that retained obsolete passage. The follow-up uses the positive request for both decisions and checks guidance again after reopening storage. These development attempts remain separate from the final full matrix and are not independent validation.

The final completeness4 diagnostic passed all eight targeted turns without rejected calls, with one accepted borderline French per-finding citation judgment. The corrected long-page restart had only the requested host rule in its fitted evidence view. SQLite, French and Persian correction checks also passed. These diagnostic outputs are preserved separately from the full run.

The verified [selection8 archive](2026-11-20-evidence-selection.tar.gz) remains immutable at SHA-256 `724d97c5331d373335f50576be14caf4f32b7f2aa7bd3b60ed307d3915563019`; its main score was 213/240, compared with 209/240 in the [earlier quality evaluation](2026-11-20-quality-report.md). The present 227/240 is another exposed development result, with thirteen substantive failures still visible.

The [completeness4 archive](2026-11-20-evidence-completeness.tar.gz) has SHA-256 `fe4bb3a072c05abfb4241912837ad133976a517675aae6671dc567abac4f585a`. The [summary](2026-11-20-evidence-completeness-summary.json) and [verification](2026-11-20-evidence-completeness-archive-check.json) record matching executable, model and source hashes, identical extracted replay results, and all 39 replay/inference controls passing. It preserves raw answers, actual fitted views, review reasons, source snapshots, earlier attempts and power/test logs. It does not include model weights or constitute new live-provider, UI or Android validation.
