# Research evaluation

## Latest report-contract and evidence-selection evaluation

The [21 November report-contract evaluation](evidence/2026-11-21-research-contract-report.md) completes **4B and 27B at seeds 1/2/7**, with all **240 answers reviewed**: **232 main passes, 212 strict and 233 lenient**. Development scores are 79/81 and 80/81; the exposed regression set scores 37/39 and 36/39 (4B/27B). Strict scores are 75/81 and 78/81 on development, 31/39 and 28/39 on exposed cases. All eight previously failing configurations now pass the main review, with two retaining strict-review wording concerns. Eight different failures remain, including omitted facts, unsupported comparisons and Cargo rules, and an incomplete biography report. The preceding [repair evaluation](evidence/2026-11-21-evidence-repair-report.md) scored 232 main and 218 strict: these results do not establish an overall answer-quality improvement.

Generation now constrains research report fields and citations to the fitted evidence, requires supported conclusions for selected inference questions, preserves relevant long-page conditions and adds narrow identity/date/owner-approval checks. Quotation/source/task/deletion validation, approvals and execution limits remain intact. Validation passed **481 workspace tests plus three native tokenizer/grammar tests**, strict Clippy, formatting and the build. The [raw archive](evidence/2026-11-21-research-contract.tar.gz), [summary](evidence/2026-11-21-research-contract-summary.json) and [extracted replay verification](evidence/2026-11-21-research-contract-archive-check.json) retain every failure, borderline judgment and superseded attempt; all 39 verifier controls pass. All 84 restart snapshots are unchanged, all requested restart evidence remains fitted and no restart retrieves sources. Restart answer quality is **83/84 main, 80 strict, 84 lenient**; the remaining main failure adds an unsupported Cargo rule to a correct quotation.

This is exposed, unblinded implementation-assistant regression review, not a fresh holdout, independent validation or a new matched OpenJarvis comparison. The final run changed from AC to battery and back, with no sampling gap over 60 seconds; background compiler/load contention and generated-answer differences also confound speed comparisons. The current build still needs live Tavily, native UI and user-run Android acceptance. The last documented Android installation is the earlier online-search repair build. Historical native-workflow passes do not certify this build.

## Frozen matched research comparison (v1)

The [preregistered protocol](v1/protocol.md), [held-out corpus](v1/held-out.json) and [freeze manifest](v1/freeze.json) define 16 fresh scenarios, four with offline restart. The full matrix is complete: **4B and 27B, seeds 1/2/7, two arms, 192 scenario runs / 240 reviewed turns**. Production source hashes and corpus bytes were frozen before inference. The two smoke scenarios are separate and do not contribute to held-out scores. See the [completed comparison](evidence/2026-11-16-held-out-report.md), [machine-readable summary](evidence/2026-11-16-held-out-summary.json) and [live Tavily findings](evidence/2026-11-16-live-report.md).

Source-reviewed answer content passed 51/60 PR3 versus 41/60 upstream at 4B, and 53/60 versus 27/60 at 27B. This was implementation-assistant review, not independent human validation; the report preserves borderline judgments and separate automatic scores. Power changed from AC to battery, so timing comparisons are observational. **The November 16 live acceptance failed:** Tavily search worked, but four of five diagnostic turns exhausted the context budget. The [November 17 reliability report](evidence/2026-11-17-reliability-report.md) records subsequent context, citation, scope and planning changes, with answer-quality failures kept separate from runtime checks. These historical matched scores do not certify the newer build.

The [November 19 coverage report](evidence/2026-11-19-coverage-report.md) records the complete 4B/27B seeds 1/2/7 development rerun: **162 turns**, 74/81 and 79/81 source-reviewed answers. All 16 previous cases are unchanged; two exposed long-page cases add four turns per run. All requested late facts were saved in all 24 new-case snapshots, and all 54 restart turns preserved their sources without SEARCH/READ. Claim-to-quotation and wording failures remain. The archive includes raw traces, all-turn review, earlier failed candidates, source overlays and a verifier with negative controls. This is not a new held-out or matched upstream comparison.

The [November 20 quotation and interpretation report](evidence/2026-11-20-support-report.md) repeats all 18 scenarios unchanged at both models and seeds 1/2/7: **76/81 at 4B and 81/81 at 27B**, with all 162 turns reviewed. Five 4B failures remain, including one incomplete report after four rejected wrapper-attribution attempts. Borderline wording is retained explicitly: stricter treatment of the accepted borderline answers gives 74/81 and 79/81. All 245 displayed quotations verify against source bytes; all 54 restart snapshots and 24 long-page fact checks pass. Recorded median/p95 turn times are 12.2/29.6 seconds and 62.1/151.1 seconds, with concurrent background compilation observed; these do not establish a controlled speed improvement or Android latency. The [verified archive](evidence/2026-11-20-support-archive-check.json) preserves the final matrix, three separate interrupted candidates, source overlays, regression logs and ten replay controls. Live Tavily acceptance and independent review remain open.

