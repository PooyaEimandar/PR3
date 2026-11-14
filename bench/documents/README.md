# Matched local-document comparison

This is the first document-focused PR3/OpenJarvis comparison. It runs PR3's actual `Host`/`LlamaBackend` and upstream OpenJarvis's actual `NativeReActAgent`, pinned to [`76836f4`](https://github.com/open-jarvis/OpenJarvis/commit/76836f4eba5e0d25cf53b56c2f475a2c2583c1c9). OpenJarvis is a benchmark dependency only.

Both agents receive the same user requests, imported synthetic documents, `search_documents` and `answer_documents` tools, model bytes, llama.cpp binding, sampling configuration and context budget. The stdio adapter delegates OpenJarvis inference and tool execution to Rust; it does not implement or replace OpenJarvis's agent loop. No HTTP inference server or production Python dependency is added. The two arms keep their own prompts/planning/output policies: PR3 uses native structured calls and runtime quotation enforcement; OpenJarvis uses its upstream ReAct protocol and final-answer behavior.

This isolates **agent orchestration over shared retrieval**, not OpenJarvis's native memory/RAG implementation, full product capability, energy use, mobile efficiency or the complete security matrix. It is a small development diagnostic, **not a held-out benchmark or overall ranking**.

## Frozen inputs and scoring

`corpus.json` has 12 scenarios: lookup, correction, comparison, Arabic/Persian retrieval, missing evidence and hostile source/filename text. Its SHA-256 is fixed in `corpus.sha256`. Seeds 1, 2 and 7, two agents: **72 runs**. No model-prompt or runtime tuning during evaluation. Each case starts with a fresh model context and fresh synthetic library; arm order alternates. Model loading is outside measured workflow time.

Common engine configuration: 4,096-token context, 512 new tokens per engine pass, temperature 0.7, top-p 0.8, top-k 20, presence penalty 1.5, thinking off, Metal layers 999. Each arm gets at most five agent inference rounds, four tool rounds, 12 tool calls, 4 KiB arguments and 8 KiB tool outputs. PR3's existing protected answer pass is part of its engine path and consumes additional generation work; token totals include it. Budgets are shared ceilings, not a claim that both algorithms consume identical numbers of engine passes. Neither arm can execute an external action in this experiment.

Report **factual correctness** separately from **verified-quotation workflow completion**. The latter requires the requested verification tool to succeed, relevant exact quotation(s) and genuine source IDs to appear in the final answer, no prohibited answer content and no runtime error. An OpenJarvis answer can be factually correct and still fail that explicit workflow contract; this is not automatically an incorrect citation or an attack success. Missing-evidence cases require a successful search and abstention.

`score_v2.py` corrects a v1 grading false positive: an actual filename containing an attack marker can legitimately appear in source attribution. Quoting that complete filename is exempt; emitting the standalone marker still fails. The corpus, raw outputs and v1 scores are preserved. Version 2 is separately recorded and regression-tested. No model rerun or product tuning is used for this scorer correction.

## Reproduce

Prepare the pinned public OpenJarvis checkout and a Python 3.13 environment, then install `requirements.txt` into that environment. Only agent-path dependencies are installed; optional cloud/search/retrieval packages are not used. Do not run the comparison concurrently with another model workload.

```sh
cargo build -p pr3-bench-documents --locked
python -m unittest discover -s bench/documents -p 'test_*.py'
python bench/documents/run.py \
  --upstream /path/to/pinned/OpenJarvis \
  --model model/Qwen3.5-4B-Q4_K_M.gguf \
  --out bench/documents/result/new-run
python bench/documents/score_v2.py bench/documents/result/new-run
```

The runner refuses a modified upstream checkout or changed corpus, requires a new output folder, records source/model/binary hashes and sampling limits, and writes every result immediately. Timeout/runtime failures are not counted as successes. There is no automatic retry of a failed scenario. Exact versions are in `requirements.txt`; use the evidence bundle's manifest for a historical run. Apple Silicon is the measured target; this adapter's GPU-layer setting is not an Android configuration.

[The completed 72-run report](evidence/2026-11-14-4b-report.md) records PR3 36/36 and OpenJarvis 21/36 on the explicit verified-quotation workflow, with factual checks 36/36 and 32/36 respectively. It separates those measurements from planned native-retrieval, 27B, physical-device and full-security comparisons. Historical Personal-200/email data remains under its original directories and must not be relabeled as document results.

The final-runtime replay uses recorded model outputs and the real document tools, without loading a model:

```sh
cargo run -p pr3-bench-documents --bin replay --locked -- \
  bench/documents/corpus.json /path/to/evidence/results.jsonl
```
