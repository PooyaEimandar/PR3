# Evidence selection and answer completeness — 20 November 2026

Both matrices and all 240 source reviews are complete. This is the **intermediate selection8 build**, preserved before the follow-up completeness fixes. Its remaining failures motivated that follow-up; it is not the final acceptance result for the current implementation.

## Change and scope

The previous evaluation showed that storing the right page was insufficient: the inference window could discard the requested rule or separate it from its condition. The captured Python window omitted the `strict=True`/`ValueError` condition, while the Git window could favor a similarly worded external-helper rule over the actual `--exit-code` option. Report repair could then drop another requested passage as the prompt grew.

Selection now ranks requested phrases and option scopes, preserves short antecedents and intervening examples as exact contiguous ranges, avoids overlapping excerpts, and distributes a shrinking inference budget across passages of different lengths. The host rebuilds evidence from current task-owned storage after restart and respects later corrections, task isolation and deletion. Report guidance permits the source explanation itself to answer a question, asks for every requested part, and preserves conditions and uncertainty. A conservative check rejects a long finding that attaches an uncited second subject's distinctive details to an otherwise relevant quotation.

The benchmark records the source views that actually fit each inference prompt, their token counts, and the reserved answer space. This diagnostic is disabled by default; the benchmark explicitly enables it. It records selected source evidence, not full private conversation prompts. Capture does not change the generated prompt. The inference-only report schema describes structured findings and citations without changing tool registration or runtime authorization.

Quotation checks, task/source binding, deletion, Web permissions and approval policy remain enforced. Limits remain four tool rounds, four calls per round, twelve calls total, a 4,096-token context and a 512-token answer reservation. No extra repair attempt or failed-case retry is added to either final matrix.

## Evaluation method

Both models use seeds 1/2/7: `Qwen3.5-4B-Q4_K_M.gguf` and `Qwen3.8-27B-Q4_K_M.gguf`. The development corpus has 18 scenarios and 27 turns per run (162 total); the previously fresh corpus has eight scenarios and 13 turns per run (78 total). All case objects and page bytes are unchanged from the [previous evaluation](2026-11-20-quality-report.md). They are now exposed regression cases, including the former generalization set. This is PR3-only evidence, not a new holdout or a current OpenJarvis comparison.

The evaluated executable is frozen at SHA-256 `748112b6c1aa683bec5ca55a7e922511ae93d4156eafe8a3cc7682316912d689`, with a 139-file production source overlay and model/corpus hashes. Runs use the production Host, tools, SQLite and real local inference with offline page fixtures. They do not measure live Tavily delivery or Android latency. Restart cases reopen SQLite and create a new Host while leaving the model loaded.

Every actual answer receives unblinded implementation-assistant review against the latest request and cited sources. This is not independent human validation. Main-rubric, stricter and lenient scores keep borderline judgments visible; automatic checks remain separate. Exact quotations do not establish semantic entailment or completeness.

The seeds and engine settings are matched, but task identifiers, capture timestamps and their bound source references are generated at runtime. These are repeated stochastic runs, not bitwise-identical prompt replays. Small score differences are descriptive; they do not establish statistical significance or broader generalization. One stricter-review judgment was corrected after checking consistency with the preceding evaluation; the before/after decision is retained in `review-corrections.jsonl`, and the main score did not change.

## Development results

| Model | Previous → current main | Current strict | Current lenient | Automatic checks |
|---|---:|---:|---:|---:|
| 4B | 76/81 → 78/81 | 75/81 | 78/81 | 80/81 |
| 27B | 80/81 → 78/81 | 73/81 | 78/81 | 81/81 |

Development main-rubric performance is unchanged in aggregate: 156/162. The 4B increase is offset by the 27B decrease. All 162 turns produced a runtime-validated report, but six answers fail the content rubric:

- 4B, all seeds: the real SQLite comparison omits requested commit-reader behavior. Seed 1 also quotes an obsolete large-transaction recommendation without its later-version qualification.
- 27B, seed 1: a rejected mixed-source finding is followed by a repair that loses the WAL reader passage and reports the missing comparison as unavailable.
- 27B, seed 2: the long-page Reed finding reverses waiting for existing readers into blocking those readers from finishing.
- 27B, seed 7, after restart: the documentation exception is restated as an exception to an invented “main license,” despite the saved engine public-domain statement.

Eight accepted borderline judgments are excluded from the strict score: three speculative 4B official-source clarification examples, two 27B causation/citation wordings, one capture-date wording and two SQLite quotation-context judgments. The raw review explains each decision.

Direct replay verifies all 267 displayed quotations, all 54 nonempty unchanged restart snapshots, zero restart retrieval turns, and the existing storage/execution caps. There are no runtime errors or empty-placeholder gaps. Core traces retain 14 rejected calls at 4B and one at 27B; final report validation does not erase those attempts.

The inference audit verifies 312 fitted quotations over 328 inference passes. The largest prompt is 3,573 tokens with the unchanged 512-token answer reservation, within 4,096 tokens. All requested facts reach at least one report prompt in each of the six SQLite initial turns and all twelve long-page initial turns. SQLite loses complete coverage during repair in three of those six turns; both long-page cases preserve coverage in every report pass. Evidence availability is not a semantic answer score.

