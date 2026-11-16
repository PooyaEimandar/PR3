# Research evaluation

## Frozen matched research comparison (v1)

The [preregistered protocol](v1/protocol.md), [held-out corpus](v1/held-out.json) and [freeze manifest](v1/freeze.json) define 16 fresh scenarios, four with offline restart. The full matrix is complete: **4B and 27B, seeds 1/2/7, two arms, 192 scenario runs / 240 reviewed turns**. Production source hashes and corpus bytes were frozen before inference. The two smoke scenarios are separate and do not contribute to held-out scores. See the [completed comparison](evidence/2026-11-16-held-out-report.md), [machine-readable summary](evidence/2026-11-16-held-out-summary.json) and [live Tavily findings](evidence/2026-11-16-live-report.md).

Source-reviewed answer content passed 51/60 PR3 versus 41/60 upstream at 4B, and 53/60 versus 27/60 at 27B. This was implementation-assistant review, not independent human validation; the report preserves borderline judgments and separate automatic scores. Power changed from AC to battery, so timing comparisons are observational. **Live research acceptance failed:** Tavily search worked, but four of five diagnostic turns exhausted the context budget. Fix real-page budgeting and source reuse before expanding the workflow.

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

Regression coverage also exercises exact user-topic egress, candidate-only web reads, fabricated quotations, snippets, task isolation, source deletion, Stop/retry, bounded storage/output, malformed provider results and missing keys. Native UI automation covers Research → compare → restart → inspect → remove; real public-link reader smoke tests are separate. Live keyed Brave search and physical Android acceptance remain manual gates.

[Initial 4B development results, seeds 1/2/7](evidence/2026-11-14-development-4b-report.md): 15/18 cases, with all failures retained. [Quotation-fix evaluation and regression results](evidence/2026-11-15-quotation-4b-report.md) rerun that unchanged corpus and the new cases, preserving intermediate failures too.
