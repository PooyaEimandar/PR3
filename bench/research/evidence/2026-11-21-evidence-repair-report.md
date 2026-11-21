# Evidence retained through report repair — 21 November 2026

**Completed:** 232/240 main passes, 218/240 strict and 232/240 lenient, with all 84 restart answers passing the main review. Both models completed seeds 1/2/7; every actual answer was reviewed. Eight substantive failures remain. The archive was extracted and replayed, and all 39 verifier controls passed. These are exposed regression results, not a new holdout or matched OpenJarvis comparison.

## Change and regression proof

The [preceding completeness4 evaluation](2026-11-20-evidence-completeness-report.md) passed 227/240 main reviews and all 84 restart turns. It also exposed deterministic evidence loss during report repair: when an error message increased the prompt size, a two-passage candidate fit first and dropped the outside-repository Git passage, even though a balanced three-passage view could retain it within the same budget. This occurred in 4B seeds 1/7 and 27B seed 7.

The fitter retains its first fitting candidate as a fallback, then tries four bounded refinements from the original evidence. A replacement must preserve each baseline primary quotation's exact bytes, source and range; retain the existing requested lexical coverage, option headings and rule-scope signals; and add coverage from source text. Metadata does not count. The full protected prompt is then measured with the actual model tokenizer. These are conservative lexical selection checks, not a proof of semantic entailment or completeness.

Stored sources and conversation messages are unchanged by fitting. Only marked tool evidence can be reduced. Trusted instructions, the latest request, error/status messages, source/task binding, deletion, approval checks and execution limits remain intact: four rounds, four calls per round, twelve calls total, 4,096 context tokens and a 512-token answer reservation. This repair builds on the preceding task-continuation, focused-correction, phrase/scope selection and provenance changes described in the linked report.

The new real-tokenizer regression varies trusted prompt pressure while retaining a report-repair error. Before the fix, it fails at padding 360 with a 3,357-token prompt that has lost `--no-index`, despite a complete alternative fitting. After the fix, all exercised fitting windows retain the outside-repository and exit-code conditions, preserve stored messages and respect the answer reservation. Five ordinary regressions cover exact primary conditions, source/range binding, UTF-8 offsets, metadata-only changes, and the loss of option/default scope. Existing SQLite, Python, long-page, correction, restart, deletion and task-isolation regressions still pass.

A broader ranking experiment was rejected after it regressed three existing evidence-selection tests. Its failure logs and patch are preserved separately and it is not part of the delivered code. A test-only Clippy warning was corrected before the final build; all attempts remain recorded.

## Frozen scope and validation

Executable `repair1` has SHA-256 `cc5e5ec2334319cddb28dbc3165caad49072e96398d91e7aa8cb5eafe596543a`, with 140 frozen source files. Models are `Qwen3.5-4B-Q4_K_M.gguf` and `Qwen3.8-27B-Q4_K_M.gguf`, each at seeds 1/2/7. The unchanged development matrix has 162 turns and the exposed regression matrix has 78. There is no failed-case retry substitution.

Every actual answer receives unblinded implementation-assistant review using the preceding main/strict/lenient criteria. A runtime-valid report can still fail for missing requested facts or unsupported conclusions. Accepted citation/wording borderlines reduce the strict score; rejected borderlines increase the lenient score. This is exposed regression evidence, not independent review, a new holdout or a matched OpenJarvis comparison. Runtime-generated references and capture timestamps mean seeds do not produce bitwise-identical prompts across runs.

The production Host, SQLite and real local inference run against offline page fixtures. Restart turns reopen SQLite and create another Host with the model still loaded. This does not establish live Tavily, native UI, cross-device or Android acceptance.

- Full Rust workspace: 472 passed, 11 ignored. The two ignored real-tokenizer budget checks passed separately.
- Strict workspace Clippy, formatting, build and whitespace checks passed.
- TypeScript checks and four Stop/nine research harness tests passed in the preceding build; this repair changes only the Rust fitter and its tests.
- A process-bound `caffeinate -dims` assertion accompanies this final run. Power, compiler/background load, monotonic duration and wall timestamps are recorded; observational desktop timing is not a controlled speed claim.

