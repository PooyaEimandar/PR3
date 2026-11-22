# Research evidence coverage and bounded completion — 22 November 2026

**Complete: 240/240 main passes, 220 strict and 240 lenient**, with 4B and 27B at seeds 1/2/7. All eight failing configurations from the preceding [report-contract evaluation](2026-11-21-research-contract-report.md) now pass main review; three retain strict-review concerns about extra background quotations. No new main failures were found. All 84 restart answers pass main and strict review. The previous scores were 232 main, 212 strict and 233 lenient. These are exposed, unblinded regression results, not independent generalization evidence. The final archive was extracted and replayed successfully; superseded attempts remain separate.

## Implementation

Factual findings now use checked quotations directly. The inference schema omits optional restatements, and the report grammar honors that schema. Requested inference receives a separate conclusion, including causal questions, recommendations, current validity and compound identity questions. This is a bounded presentation hint from the active user question, not a general semantic classifier. Negative instructions such as “do not imply a live check,” and incidental substrings such as “because,” cannot activate the tested inference hints. Corrections rebuild the schema rather than inheriting the preceding question's conclusion requirement. Existing runtime conclusion and citation validation remain in charge.

Each fitted evidence view carries a bounded coverage plan. It retains a primary quotation from each selected origin, then complementary requested concepts, default/option scope and condition-bearing passages. A full exception sentence can qualify on one distinctive requested term; weak function words and very short navigation fragments do not count as separate facts. The plan is recomputed after token fitting, never copied unchanged from a larger view. It contains at most eight source-bound references, grouped into at most four findings. Generation must include those references and cannot pad the plan with duplicate optional findings. Short exact anchors remain checked by the runtime. The source bytes, ranges, task ownership, website restrictions and deletion checks are unchanged.

Requested concepts are tracked separately for each origin: one subject's restoration rule cannot stand in for another subject's rule. A terse continuation refits excerpts using the same trusted task question as the coverage plan; substantive new wording replaces it. French, Spanish and English regressions cover separately stated comparison conditions and refitting.

The existing narrow English identity relationship check now treats “individual,” “individuals” and “persons” consistently with “person” and “people” in both claims and evidence. Unsupported categorical identity claims remain rejected after continuation and reopening, including a categorical sentence following an uncertainty sentence. Explicit uncertainty and supported identity links remain valid. This is not general entity resolution or semantic entailment validation.

If a real report result rejects an unsupported identity conclusion, its next inference receives a bounded conclusion enum stating that an identity match or separation remains unverified. This is the validation outcome, not a claim of exhaustive identity research. The model still generates a report whose quotations pass normal validation. Only the actual failed report result activates this repair, not source text imitating that error. The flag is local to the turn, resets on other tool outcomes and cannot leak across a restart or later correction. Both tool formats support the enum without adding a round.

The plan is lexical extraction, not a proof of relevance, completeness or entailment. It may include unnecessary background passages, and it cannot guarantee that missing or poorly selected evidence answers a question. Model-written conclusions and gaps still require review. Unsupported interpretation is not solved generally by proving that a quotation exists.

When the host requires an unread candidate, non-thinking generation uses a separate read grammar derived only from the host's offered candidate enum. It permits ordered subsets of distinct candidates, up to four per round. Ordinary prose, a foreign candidate and a repeated candidate in that batch cannot replace the required read. Execution still passes through the original source checks, loop detection and counters. No action is synthesized from prose, no rejected result is silently salvaged, and no extra retry is added.

Automatic grammars remain disabled for thinking mode or tool sets containing consequential actions. Explicit caller grammars retain precedence. Approval policy and limits remain unchanged: **four tool rounds, four calls per round, twelve calls total, 4,096 context tokens and a 512-token answer reservation**.

## Validation and frozen build

