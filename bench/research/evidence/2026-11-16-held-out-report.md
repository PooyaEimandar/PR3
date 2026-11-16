# Frozen research comparison v1 — 2026-11-16

**Complete: all 192 runs / 240 turns, with every produced answer reviewed and no failed-case retries.** PR3 returned more supported, complete answer content in this shared-tool comparison. The 27B model added only two reviewed passes for PR3 over 4B. These results do not establish overall product superiority, and power changed during the run, so the timing comparison is observational.

The separate [live Tavily acceptance](2026-11-16-live-report.md) **failed general research acceptance**: search worked, but four of five diagnostic turns exceeded the context budget. One short-page answer was supported and inspectable. These five turns are diagnostics, not an accuracy estimate. The live failures take priority over adding more research features.

## Results

Each row covers 48 scenario runs and 60 turns: 16 scenarios across three seeds, including four resumed questions per seed. A scenario passes source review only if all its turns pass. Runtime errors and incomplete answers remain in the denominators. Review is by the implementation assistant, not an independent human panel.

| Model / arm | Reviewed answer content | Reviewed scenarios | Frozen automatic scenarios | Runtime exceptions | No substantive final answer |
|---|---:|---:|---:|---:|---:|
| 4B / PR3 | 51/60 (85.0%) | 39/48 | 33/48 | 0 | 1 |
| 4B / OpenJarvis | 41/60 (68.3%) | 32/48 | 5/48 | 10 | 14 |
| 27B / PR3 | 53/60 (88.3%) | 41/48 | 38/48 | 0 | 1 |
| 27B / OpenJarvis | 27/60 (45.0%) | 18/48 | 3/48 | 13 | 24 |

All 23 runtime exceptions were context-limit errors. These counts overlap with “no substantive final answer.” The upstream arm also flagged iteration exhaustion on 4 turns at 4B and 12 at 27B; that metadata can coexist with substantive but incomplete output, so it is not added to exception counts. PR3's two no-answer turns honestly reported incomplete research. There were no missing turns or RPC deadline failures.

Four review decisions have plausible alternate readings. Flipping the flagged judgments independently gives **52–54/60 for PR3 27B and 26–28/60 for OpenJarvis 27B**. These are sensitivity ranges, not statistical confidence intervals. The corpus contains only 16 scenario families; repeating seeds does not create 48 independent task families.

| Model / arm | Observed median turn | p95 turn | Slowest turn | Median among reviewed passing turns |
|---|---:|---:|---:|---:|
| 4B / PR3 | 15.8 s | 25.1 s | 27.4 s | 15.6 s |
| 4B / OpenJarvis | 29.2 s | 41.5 s | 47.1 s | 29.7 s |
| 27B / PR3 | 88.3 s | 166.7 s | 219.8 s | 84.7 s |
| 27B / OpenJarvis | 153.8 s | 246.6 s | 346.0 s | 149.7 s |

The full matrix took approximately **5 hours 3 minutes**, from 10:13:47 to 15:16:35 UTC on 2026-11-16. Median is the mean of the middle pair; p95 uses nearest rank. These timings include failed turns and exclude model loading. Power was not held constant; do not attribute the entire cross-model latency difference to model size.

| Model / arm | Reviewed passes, seeds 1 / 2 / 7 (each /20) | Median seconds, seeds 1 / 2 / 7 |
|---|---|---|
| 4B / PR3 | 17 / 18 / 16 | 13.3 / 15.9 / 16.7 |
| 4B / OpenJarvis | 15 / 11 / 15 | 28.5 / 29.5 / 29.0 |
| 27B / PR3 | 18 / 18 / 17 | 82.2 / 100.0 / 100.5 |
| 27B / OpenJarvis | 13 / 7 / 7 | 147.3 / 150.6 / 180.4 |

### What failed

PR3 4B's nine reviewed failures were three stale answers to the new source-inspection request after restart, three missing current-price conclusions, one unsupported random-assignment claim, one misleading quotation/finding association, and one honestly incomplete quota-fallback task. At 27B, source inspection improved to 3/3, but the model asserted an unverified current price in all three seeds, made one incorrect Persian conditional-price comparison, added an unsupported study-design assertion, inferred a borderline under-16 permit exemption, and failed to complete one quota-fallback task.

The upstream arm's failures included context exhaustion, exhausted iterations, an unfinished JSON answer, fabricated or denied saved-source quotations, false missing-evidence claims, and promotion of an undated price to a verified current price. Its 27B source-inspection follow-up failed in all three seeds. These are outcomes under the shared adapter and budgets, not a claim about every native OpenJarvis configuration.

