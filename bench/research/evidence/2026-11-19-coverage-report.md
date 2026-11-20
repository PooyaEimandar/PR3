# Long-page evidence and multi-part research answers — 19 November 2026

The runtime now selects relevant evidence across the entire bounded page instead of saving only its first twelve parts. It preserves supporting conditions, offers unread requested pages before cached rereads, and carries the selected evidence through offline corrections and restart. Approval checks and execution limits are unchanged. **This improves coverage; it does not guarantee that every model-written claim uses the right supporting quotation.**

## Implementation and regression coverage

- The reader still stores at most twelve parts per page. Selection uses lexical term coverage and supporting-passage ranking over the bounded response. Original part numbers, source identity, byte ranges and capture times remain intact. An explicit part uses the same allowance. Cached reads never silently refetch an omitted part; deletion still fails closed. Existing snapshots are not retroactively expanded.
- Inference preserves relevant evidence from distinct pages, complete conditional/causal passages and distinct requested properties before filling space with repeated or irrelevant text. Narrow corrections stop excluded subjects from dominating ranking. Recognized unrelated commands no longer crowd factual excerpts by repeating query terms: the existing report-quality guard filters the inference excerpts, while original snapshots, explicitly requested inspection and legitimate procedures remain available.
- The planner offers the current request's unread candidate references before a cached reread. Failed validation of `part: 0` does not count as a page read. Current web permission and the final reporting round remain authoritative. The host rejects reads outside the offered candidate set, charging the existing budget. Fresh local research is guided to an initial nonempty search. Successful retrieval transcripts are compacted only in the inference copy; actual history, errors, traces and counters are preserved.
- The report prompt asks for each comparison subject, additional restrictions and explicit uncertainty. Context fitting tries complete short passages from several sources before fragments. Lexical English-oriented ranking and fixed prompt cues are not general semantic or multilingual retrieval, a proof of entailment, or a new security boundary.

The unchanged limits are **four tool rounds, four calls per round and twelve calls per turn**, with a 4096-token context and 512-token output reservation. Existing page/task/storage/quotation limits also remain unchanged. No approval bypass or new execution permission is introduced; repair attempts consume the original turn budget.

Verification: **431 Rust tests passed, 10 ignored**, strict workspace Clippy passed, and **nine native macOS workflow tests passed with a scripted model**. The separately run real-tokenizer budget regression passed, as did TypeScript checking, four Stop tests, nine research harness tests and the memory Thumb build. Native workflow regressions include cancellation, explicit retry, encrypted recovery, source inspection after restart and deletion. The matrix below uses actual model inference. Earlier failed candidates and development verification attempts are retained separately. Physical Android remains user-run.

## Frozen evaluation and interpretation

The final v13 binary was frozen before the six-run evaluation: **4B and 27B, seeds 1/2/7**, 18 scenarios / 27 turns per run, **108 scenario runs / 162 turns**. All 16 cases from the 17 November reliability corpus are unchanged; two exposed synthetic long-page cases add four turns per run. Their relevant facts occur in original parts 19, 21 and 22, beyond the old first-twelve-part selection. Each includes an offline correction after restart. The public SQLite HTML is the same frozen baseline fixture, passed through the production parser.

This is development evidence, **not a new holdout, live provider test or matched OpenJarvis comparison**. The entire production Host is exercised with local model inference and offline page fixtures. No live Tavily key or private profile is used. The unchanged historical comparison remains separate. All final turns receive explicit unblinded implementation-assistant source review; it is not independent human validation. Material unsupported claims, missing facts and obsolete/unrequested findings fail; harmless redundancy and formatting do not. Borderline decisions remain visible.

Two earlier candidates were stopped and retained: candidate 1 exposed excerpt-ranking regressions involving source commands and correction wording; candidate 2 exposed a malformed one-based part request being counted as an attempted read. The final matrix uses one binary and corpus throughout, with no failed-case retries. Earlier outputs are never substituted into it.

Final binary SHA-256: `3ccea269456d3d4c6a1ee38013deca51ac2b5ff53ac94e432445fe0f4c335c6d`. Corpus SHA-256: `8bef4ce1cec707bb25dd87767571d3823e44414910d4f1b10f009376d915e0ee`. The archive manifest records the model hashes and 136 source-overlay hashes. Runtime task IDs and capture times vary, so fixed seeds do not promise identical output.

## Results

**All six runs and all 162 source reviews are complete.** No final-run case was retried or replaced.

| Final development build | Validated reports | Basic diagnostics | Source-reviewed answers | Terminal errors | Median / p95 turn time |
|---|---:|---:|---:|---:|---:|
| Qwen3.5 4B Q4_K_M | 81/81 | 79/81 | **74/81** | 0 | 8.4 / 22.5 s |
| Qwen3.8 27B Q4_K_M | 81/81 | 81/81 | **79/81** | 0 | 56.7 / 118.1 s |