- **502 workspace tests passed; 13 ignored.** Four ignored native tokenizer/grammar tests passed separately.
- Formatting, strict workspace Clippy, whitespace checks and the research evaluator build passed.
- Regressions cover required source reads, duplicate/foreign candidates, action substitution, four-call bounds, omitted comparison evidence, unsupported finding restatements, affirmative default rules, single-term exceptions, negative inference wording and both tool formats. Identity repair tests include forged source errors, repeated turns, reopened storage, later corrections and native grammar rejection of conclusions outside the offered enum.
- Existing restart, correction, task isolation, deletion, source integrity, approval, cancellation and execution-budget tests remain green.

Final build `coverage12` has SHA-256 **1a8aded430485d923fb2210d9ddb1b75af37ec2d27552f7f6b3aec84b9023f8d**, with **144 frozen source files**. The work is based on `cc0f5a76ba302dcd31c6074bf1e391f93343ba65`, including the previously committed performance/memory work. Comparison with the preceding archive therefore does not isolate the effect of this patch.

## Evaluation and review method

The existing development matrix has 162 turns and the exposed regression matrix has 78. Both use unchanged cases/page bytes, `Qwen3.5-4B-Q4_K_M.gguf` and `Qwen3.8-27B-Q4_K_M.gguf`, each at seeds 1/2/7. No failed-case retry substitutes for a final answer. Every actual answer is reviewed against the latest request and its source evidence by the implementation assistant, unblinded. Material omissions, unsupported claims and incomplete reports fail; borderline judgments are retained for strict/main/lenient sensitivity. This is exposed regression evidence, not independent held-out validation or a new matched OpenJarvis comparison.

The evidence audit checks exact quotations, source bindings, ranges, prompt size and answer reservation. A separate plan audit aligns each actual model generation with its fitted view and checks that required references appear in report calls. Positive and deliberately corrupted controls test both verifiers. Restart snapshots and retrieval events are checked separately from answer quality. Evidence availability and plan compliance do not replace semantic review.

Repeated answers to the same case and turn may be grouped for review only when the displayed answer is byte-identical after removing internal citation identifiers. All wording, quotations and source labels remain in the comparison; new wording is reviewed separately. This groups identical outputs within the final frozen matrix, never substitutes an earlier run's answer. Source bindings and runtime results are still checked for each turn, and a separate consistency check compares the entered judgments for identical displays.

Earlier attempts remain separate. The initial sandbox diagnostic could not load a model. An early Metal diagnostic exposed duplicate read batches and repeated findings. Later pruning attempts failed the Cargo default-condition regression. The superseded `coverage7` full-matrix attempt was stopped after 13 reviewed turns (12 main passes); it exposed a missing documentation exception and an unnecessary conclusion triggered by negative wording. All these artifacts are preserved rather than combined with the final matrix.

The superseded `coverage9` run completed development at 162/162 main and 159 strict, but exposed review found a missing French local-key restoration condition that the automated diagnostic did not catch. All 177 completed answers are preserved separately (176 main and 171 strict passes). The next code revision adds coverage per origin and uses the retained task question during continuation refitting. Its separate 13-turn exposed preflight passes main review, with Git/Cargo background-padding concerns and one identity-guard rejection repaired within existing limits. No preflight or superseded answer substitutes for a final-matrix answer.

The `coverage10` fresh matrix was then stopped after an unsupported “two different individuals” conclusion bypassed the narrow identity wording check. All 15 completed answers are preserved (14 main, 11 strict), including the corrected French comparison. The noun-variant regression reproduced that defect before the final identity-check fix.

The `coverage11` matrix correctly rejected the unsupported identity claims, but 4B seed 2 repeated them during repair and ended incomplete. All 39 completed turns are preserved separately (38 main, 30 strict). The next revision adds the cautious conclusion format to that existing repair opportunity. No result from this partial run substitutes for a final answer.

## Results and acceptance