Although this task's validation finished before evaluation began, sampling recorded compiler activity at 04:22:34 and 04:23:04 UTC, and other-process CPU exceeded 1,000% at 04:26:04 and 04:30:04 during the first 27B seed. The sampler does not identify the source of that background work. Those timings include machine contention as well as any recorded report retries; they cannot isolate the repair's speed impact.

## Complete results

| Matrix / model, seeds 1/2/7 | Main | Strict | Lenient |
|---|---:|---:|---:|
| Development / 4B | 81/81 | 77/81 | 81/81 |
| Development / 27B | 80/81 | 77/81 | 80/81 |
| Exposed / 4B | 34/39 | 33/39 | 34/39 |
| Exposed / 27B | 37/39 | 31/39 | 37/39 |
| Total | **232/240** | **218/240** | **232/240** |

All 84 restart answers pass the main review; two Cargo corrections have incomplete local citation support and therefore do not pass strict review. All six Git answers now include outside-repository comparison and exit codes. All long-page Orchid/Reed and Delta answers, biography/Continue pairs, and French comparison/correction pairs pass. A validated report alone is insufficient: 237/240 reports validated, but five of those still fail semantic review. Three turns never produce a validated report.

The previous complete matrix scored 227/240 main and 214/240 strict. This run adds five main passes and four strict passes, but cases are exposed, review is not independent and generated references vary across runs. This difference is not a statistical significance or generalization claim. The deterministic before/after tokenizer regression is the direct proof of the fitting repair.

### Eight remaining main failures

| Matrix / model / seed | Case | Failure |
|---|---|---|
| Development / 27B / 7 | SQLite comparison | Omits rollback reader exclusion during commit. The full condition and antecedent are present in the actual 3,433-token report prompt. |
| Exposed / 4B / 2 | Python | No validated report after two citation-reference/anchor errors. |
| Exposed / 4B / 2 | Cargo | No validated report after two missing-gaps schema errors. |
| Exposed / 4B / 2 | Same-name identity | No validated report after two unsupported identity conclusions are rejected. |
| Exposed / 4B / 7 | Same-name identity | Categorically asserts that the profiles establish different people and mistakes missing mentions for explicit exclusions. The narrow guard misses the reworded claim. |
| Exposed / 4B / 7 | Spanish current price | Calls the undated catalog price current without explicitly concluding that current price cannot be established. |
| Exposed / 27B / 1 | Persian recovery | Omits requested 45-day retention despite its presence in the 2,814-token report prompt. |
| Exposed / 27B / 7 | Persian recovery | Replaces device-owner approval with device-ownership verification in the explanation, despite the correct attached quotation. These are different requirements. |

The SQLite and Persian omissions demonstrate unused evidence rather than context exhaustion. The ownership statement is a generated interpretation error; no recovery action executes or runtime approval policy changes. Correct quotations do not excuse materially incorrect accompanying prose. Every failed initial turn remains counted even when its focused restart correction succeeds.

### Fourteen accepted borderlines

Development has seven: 4B official-source clarification gives speculative authority examples at all three seeds; 4B seed-1 causation cites a control-group fact only in another finding; 27B seed-1 SQLite places a pre-commit quotation under a during-commit heading without explaining the transition; 27B seed-2 SQLite repeats facts supported only in other findings; and the 27B seed-2 Persian comparison ends with an ambiguous phrase about no difference in conditions.

Exposed regression has seven: 4B seed-7 Python omits the strict=True antecedent from its ValueError quotation; 27B Cargo initial answers at seeds 1/2 and restart answers at seeds 1/7 explain the implicit feature's same-name definition without quoting that detail; 27B seed-2 Persian ambiguously suggests verifying the owner rather than obtaining the owner's approval; and 27B seed-7 Git attaches a driver-specific quotation to a general exit-code claim, whose correct support appears elsewhere in the answer. Seed-7 Persian explicitly changes the requirement to ownership verification and fails the main review rather than receiving a borderline pass.