The subsequent [online-search repair](evidence/2026-11-20-online-repair-report.md) addresses a user-reported biography failure: read pages before citing them, reserve a bounded repair round, reject namesake evidence and omit heading/profile-only labels from simple full-name answers. It retains all nine live turns, including five earlier answer-quality failures, alongside 446 passing Rust tests and nine passing native workflows. The final 4B macOS checks passed the exact online request (13.43 s) and offline continuation after restart (9.97 s), with no final rejected calls or restart retrieval. That build still showed a cosmetic gaps placeholder, addressed by the later [quality change](evidence/2026-11-20-quality-report.md). The matching APK is installed on Android; user-run device and broader live acceptance remain open. These single-task times are not controlled benchmarks. The preceding full matrix is historical evidence and was not rerun for this follow-up.

The [raw evidence archive](evidence/2026-11-16-held-out-v1.tar.gz) includes complete model/tool IPC, snapshots, both review packets, manifests and replay helpers. Its [extraction and aggregate replay check](evidence/2026-11-16-held-out-archive-check.json) passed. Credentials and personal app traces are excluded.

This invokes the actual upstream OpenJarvis `OrchestratorAgent` at `b3b7f07efda4ea30fe61f25309d4af5b9dba5c34`, with PR3's read-only research tools and local llama.cpp engine shared across both arms. It measures orchestration, source-backed answers and recovery under that adapter. It does not establish native retrieval parity or overall product superiority. TypeScript owns orchestration, freezing, scoring and tests; the small Python binding invokes the upstream Python agent without reimplementing its loop.

```sh
# From the repository root, with the existing local GGUF files in model/.
npm --prefix app/pr3-app ci
npm --prefix app/pr3-app run test:research
cargo build -p pr3-bench-documents --bin research_bridge --locked
git clone https://github.com/open-jarvis/OpenJarvis.git /tmp/pr3-research-openjarvis
git -C /tmp/pr3-research-openjarvis checkout b3b7f07efda4ea30fe61f25309d4af5b9dba5c34
uv venv --python 3.13 /tmp/pr3-research-python
uv pip install --python /tmp/pr3-research-python/bin/python -r bench/documents/requirements.txt
mkdir -p bench/research/result
node bench/research/run.ts --upstream /tmp/pr3-research-openjarvis \
  --python /tmp/pr3-research-python/bin/python \
  --out bench/research/result/smoke --split smoke
# Stop other inference workloads, including the app, before timing the matrix.
node bench/research/run.ts --upstream /tmp/pr3-research-openjarvis \
  --python /tmp/pr3-research-python/bin/python \
  --out bench/research/result/held-out --split held-out
node bench/research/score.ts bench/research/result/held-out
```

Use a new output directory for each attempt; preserve failures. The runner refuses changed frozen production sources or a dirty/different upstream revision. It records model, binary and harness hashes, hardware, sampled power state, dependencies, generations, tool events, source snapshots and timings. The scorer verifies corpus bytes, rejects duplicate/foreign rows, and retains error and missing turns in denominators. Some equivalent Markdown wrappers are accepted, but alternate citation layouts, shortened/foreign source IDs and changed quotations do not pass. The completed report documents lexical false positives/negatives and one weak numeric prohibition in the frozen corpus. Automatic string/regex checks are only diagnostics: retain the original `semantic-review.json` and save completed source reviews separately before making synthesis-quality claims.

Model loading is outside warm-turn timings. Adapter wall time is recorded separately, and model processes exit before the next run starts. No credentials or live provider requests are used by this comparison. Live Tavily checks ran separately in the app, with the key entered by the user. Desktop timings are not Android latency evidence. This corpus has now been exposed to implementation review: preserve it as a regression baseline and freeze a new version for subsequent held-out claims rather than overwriting v1.

## Development diagnostics

`chat-regressions.json` exercises the unified chat entry point with the production Host and local model: natural document comparison, model-selected research without an explicit research command, and a user-provided public-link fixture. Two scenarios reopen SQLite and reuse saved evidence with Web off. The optional `chat` object sends structured composer controls instead of an implicit `Research:` prefix; ordinary clock/calculator/memory tools are available before research begins. This is exposed development coverage, not a held-out or matched comparison. Review the saved answers against their source snapshots in addition to the automatic quotation checks. The [three-seed 4B check](evidence/2026-11-17-unified-chat.md) records 15/15 diagnostic passes and the remaining presentation limitations.

```sh
cargo build -p pr3-bench-documents --bin research --locked
# Use a fresh output path for each seed, 1/2/7.
target/debug/research model/Qwen3.5-4B-Q4_K_M.gguf 1 \
  bench/research/chat-regressions.json /tmp/research-chat-4b-seed1.jsonl
```