| Matrix | Model | Main | Strict | Lenient |
|---|---|---:|---:|---:|
| Development | 4B | 81/81 | 79/81 | 81/81 |
| Development | 27B | 81/81 | 80/81 | 81/81 |
| Exposed regression | 4B | 39/39 | 31/39 | 39/39 |
| Exposed regression | 27B | 39/39 | 30/39 | 39/39 |
| Total | Both | **240/240** | **220/240** | **240/240** |
| Restart subset | Both | **84/84** | **84/84** | **84/84** |

Each model scores 120/120 main and 110/120 strict across both matrices. Per-seed scores preserve the complete denominators:

| Matrix | Model | Seed | Main | Strict |
|---|---|---:|---:|---:|
| Development | 4B | 1 | 27/27 | 26/27 |
| Development | 4B | 2 | 27/27 | 26/27 |
| Development | 4B | 7 | 27/27 | 27/27 |
| Development | 27B | 1 | 27/27 | 27/27 |
| Development | 27B | 2 | 27/27 | 27/27 |
| Development | 27B | 7 | 27/27 | 26/27 |
| Exposed regression | 4B | 1 | 13/13 | 10/13 |
| Exposed regression | 4B | 2 | 13/13 | 10/13 |
| Exposed regression | 4B | 7 | 13/13 | 11/13 |
| Exposed regression | 27B | 1 | 13/13 | 11/13 |
| Exposed regression | 27B | 2 | 13/13 | 10/13 |
| Exposed regression | 27B | 7 | 13/13 | 9/13 |

The eight preceding failures are resolved as follows. Turn indices start at zero; turn 1 reopens storage.

| Previous failing configuration | Final behavior | Main / strict |
|---|---|---|
| Development, 4B seed 2, `replay-h08`, turn 0 | Persian prices and residency conditions retained without the reversed fee claim | Pass / pass |
| Development, 27B seed 2, `replay-h08`, turn 0 | Correct conditions without claiming a paid membership is cheaper than free membership | Pass / pass |
| Development, 4B seed 2, `public-sqlite-comparison-restart`, turn 0 | WAL readers, rollback reader exclusion and same-host restriction all covered | Pass / pass |
| Exposed, 4B seed 1, `fresh-git-diff`, turn 0 | Outside-repository use and both exit-code meanings covered; extra passages remain | Pass / fail |
| Exposed, 27B seed 1, `fresh-git-diff`, turn 0 | Outside-repository use and both exit-code meanings covered; extra passages remain | Pass / fail |
| Exposed, 4B seed 7, `fresh-similar-name`, turn 0 | Correct historian biography in a validated report | Pass / pass |
| Exposed, 27B seed 2, `fresh-cargo-features`, turn 1 | Saved implicit-feature suppression rule without unsupported exclusive activation | Pass / pass |
| Exposed, 27B seed 7, `fresh-cargo-features`, turn 0 | Default-feature conditions and suppression rule covered; extra examples remain | Pass / fail |

The **20 accepted borderline answers** are not full-quality successes. Twelve Git/Cargo initial answers include unnecessary background quotations. Three development clarification answers introduce irrelevant organizations, assumed subject matter or invented example brand domains. Two identity answers overstate differences or missing mentions while preserving explicit identity uncertainty. Three Spanish date answers have imprecise temporal/price wording; one also contains an English conclusion. All are retained as strict failures in the source-review records. The consistency check found 64 distinct displays across 240 turns and 36 repeated-display groups, with consistent judgments for every identical case/turn/display.

One review boundary is explicit: in the exposed 27B seed 2 Spanish price case, the Spanish source quotations and Spanish runtime uncertainty notice provide the requested facts and temporal explanation, but the additional model conclusion is English. It is a borderline main pass and a strict failure, not full language compliance. The conclusion also calls the temporal statuses inconsistent, although their validity windows are not established to conflict. Both defects remain in the review record.

## Integrity, limits and reproducibility

All 240 turns ended with a validated report and no runtime error. **408 displayed quotations** verify against their saved sources. The inference audit checks **474 quotations across 472 inference passes**, including exact ranges and source bindings. Maximum fitted prompt size is 3,235 tokens on development and 3,583 on exposed cases, with the full 512-token answer reservation retained. Recorded maxima are four tool rounds and five tool calls in a turn; no configured limit was exceeded.