Observed median/p95 turn times are 7.67/13.51 seconds for 4B and 40.30/89.82 seconds for 27B; maxima are 32.50 and 202.16 seconds. Wall and monotonic turn times differ by less than one second throughout this matrix. These are desktop observations under the power/load conditions below, not Android latency or a controlled improvement claim.

## Exposed regression results

| Model | Previous → selection8 main | Selection8 strict | Selection8 lenient | Automatic checks |
|---|---:|---:|---:|---:|
| 4B | 23/39 → 23/39 | 19/39 | 28/39 | 30/39 |
| 27B | 30/39 → 34/39 | 24/39 | 35/39 | 35/39 |

The combined development/exposed main score is 213/240, versus 209/240 in the previous quality evaluation. This small descriptive change does not establish generalization. Two 4B turns produce no validated report: Cargo seed 1 fails report-shape checks, and Git seed 2 fails citation-reference checks. All other turns produce validated reports; the content review still finds 27 failures across both matrices.

At 4B, exposed failures include a missing Python strict-mode antecedent, incomplete Git coverage, all three identity-uncertainty conclusions, Spanish current-price uncertainty, and French/Persian correction scope. At 27B, five exposed answers fail: one French correction repeats retention, one biography attaches uncited namesake details, and all three initial Persian answers omit a requested part. Fourteen accepted exposed borderline answers and six borderline failures remain separately recorded, including unnecessary namesake material, incomplete per-finding citations, quotation context and capture-date wording.

The exposed replay verifies 180 displayed quotations and all 30 nonempty unchanged restart snapshots, with zero restart retrieval turns. Core traces retain 15 rejected calls at 4B and three at 27B. There are no runtime errors or placeholder gaps. The inference audit verifies 362 fitted quotations over 174 passes; the largest prompt is 3,582 tokens plus the unchanged 512-token answer reservation. All 18 initial technical-page turns contain all requested evidence in at least one report prompt; two 4B Git turns lose some coverage during repair. Correct evidence in the prompt still does not ensure a complete answer.

Observed exposed median/p95 turn times are 11.59/30.47 seconds at 4B and 61.46/174.21 seconds at 27B, with maxima 39.19 and 192.82 seconds. No wall/monotonic clock divergence was detected. These are observational desktop timings, not a controlled comparison or Android measurements.

## Verified archive

[Raw archive](2026-11-20-evidence-selection.tar.gz), [summary](2026-11-20-evidence-selection-summary.json) and [replay verification](2026-11-20-evidence-selection-archive-check.json) preserve all 240 turns, source reviews, source overlays, prior partial attempts, inference views and checks. Archive SHA-256: `724d97c5331d373335f50576be14caf4f32b7f2aa7bd3b60ed307d3915563019`.

The verifier checked the source overlay against the working files before follow-up edits, both model files and the frozen executable, then extracted the archive and reproduced both summaries and their controls. All 21 replay checks and 16 inference-audit checks passed. Across both matrices, 447 displayed quotations and 84 unchanged restart snapshots verify. Execution caps remain enforced; no failed-case retry is substituted.

## Regression validation

- Complete Rust workspace suite: 457 passed, 10 ignored; the ignored real-tokenizer budget test also passed separately.
- Strict workspace Clippy passed. TypeScript checks and the 4 Stop/9 research harness tests passed.
- New regressions cover conditional examples/defaults through both selection stages, command-option scope, full captured technical-page variants under constrained budgets, and disk-backed restart/correction/isolation/deletion.
- Support-check regressions reject transferring uncited namesake details while accepting supported multi-source findings and conservative paraphrases. The existing bounded correction test also verifies ordered diagnostic capture without extending the repair limit.
- A quotation-only report prompt keeps requested source-instruction inspection separate from the runtime-owned authority explanation. Its regression checks all supported prompt formats, preserves source isolation and verifies that source text cannot activate the mode. The explicit narrow schema asks for source/literal quotation pairs; runtime citation checks are unchanged.
- No native UI, live Tavily or physical Android acceptance was performed for this change.

## Preserved development attempts

The evidence archive retains the audit-only build, selection candidates, diagnostic outputs and failed validation attempts. A preliminary selection4 development matrix was interrupted after 14 complete 4B turns: all passed the main content review, with one borderline wording judgment and eleven rejected calls in core traces. Its partial output is separate from the final matrix. Later diagnostic failures are preserved too, including selection5's incomplete Git report and selection6's Python restart quotation failure despite the required passage being present.

A second candidate, selection6, was stopped after 48 complete development turns when source-instruction inspection failed after restart at both completed 4B seeds. All 48 answers were reviewed: 45 main-rubric passes, two accepted borderline clarification answers, and fourteen rejected trace calls. A literal-citation-only schema in selection7 still failed two of the three targeted restart checks. Selection8 adds the trusted quotation-only prompt; its six-turn diagnostic passed at seeds 1/2/7, with no rejected inspection calls. Initial price turns still needed three missing-gaps repairs in total. All attempts are retained; only selection8 contributes to the final matrices.

These attempts were used to develop the fix and are not substituted for final results. A process-bound idle-sleep assertion is used for the final runs. The final run started on battery at 17:37:52 UTC and returned to AC power at 17:55:23 UTC. Mixed power conditions and varying desktop load prevent a controlled speed-improvement claim.

Sampling ended at 19:49:53 UTC. Across 265 samples, no gap exceeded 60 seconds and no compiler process was sampled as active; unrelated desktop CPU load varied. The verified selection8 archive was created before applying the follow-up completeness changes.