All judgments and reasons are preserved in each matrix's source-review.json. Duplicate supported findings, redundant status prose and old research-dialog wording remain presentation issues. They alone do not change these scores.

## Evidence, restart and limit audit

The extracted replay verifies **431 displayed quotations**, **661 fitted quotations across 497 inference passes**, and **84 unchanged, nonempty restart snapshots** with zero restart SEARCH/READ calls. All 24 long-page snapshots contain the requested late facts. All 36 audited initial technical/late-page turns and all 84 restart turns retain the required literal evidence in every report-pass view. These coverage checks follow exposed rubrics and do not prove semantic use or entailment.

Git retains all required literal evidence through every report attempt in all six runs, compared with only three of six in the preceding complete matrix. This includes four Git report attempts at 4B and five at 27B. The largest fitted prompt is **3,579 tokens plus the unchanged 512-token answer reservation**, within 4,096. Traces stay within four rounds and at most five total calls, below the unchanged twelve-call cap. No empty gaps placeholders or turn-level runtime errors were recorded; schema, citation and support rejections remain in the traces and timing totals.

The 39 controls include accepting untouched evidence and rejecting changed quotes, foreign sources, changed offsets, altered frozen corpora, missing/duplicate reviews, incomplete matrices, changed restart capture times, reduced answer reservation and exceeded limits. Removing only restart evidence views reduces restart coverage without changing initial coverage. Replaying the extracted archive reproduces all summaries, inference audits and control results.

## Observed performance

| Matrix / model | Median seconds | p95 seconds | Maximum seconds |
|---|---:|---:|---:|
| Development / 4B | 6.84 | 13.41 | 33.37 |
| Development / 27B | 38.14 | 106.78 | 152.25 |
| Exposed / 4B | 10.65 | 28.75 | 39.10 |
| Exposed / 27B | 60.28 | 144.86 | 156.97 |

Warm-turn monotonic durations include all repairs; model loading is excluded. Wall clocks agree within the audit threshold on every turn. Sampling spans **03:58:04–06:05:56 UTC on 21 November**: 257 samples, AC power throughout, no gaps over sixty seconds, and two compiler-active samples. Background CPU and compiler contention described above remain confounds. These are desktop observations, not controlled before/after speed measurements or Android latency. The slowest exposed turn is 27B seed-1 Cargo after a schema repair; several other repaired 27B answers take over two minutes.

## Preserved evidence and next acceptance

The [raw archive](2026-11-21-evidence-repair.tar.gz), [summary](2026-11-21-evidence-repair-summary.json) and [archive verification](2026-11-21-evidence-repair-archive-check.json) retain both full matrices, all reviews, source overlays, fitted evidence, clocks/power samples, validation logs and the before/after regression proof. Archive SHA-256: **e541c92bec95034acfaedd844fc9c9eac690ec5beaf00a8c9b40a4792649be58**. Source, executable and both model-file hashes match their frozen manifests. Model weights and executables are not bundled.

The preceding [completeness4 archive](2026-11-20-evidence-completeness.tar.gz) remains immutable at SHA-256 fe4bb3a072c05abfb4241912837ad133976a517675aae6671dc567abac4f585a. Earlier quality, selection and partial attempts remain separate; no failed-case retry replaces a final-matrix answer.

After extraction, run `node development/replay.ts` and `node exposed/replay.ts` from the extracted directory. Each matrix also contains `check-replay.ts`, `audit-inference.ts` and `check-inference.ts`; pass its matrix directory to the inference scripts. Replays require Node with TypeScript support, not model weights or network access.

Next work is to reduce malformed report/citation attempts and unsupported or incomplete interpretations within the same limits, followed by live macOS Tavily and user-run Android acceptance, controlled latency measurements and a newly frozen independently reviewed holdout. This build has not been newly deployed to Android or accepted in the native app. Research parity with OpenJarvis remains open; the historical matched comparison is unchanged.