All **84 restart snapshots remain unchanged**, all 84 focused restart evidence checks pass, and no restart turn performs search/read retrieval. All 36 focused initial coverage checks pass. The 24 long-page case snapshots retain every requested fact, and 42 displayed citations refer to passages beyond part 12.

The plan audit checks 231 generated report attempts containing 408 required citation selections, with **zero omitted planned references**. Twelve development reports use the separate explicit-instruction-quotation or empty-evidence clarification paths. Counts include three rejected report attempts: two unsupported identity conclusions were repaired using the bounded neutral format, and one Spanish date answer was repaired after a conservative validity rejection. The rejected Spanish answer already expressed current-price uncertainty; reducing such avoidable rejections remains a quality and latency concern. These are real failed attempts followed by checked reports within the existing budgets, not first-attempt successes or substituted retries.

The [raw archive](2026-11-22-research-coverage.tar.gz), [recomputed summary](2026-11-22-research-coverage-summary.json) and [extraction/replay verification](2026-11-22-research-coverage-archive-check.json) preserve the complete final matrices, source reviews, source overlays, validation logs, power samples, comparison/consistency checks and separate superseded attempts. All **49 positive and corruption controls** pass. Extracted replay, inference and plan audits reproduce the originals. The frozen executable, both model files and all 144 production source hashes match their manifests; the previous archive's hash also verifies.

Archive SHA-256: **d65b109034a2c4d9a74a57454d0cc843126ab80adbcd94bc2319673f1719c9a5**. Replay requires Node with TypeScript support, but no model weights or executable:

```sh
review_dir=$(mktemp -d)
tar -xzf bench/research/evidence/2026-11-22-research-coverage.tar.gz -C "$review_dir"
node "$review_dir/development/replay.ts"
node "$review_dir/exposed/replay.ts"
node "$review_dir/development/check-replay.ts"
node "$review_dir/exposed/check-replay.ts"
```

## Observed performance and remaining acceptance

The final matrix ran from **15:44:24 to 18:09:59 UTC on 22 November**. All 293 power samples report AC power, with no sampling gap over 60 seconds and no material divergence between turn and wall clocks. Background compiler activity appears in 44 samples; other processes reached 638.9% aggregate CPU and one-minute load reached 15.40. These conditions, different generated answers and the previously committed performance work prevent an isolated speed claim.

| Matrix | Model | Median turn (s) | p95 (s) | Maximum (s) |
|---|---|---:|---:|---:|
| Development | 4B | 9.62 | 16.94 | 18.08 |
| Development | 27B | 56.41 | 96.88 | 111.05 |
| Exposed regression | 4B | 10.89 | 29.64 | 29.99 |
| Exposed regression | 27B | 68.72 | 128.02 | 134.56 |

These runs use the production Host with offline fixture pages. Timings exclude model loading and include repairs. Restart cases reopen storage and create another Host while retaining the loaded model. They do not establish live Tavily, native UI, cross-device or Android acceptance; no mobile latency claim is made. The earlier online-repair APK remains the last documented Android installation.

Next acceptance work is concise quotation selection, requested-language compliance and precise clarification/uncertainty wording, followed by live Tavily/native UI and user-run Android checks on this build. Measure latency under controlled power/load and freeze a new independently reviewed corpus before making fresh generalization or matched OpenJarvis claims. Exact quotation integrity and a complete coverage plan do not establish general semantic understanding.

During the superseded `coverage9` development 27B seed 2 run, macOS entered low-power sleep at 17:45:35 and woke on AC attachment at 17:56:04 (Asia/Dubai). The same evaluator process continued, preserving the interrupted case rather than substituting a retry. The power log and a brief post-wake stack sample are retained. High background load and memory pressure also affected that stretch; its elapsed and wall-clock timings must be read with those conditions.