`run-reliability.ts` freezes and runs the context, quotation, source-scope/conditional-reasoning, public-page and long-page coverage development replays together, using 4B and 27B at seeds 1/2/7. It records source, corpus, model and executable hashes and preserves every failed turn. Public SQLite HTML is downloaded once during preparation; all model turns then use offline page fixtures with the production parser. An optional third argument reuses an exact earlier `corpus.json`, preserving its cases and page bytes, and appends the new coverage cases. This does not replay Tavily's search-result inventory or provider timing. Run it with a copied executable and a new directory:

```sh
cargo build -p pr3-bench-documents --bin research --locked
cp target/debug/research /tmp/pr3-research-frozen
node bench/research/run-reliability.ts /tmp/pr3-research-frozen \
  bench/research/result/reliability-new \
  bench/research/result/reliability-2026-11-17-final/corpus.json
node bench/research/review-reliability.ts bench/research/result/reliability-new
```

`coverage-regressions.ts` adds two synthetic long-page scenarios with four turns: a two-page comparison plus a restriction, and eligibility plus a quota. Required facts occur beyond part 12. Both reopen SQLite with network access disabled and correct the request to quote one saved detail. The review packet now includes every turn, including turns without a specialized rubric. These are exposed development regressions, not a fresh holdout; review every requested fact, each attached quotation, corrections and uncertainty separately from citation-integrity checks.

To repeat an existing complete corpus byte for byte, supply its `corpus.json` and append `--replay-only`. This avoids appending the coverage cases again. Keep each candidate in its own directory, freeze the executable and source overlay before inference, and complete all six runs before reporting aggregate results. Failed or interrupted candidates remain separate from the final matrix.

`reliability-regressions.ts` replays four exposed v1 reasoning cases and adds website restriction/restart and ambiguous official-source clarification cases. These are development evidence, not a new holdout. The diagnostic runner's `pass` is intentionally narrow; review the saved `evidence.sources` against each case's `review` rubric before claiming conclusion quality. Public live-page acceptance and Android measurements remain separate.

This exercises the production PR3 Host, research tools and SQLite with real local-model inference. Six **development** cases cover two-source prices, conflicting dates with restart, missing evidence, Spanish documents, source-embedded hostile instructions, and a local/web comparison with offline restart. The web case uses a synthetic fetcher and no search provider/key; it is not live internet evidence.

`quotation-regressions.json` adds six development cases for irrelevant instructions before/after a fact, an instruction in the same sentence, Spanish source text, web snapshots reused offline, an explicit request to quote an instruction after restart, and relevant operating instructions from a manual. These cases test both omission of unrelated text and preservation of evidence the user actually requested.

The cases were used while developing the feature. They are **not held-out**, not a release/security benchmark, and not an OpenJarvis comparison. The earlier matched document benchmark remains unchanged under `bench/documents/evidence`.

```sh
cargo build -p pr3-bench-documents --bin research --locked
# Output must not already exist. Repeat with seeds 2 and 7.
target/debug/research model/Qwen3.5-4B-Q4_K_M.gguf 1 \
  bench/research/development.json /tmp/research-4b-seed1.jsonl
target/debug/research model/Qwen3.5-4B-Q4_K_M.gguf 1 \
  bench/research/quotation-regressions.json /tmp/research-quotations-4b-seed1.jsonl
```

Each JSONL row preserves final answers, errors, tool events, full core traces and elapsed time. Restart cases reopen the actual SQLite database and construct a new Host; the model engine stays loaded, so this measures host/storage recovery rather than cold model startup. The network fixture refuses new fetches on the resumed turn. No real API credential or private document is used. Existing runtime budgets and authorization remain active.

Automatic checks require a runtime-validated report, expected fact strings, a minimum count of distinct cited source IDs and absence of the injection marker. Expected facts are checked outside source metadata, so digits in a source fingerprint or filename cannot supply a missing fact. The new corpus also checks requested phrases inside actual evidence blockquotes, rather than accepting a mention in analysis; `restart_checks` can specify different expectations for the resumed user request. Every quoted passage is independently checked by the production tool. These development checks do not establish semantic entailment, exhaustive coverage or strong multilingual quality; the completed frozen comparison above adds source review, while independent review and a larger real-page holdout remain necessary for broader claims. An irrelevant quoted hostile instruction counts as an answer-quality failure even if no tool action follows it. Desktop timings here are diagnostic and may include concurrent test/build load; they are not Android latency measurements.

Regression coverage also exercises exact user-topic egress, candidate-only web reads, fabricated quotations, snippets, task isolation, source deletion, Stop/retry, bounded storage/output, malformed provider results and missing keys. Native UI automation covers ordinary chat → natural research in the same task → ordinary chat → restart → inspect → remove, along with inline source validation and New task. Real public-link reader smoke tests are separate. Live keyed search and physical Android acceptance remain manual gates.

[Initial 4B development results, seeds 1/2/7](evidence/2026-11-14-development-4b-report.md): 15/18 cases, with all failures retained. [Quotation-fix evaluation and regression results](evidence/2026-11-15-quotation-4b-report.md) rerun that unchanged corpus and the new cases, preserving intermediate failures too.