All **564 quotations in successful report-tool calls** independently matched their saved source text and identity: 127 PR3/4B, 115 upstream/4B, 151 PR3/27B and 171 upstream/27B. This does not validate every final answer: the upstream loop can rewrite the tool result afterward, and either model can attach an unsupported inference to a valid quotation. Source-reviewed passing answers with a successful report call numbered 51, 34, 53 and 25 respectively; content review and tool-workflow compliance are deliberately separate.

## Scope and provenance

The [preregistered protocol](../v1/protocol.md) froze 16 self-authored synthetic scenario families, with four offline restart questions, before inference. Both 4B and 27B models run at seeds 1, 2 and 7 in PR3 and the actual pinned upstream OpenJarvis `OrchestratorAgent`: **192 scenario runs / 240 turns**. This is a public runtime holdout with intentional category overlap with development cases, not independent corpus authorship or proof of training-data independence. Its short sources do not represent the page sizes that failed in the live app.

Upstream commit: `b3b7f07efda4ea30fe61f25309d4af5b9dba5c34`. PR3 base commit: `ab39c750e2bbdbb3f947ee2885caec32dfedeba8`, with the benchmark harness additions and frozen production-source hashes recorded in the manifest. Model identity is defined by the GGUF hashes in that manifest; its filenames are `Qwen3.5-4B-Q4_K_M.gguf` and `Qwen3.8-27B-Q4_K_M.gguf`.

Both arms share the local llama.cpp engine, model bytes, source snapshots, fixture network and three read-only research tools. The context is 4,096 tokens with 512 reserved for each answer, temperature 0.7, top-p 0.8, top-k 20, presence penalty 1.5 and thinking disabled. Common tool limits are four rounds, four calls per round, twelve calls per turn, 4 KiB arguments and 8 KiB output. Five outer inference rounds permit a final answer after the tool rounds; additional internal generation passes are measured. Production prompts, approval checks and execution limits were not changed for this evaluation.

PR3 retains its shipped Host behavior. OpenJarvis uses its actual function-calling loop with an adapted research-role prompt and parallel tools disabled for the sequential bridge. This measures orchestration over shared retrieval and source-verification tools. It does **not** compare native OpenJarvis retrieval, its full application or connector ecosystem, or broad security. Different system prompts and task-ID metadata are expected; the prompts are not byte-identical.

Restart closes and reopens real SQLite storage and reconstructs the agent while model weights remain loaded. PR3 restores its native task. The OpenJarvis adapter restores the saved user/assistant transcript through `AgentContext`. This is recovery under the shared adapter, not proof of native OpenJarvis persistence, cold startup or phone/computer continuity. The fixture network is disabled on resumed turns. No live provider key, user document or Tavily request is part of the matrix.

## How to read the scores

Automatic checks and source review answer different questions. The frozen automatic checks require facts, selected reasoning expressions, authentic quotations in the expected source-label format, report-tool use and no runtime error. They are diagnostics, not a semantic judge. Alternate final citation layouts disproportionately fail in the upstream arm even when their content is correct. Several lexical checks also misclassify valid uncertainty wording or match a causal phrase inside a denial. Conversely, correct quotations and expected words can coexist with unsupported conclusions.

The frozen battery-life prohibition contains a literal `d+` where a numeric character class was intended. This weakens that automatic prohibition; source review checks for invented battery estimates directly. The corpus and oracles were left unchanged after discovery. These limitations preclude using the automatic pass rate as a headline accuracy comparison.

The implementation assistant reviewed each produced answer against its full source records and frozen rubric as scenarios completed. This review was incremental, not an independent post-run human assessment; arm identity could be inferred from presentation and run order. The original review packet and separate completed decisions are retained. Exact duplicate answers may reuse a documented decision only when the case, turn, error and source identities/text/location also match after canonicalizing numeric research task IDs. Corrections preserve their earlier decision and explanation.

A source-review pass requires supported, complete answer content responsive to the latest request and no runtime error. Missing requested information, fabricated provenance, misleading evidence associations and unsupported additions fail. Citation-format compliance and successful report-tool use are recorded separately; a content pass does not establish an inspectable final citation or completion of every tool-workflow requirement. Requested source-instruction inspection is distinct from obeying an instruction embedded in a document.

Borderline wording is explicitly flagged. In particular, ambiguous uses of “observational” in one answer from each arm are accepted as descriptions of a before/after association; definite invented assignment or exclusion of experimental intervention is not. A definite North under-16 permit exemption is treated more strictly because the record explicitly establishes only the positive requirement for people aged 16 or older. An upstream answer's denial of additional ticket “conditions” is also flagged because that word could mean purchase terms or the shared-session coverage actually present in the source. The sensitivity counts above show how alternate interpretations affect the result.

