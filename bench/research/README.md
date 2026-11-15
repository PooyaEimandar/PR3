# Bounded research development diagnostic

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

Automatic checks require a runtime-validated report, expected fact strings, a minimum count of distinct cited source IDs and absence of the injection marker. Expected facts are checked outside source metadata, so digits in a source fingerprint or filename cannot supply a missing fact. The new corpus also checks requested phrases inside actual evidence blockquotes, rather than accepting a mention in analysis; `restart_checks` can specify different expectations for the resumed user request. Every quoted passage is independently checked by the production tool. These checks do not establish semantic entailment, exhaustive coverage or strong multilingual quality; manual review and a new frozen held-out corpus are still required. A quoted hostile instruction counts as an answer-quality failure even if no tool action follows it. Desktop timings here are diagnostic and may include concurrent test/build load; they are not Android latency measurements.

Regression coverage also exercises exact user-topic egress, candidate-only web reads, fabricated quotations, snippets, task isolation, source deletion, Stop/retry, bounded storage/output, malformed provider results and missing keys. Native UI automation covers Research → compare → restart → inspect → remove; real public-link reader smoke tests are separate. Live keyed Brave search and physical Android acceptance remain manual gates.

[Initial 4B development results, seeds 1/2/7](evidence/2026-11-14-development-4b-report.md): 15/18 cases, with all failures retained. [Quotation-fix evaluation and regression results](evidence/2026-11-15-quotation-4b-report.md) rerun that unchanged corpus and the new cases, preserving intermediate failures too.