The archive replay verified **270 displayed quotations**, all **136 final source-overlay hashes** and complete review coverage. Model and executable hashes were also rechecked before packaging; their files are not redistributed. All **54 restart turns** retained identical source snapshots, including capture times, and used no SEARCH/READ. Recorded tool use stayed within four rounds and at most five total calls; the configured four-round/four-calls-per-round/twelve-call limits remain unchanged. Four intermediate 4B report attempts were rejected and recovered within the original turn budget: three lacked a required citation and one exceeded four findings. There were no terminal context, planning or citation-integrity errors.

**Long-page evidence capture passed in all 24 new-case snapshots:** every requested late fact was saved, including on turns whose answer failed semantic review. Original parts 19, 21 and 22 were retained within the same twelve-part allowance. This separates the fixed first-twelve-parts coverage problem from the remaining model errors in choosing quotations and interpreting evidence.

| Source-reviewed comparison | 4B | 27B |
|---|---:|---:|
| Same 16 baseline cases, 17 November build | 58/69 | 65/69 |
| Same 16 baseline cases, this build | **64/69** | **67/69** |
| New long-page cases and offline corrections | **10/12** | **12/12** |
| Public SQLite comparison, initial turn | **2/3**, previously 0/3 | **3/3**, previously 0/3 |
| Public SQLite offline correction | 3/3 | 3/3 |
| Explicit current-price uncertainty | 2/3, previously 0/3 | 3/3, previously 2/3 |

The baseline comparison uses the unchanged questions and source bytes from the [17 November replay](2026-11-17-reliability-report.md). It is an exposed development comparison across builds, not a controlled ablation, independent review or estimate of general accuracy. The intervening unified-chat integration is also part of the current build. Improvements are not uniform: the 27B documentation corrections introduce borderline unsupported license wording that the earlier build did not receive failures for.

The **seven 4B failures** are retained individually in the source review: one unsupported attribution of a host trust label to the source; one overconfident current-price answer; one SQLite answer with a host claim attached to a performance quotation and an unsupported extra detail; two export-conditions answers that attach the wrong quotation (one also falsely claims the quota evidence is missing); one correction that revives an obsolete comparison in its gaps; and one group-size condition misinterpreted as an age condition. The new late-page facts were present in the saved snapshots on both export failures.

The **two 27B failures** are conservative borderline judgments: documentation corrections describe an unspecified “main license” despite the evidence establishing public-domain engine status and separate documentation licensing. This applies the previous report's license-characterization criterion consistently. A reviewer treating those phrases and the 4B host-label attribution as harmless wording could score **75/81 at 4B and 81/81 at 27B**. One passing 4B instruction-inspection answer also has borderline attribution wording and is explicitly marked; a stricter judgment there would lower 4B by one. These are disclosed review sensitivities, not statistical uncertainty intervals.

Presentation issues remain, including placeholder gaps text, an internal reference in prose, an obsolete “research dialog” hint and English prose for some Spanish questions. They are recorded separately from factual failures. The explicitly requested Persian comparison passes all six model/seed runs; this small exposed set does not establish broad multilingual quality.

The machine was an **Apple M2 Max with 96 GiB memory**. Power samples at the start and after every model/seed run showed AC power and 100% charge. A mid-run process inventory found only the intended benchmark runner; an idle Ollama daemon had no model runner. Timing includes within-turn failed attempts and uses Cargo's **development profile**, with offline fixtures and no provider latency. Model loading is excluded, and restart turns reopen storage/new Host while the engine remains loaded; these are not cold-app-start measurements. Background load, thermal conditions and energy were not controlled. The timings do not establish release-build, live-search, Android or current OpenJarvis performance.

The acceptance decision is **bounded evidence capture and runtime integrity verified; answer-quality acceptance still incomplete**. The real-page comparison improved substantially, but 4B claim-to-quotation matching, source attribution and faithful corrections still need work. The archive was extracted into a fresh directory and replayed successfully; all ten positive/negative verification controls passed.

## Inspectable evidence and remaining acceptance

- [Raw evidence archive](2026-11-19-coverage.tar.gz): final matrix, unchanged baseline corpus, all-turn source review, both stopped candidate matrices, exploratory outputs, verification logs and source overlays.
- [Recomputed summary](2026-11-19-coverage-summary.json) and [archive verification](2026-11-19-coverage-archive-check.json).
- Extract and run `node replay.ts` and `node check-replay.ts` with Node 24 or later. These verify the raw artifacts without model weights, including quotation equality, corpus/source hashes, restart snapshots and budgets. The README describes inference reproduction.

The next quality gate is claim-to-quotation support and current-validity reasoning, followed by live acceptance, Android measurements and an independently reviewed fresh real-page holdout. Broader ingestion, source sync and the remaining [OpenJarvis capabilities](../../../doc/openjarvis-parity.md) remain separate work. No fresh security matrix or live/Android performance claim is made here.