## Performance conditions

The run uses an Apple M2 Max with 96 GiB memory, Darwin 27.2.0 arm64. Models run sequentially, 4B before 27B, seeds 1/2/7; arm order alternates. The live test app was quit before inference. Model processes exit before the next scenario starts. Warm-turn timings exclude model loading; adapter wall time is preserved separately. All completed and failed turns remain in the performance sample, with quality-passing timing also reported separately.

Power was sampled as AC at the start (100%) and again during the final seed (60%, charging); the final check found **battery power at 28%, discharging**. The transition time is unknown. A process check during inference found only the benchmark model plus an idle Ollama service; `ollama ps` showed no loaded model. The final check confirmed the benchmark process and test app had exited. Power, other application load and thermal state were not continuously controlled. Per-seed timings are retained, so time/order variation is visible. A controlled latency comparison requires a repeat with power conditions held steady. These are observations from one desktop run, not Android latency, energy efficiency, a cold-start benchmark or a live-web service-level guarantee.

The bridge has a 300-second response deadline per RPC. PR3 submits a whole turn in one RPC; the upstream adapter submits individual generation RPCs, so this transport deadline is not a matched whole-turn timeout. **No deadline fired**, and no run was censored by it. Two upstream turns exceeded 300 seconds while still respecting the shared tool/iteration limits. Do not describe the two arms as having identical wall-clock limits.

## Implications for the next implementation

1. Fix live context budgeting and saved-source reuse first. Keep complete immutable source snapshots in storage, select bounded relevant excerpts using the actual tokenizer, and preserve provenance and the untrusted-source boundary. Add real-page-sized and resumed-task regressions.
2. Test conclusions as well as quotations: temporal validity, conditional prices, causal limitations and invented provenance. Correct source text alone does not validate the inference attached to it.
3. Honor explicit source restrictions such as official-only requests. Keep task status tied to runtime completion and show context exhaustion clearly.
4. Recheck the live Tavily workflow, then physical Android latency and continuity. Treat this exposed corpus as regression evidence for subsequent fixes and freeze a fresh holdout for a new quality claim. Native retrieval and the broader matched security matrix remain separate work.

## Validation and artifacts

The harness adds TypeScript orchestration, freezing, scoring and regression tests, a Rust bridge and a thin Python binding to invoke the upstream Python agent. The regression coverage includes corpus integrity, score replay, duplicate/foreign rows, errors in denominators, quotation authenticity and formatting, numeric facts outside metadata, reasoning versus quoted text, median calculation, and common tool/network limits. Earlier smoke attempts and their failures are retained separately from held-out scores.

Validation passed: workspace unit/integration tests **384 passed, 9 ignored**, sequential workspace doctests, strict workspace Clippy, formatting and TypeScript checks. The initial concurrent app build caused a doctest artifact lookup error; the sequential rerun passed, as recorded in the [handoff](2026-11-16-research-handoff.md). The four existing Stop tests and nine new TypeScript research tests passed; the nine research tests were rerun after inference. The two new bridge regressions are included in the workspace results.

Post-run verification found the production freeze unchanged and all **14 recorded harness/binary/model hashes matching**. Replaying raw results reproduced the runner's automatic summary exactly. All 240 review IDs and exact-equivalence links validated, and all 564 accepted report-tool quotations matched saved sources. The archive was extracted into a separate directory: all 19 checksummed files matched and its analysis replay reproduced the stored aggregates. No model weights, API key, personal chat trace or live page body is in this archive.

- [Machine-readable aggregate and review sensitivity](2026-11-16-held-out-summary.json)
- [Complete raw evidence and replay helpers](2026-11-16-held-out-v1.tar.gz), including generations, IPC, source snapshots, original and completed review packets, per-decision explanations, manifests and power observations
- [Archive extraction/replay verification](2026-11-16-held-out-archive-check.json)
- [Earlier smoke attempts](2026-11-16-research-smoke.tar.gz), excluded from held-out scores

Archive SHA-256: `f9f6ee8f39f93173545f015ab718b7a4b9f2a4d0e75f28af464e0f1545ba95ac`.

To replay without model inference, extract the archive into a **new** direct child directory of `bench/research/result/` using `tar --strip-components=1`, then run `node <that-directory>/analysis-replay.ts --partial` from the repository root. Its `REPLAY.md` documents automatic-score replay and the optional model/binary integrity check. Use the matching source revision and retained harness hashes. A future production change must not overwrite this frozen evidence.
