# Benchmarks

How PR3's speed, size and quality are measured: against other projects with a public, audited benchmark, and against PR3's own budgets on every change. Inference is compared in public with **MLPerf Inference** through MLCommons LoadGen. Everything else (the core, the parsers, the trusted side, the stores, the app) is held to **module budgets** from the first milestone, so no module closes slower or larger than its budget and no change makes it slower by more than 5%.

## MLPerf Inference

MLPerf Inference is the industry's peer-reviewed benchmark for inference systems, run by MLCommons. Its load generator, **LoadGen**, is the same C++ library every submitter links: it issues queries in a fixed pattern, times every response and writes the logs a result is judged on. It measures any system under test (SUT) that implements its two interfaces, so PR3's engines plug in directly, with no vendor runtime in between ([D-25](19-decisions-and-risks.md)).

| What | Value |
| --- | --- |
| LoadGen | 6.0.17, `mlcommons/inference` at commit `3fbc329939999c13d0a7b5e67fb2092287e06047`, pinned in `third_party/manifest.txt` |
| Category | Edge |
| Benchmark | `llama3_1-8b-edge`: summarization of CNN/DailyMail 3.0.0 articles, at most 128 new tokens; token latencies on (`use_token_latencies`), so LoadGen also records time to first token and time per output token |
| Scenarios | **SingleStream**: one query at a time, the next issued when the last completes; metric: 90th-percentile latency; at least 600 s. **Offline**: every sample issued at once; metric: tokens per second |
| Settings | The benchmark's settings come from LoadGen's embedded `mlperf.conf` and are never changed for a reported run; `bench/mlperf/conf/` holds the `user.conf` files with the per-system values the rules allow (target latency, expected throughput) and the smoke settings |
| Modes | Performance (timing) and accuracy (every output logged to `mlperf_log_accuracy.json` and scored with MLCommons' evaluation script) |

**Divisions.** A **Closed** run uses Llama 3.1 8B Instruct and must reach the benchmark's accuracy target (ROUGE scores at least 99% of those of the unquantized model, as MLCommons sets them), so it compares systems on the same model. An **Open** run may use any model, such as PR3's pinned Qwen3.5-2B, and is reported as Open with the model named.

**Subjects.** On one device, one model file and one scenario at a time:

| Subject | SUT in `pr3_mlperf` | What the comparison shows |
| --- | --- | --- |
| llama.cpp alone | `llamacpp`: `llama.h` called directly by the harness, no PR3 code | The baseline |
| PR3 on the llama.cpp backend | `pr3-llamacpp`: the full PR3 inference host ([part 06](06-inference.md)) | PR3's own overhead over llama.cpp: prompts, the text pipeline, tool-call parsing, KV reuse |
| The PR3 engine | `pr3-engine` | The engine against llama.cpp, with the host held constant |
| Loopback | `loopback`: answers each query with a fixed token sequence after a set delay | The harness itself; no model needed |

Published results from other systems in the same category, scenario and division are cited beside PR3's numbers as context, with their MLPerf result IDs.

### The harness

`bench/mlperf/` builds LoadGen from the fetched sources with our CMake (LoadGen's own CMake writes into its source tree), and keeps C++ inside that directory ([D-22](19-decisions-and-risks.md)):

| File | Purpose |
| --- | --- |
| `pr3_loadgen.h`, `pr3_loadgen.cc` | A C ABI over LoadGen: a SUT and a query sample library (QSL) as C callbacks, `pr3_lg_complete`, `pr3_lg_first_token`, `pr3_lg_run` |
| `pr3_mlperf.c` | The `pr3_mlperf` command: picks the SUT, scenario, mode and configuration, runs LoadGen and writes its logs |
| `sut_loopback.h`, `sut_loopback.c` | The loopback SUT, with a worker thread that answers queries in order |
| `conf/smoke.conf` | Short runs (a few seconds) that still produce a `VALID` result, for CI and for checking the harness; with `--performance 256`, because LoadGen rounds SingleStream query counts up to a multiple of the QSL's performance sample count |
| `tsan.supp` | One ThreadSanitizer suppression for a race inside LoadGen's logger; LoadGen is not patched, because every run logs its git status and file hashes and a patched copy would show there |

```bash
tools/fetch-third-party.sh
cmake --preset release && cmake --build --preset release --target pr3_mlperf
build/release/bench/mlperf/pr3_mlperf --sut loopback --scenario SingleStream --performance 256 \
  --user-conf bench/mlperf/conf/smoke.conf --out bench/runs/smoke-singlestream
```

Each run directory keeps LoadGen's files unchanged (`mlperf_log_summary.txt`, `mlperf_log_detail.txt`, `mlperf_log_accuracy.json`) beside PR3's `manifest.json`.

### Publishing and labelling

Only MLCommons members and test partners can submit results for review and have them **verified**. Until the owner decides on membership ([D-26](19-decisions-and-risks.md)), every number PR3 publishes from these runs:

- carries the words "Result not verified by MLCommons Association." next to it;
- writes MLPerf with the registered mark the first time it appears (MLPerf®) and names the division, scenario, benchmark and LoadGen version;
- never compares itself to a verified result without saying which one is which.

## Agent security

AgentDojo measures what a prompt injection in a tool result can make an agent do. PR3 runs its suite through `bench/agentdojo/` with the benchmark build's sandbox mail connector (`PR3_BENCH_MAIL`, never in a release, [part 02](02-architecture.md#a-consequential-action)): utility without attacks, utility under attack, and the attack success rate, with every consequential call checked against the gate's audit log. The target is zero unauthorized effects.

## Module budgets

Each row is a budget for the module that owns it. The budget is set from the module's first measurement on the development Mac and the Android phone ([BEN-03](17-delivery-plan.md)) and written beside the row; the module closes only when it meets it. From then on a change that makes a row's median more than 5% slower, or its peak memory larger, blocks the merge unless the reason is explained and accepted ([part 14](14-build-and-quality.md#gates)).

| Workload | Module | Measures | First measurement |
| --- | --- | --- | --- |
| Plain turn: utterance → infer → reply → trace | `core` | Turns per second, allocations per turn, peak memory | Mac, 22 Nov 2026: 527 ns a turn (1.9 million a second), no allocation, 2.1 KiB of heap and 3.3 KiB of step arena at most ([below](#results-so-far)). Phone: not yet |
| Tool turn: two rounds, two calls each, schema validation and canonicalization | `core` | Turns per second, allocations per turn | Mac, 22 Nov 2026: 3.76 µs a turn (266 000 a second), no allocation. Phone: not yet |
| A tool turn on a full history (32 messages), with 64 B and 8 KiB tool results | `core` | Time per turn; must not grow with the history's bytes | Mac, 22 Nov 2026: 385 ns and 655 ns; the difference is the turn copying its own new result, not the history. Phone: not yet |
| Validate and canonicalize arguments (small, nested, number-heavy, string-escape-heavy) | `json` | MB/s, ns per call | Mac, 22 Nov 2026: parse 894, 739, 120 and 331 MB/s; canonicalize 359, 190, 88 and 230 MB/s ([below](#results-so-far)). Phone: not yet |
| Gate cycle: submit, approve, issue, redeem, executed, with audit entries | `trusted` | Cycles per second (signature verification reported separately) | Not yet |
| Audit verify of a 10 000-entry log | `trusted` | Entries per second | Not yet |
| CBOR round trip of `generate` and `done` messages | `cbor`, `proto` | MB/s | Not yet |
| Text pipeline over a recorded token stream with think blocks and tool calls | `infer` | Bytes per second, allocations per token | Not yet |
| Prompt render and tokenize, Qwen3 and Qwen3.5, with budget fitting | `infer` | Time to a fitted prompt | Not yet |
| Retrieval terms and scoring over the document fixtures | `memory` | Passages per second | Not yet |
| Page extraction over the frozen research pages | `tool` | MB/s, peak memory | Not yet |
| Store operations: remember, search, correct, forget, task save, document import and search | `store` | Operations per second (PR3's use of SQLCipher) | Not yet |

`bench/c/` runs these workloads from the same files on every device, warms up the same way and writes one sample format. A runner per module (`bench_<module>`, built from `pr3_bench.h`) reads its workloads from `bench/c/workloads/`, which `tools/gen-workloads.py` writes deterministically; it warms each workload up for 50 ms, sizes a run to last at least `--min-ms` (100 by default), and times `--runs` runs (30 by default), printing the median with the minimum, maximum and interquartile range. With `--out FILE` it appends one JSON line per run: `subject`, `module`, `workload`, `repetition`, `seed`, `device` (the model and chip on Apple, the maker, model and SoC on Android), `model`, `configuration` (build type and compiler), `cache`, `outcome`, `elapsed_ns`, `calls`, `bytes_per_call`, `ns_per_call`, `mb_per_s` and `collector`. `test/device/android.sh` runs the same runner on a phone with `PR3_BENCH_ARGS`.

## End-to-end dimensions

| Dimension | Workloads and reporting |
| --- | --- |
| Memory | Process start, model load, first response, the worst supported context, cache restore, repeated turns and idle after a cancel: peak resident memory, physical footprint where available, PR3 arena high-water marks, mapped model bytes, GPU allocation; RSS and PSS on Linux; shared and unified pages identified so nothing is counted twice; growth after repeated turns |
| Compute and latency | Cold and warm start, prompt tokens per second, generated tokens per second, time to first visible token, p50/p95 task latency, time to acknowledge a cancel, UI frame time; a short turn, a long prefill, a cached follow-up, grounded research with repairs, an interrupted workflow; tokens, tool calls and retries per sample |
| Footprint and energy | Executable or firmware, installed bundle, model download and on-disk data, separately; for boards, whole-image flash, static RAM, peak stack, arena use, headroom, bytes per request, watchdog resets; joules per attempted and per successful task, idle power, sustained thermal throttling where measurable; unavailable instruments marked as such |
| Quality and reliability | Pass counts and denominators by suite, seed, model and device: citation validity, completeness, task success, security and approval invariants, crashes, timeouts, retries, disconnect and data recovery, deletion; failures included in totals; latency to a correct answer beside success rate |

## App targets

Packaged apps on the development Mac and the Android phone, without a model loaded unless stated. These are proposals, settled by APP-01's first measurement:

| Measure | Target |
| --- | --- |
| Idle CPU with the window open | 0% |
| Resident memory without a model | At most 60 MiB on macOS, 80 MiB on Android |
| Cold start to an interactive window | At most 250 ms on the Mac, 600 ms on the phone |
| Installed size without models | At most 25 MB |
| UI frame time while tokens stream | p95 within one frame at the display's refresh rate |

## Controls

The same physical unit, OS or firmware, model hash and quantization, context length, prompt tokens for engine microbenchmarks, token budget, sampling and seeds, CPU thread count, offload settings, and power and thermal conditions (on mains, the same ambient temperature, the machine otherwise idle). The same user input, fixtures, tool and network budgets for product tests; generated token counts and fitted prompts recorded where they differ. Subjects alternate in blocks. Cold process, cold model and page cache, and warm prefix reuse are separated, with which states each OS lets the harness enforce. Research timing uses offline snapshots; live network observations are published separately.

## Sampling and decisions

Per device and subject: at least 10 independent cold starts and 50 warm repetitions per timed scenario, and a 30-minute sustained run; MLPerf runs use LoadGen's own minimums. Report the median, the p95 of the warm samples, paired differences with 95% intervals, sample counts and raw attempts. Microbenchmarks report the median of at least 30 runs of at least 100 ms each, with the spread. Deterministic invariants require exact results: zero unauthorized effects, and every fixture recovered. A faster answer with a failed citation is a failure. When the uncertainty crosses an agreed margin the sample grows, or the result is recorded as inconclusive. The full matrix is published, including the rows where PR3 loses.

## Evidence

`bench/results/<date>-<scope>/` holds `manifest.json` (PR3 and third-party revisions, build flags, model and fixture hashes), `devices.json`, the test ledger's state, `samples.jsonl`, `report.md` and the replay command; MLPerf runs add LoadGen's logs unchanged. Each sample names the subject, workload, repetition and seed, device, model, configuration, cache state, outcome, elapsed time, units and collector. Each budget row shows the measured value, the budget, the change since the last archived run and its uncertainty. Energy per success counts the energy of failed attempts. Raw meter traces and board link maps are kept beside the rows. No private conversation, user document or real credential enters the archive. Local runs go to `bench/runs/`, which git ignores.

## Results so far

None archived yet. The MLPerf harness ([BEN-01](17-delivery-plan.md)) passes its smoke runs on the development Mac (21 Nov 2026): the loopback SUT reaches `VALID` in SingleStream (1536 queries in 5.0 s) and Offline (1280 samples in 4.2 s) under the `release`, `asan` and `tsan` presets, and an accuracy run writes one log entry per sample. These check the harness, not a system, and are never reported. llama.cpp's baselines on both devices are BEN-02; the first module budgets (`core`, `json`, `trusted`) are BEN-03, at the end of M1.

The `json` row's first measurement ([CORE-01](17-delivery-plan.md), 22 Nov 2026) is a local run in `bench/runs/2026-11-22-json/`, not an archived result: `bench_json --runs 30` on the development Mac (Mac14,6, Apple M2 Max, macOS on Darwin 27.2), the `release` preset with Apple Clang 21, warm cache, 30 runs of at least 100 ms each, with other applications open (load average about 3). Medians, with every interquartile range at most 6.5% of its median:

| Workload | Bytes | Parse | Canonicalize |
| --- | --- | --- | --- |
| small | 79 | 88 ns, 894 MB/s | 220 ns, 359 MB/s |
| nested | 11 069 | 15.0 µs, 739 MB/s | 58.2 µs, 190 MB/s |
| numbers | 28 564 | 238 µs, 120 MB/s | 326 µs, 88 MB/s |
| escapes | 19 105 | 57.7 µs, 331 MB/s | 83.0 µs, 230 MB/s |

Canonicalizing includes parsing. Validation against a schema is not in these numbers: the validator's primitives are CORE-01's, the schemas they check arrive with the tool registry (CORE-02), and the row's workload gains that step then. The budget is set from this measurement and the phone's in BEN-03.

The `core` rows' first measurement ([CORE-02](17-delivery-plan.md), 22 Nov 2026) is a local run in `bench/runs/2026-11-22-core/`, on the same Mac, preset and compiler: `bench_core --runs 30`, 30 runs of at least 100 ms each, load average about 3.4. Each turn runs from the user's message to the reply on the default limits, with the model's answers and the tools' results fed in from memory, so the numbers are the core's own cost. Medians, with every interquartile range at most 2.4% of its median:

| Workload | Per turn | Turns a second | Heap high water | Step arena high water | Turn high water |
| --- | --- | --- | --- | --- | --- |
| Plain turn | 527 ns | 1.90 million | 2 112 B | 3 336 B | 34 B |
| Tool turn: two rounds, two calls each | 3.76 µs | 266 000 | 3 648 B | 10 536 B | 796 B |
| Full history, 64 B tool results | 385 ns | 2.60 million | 2 624 B | 5 784 B | 156 B |
| Full history, 8 KiB tool results | 655 ns | 1.53 million | 67 648 B | 5 784 B | 8 284 B |

The plain and tool turns end by writing their trace as canonical JSON, as a host that stores it does; the full-history turns, one call each on a history kept at 32 messages, do not, which is why they cost less than a plain turn. The two full-history rows show the infer step does not copy the history: their requests show the same 32 messages, and the 269 ns between them is the turn copying its own new 8 KiB result into the history heap (about 30 GB/s). The core allocates nothing after `pr3_core_init`; at the default limits its block is 10 534 935 bytes, of which the 8 MiB heap is the largest part, a turn slot 383 464 bytes and the step arena 177 369 bytes. The budget is set from this measurement and the phone's in BEN-03.

---

[Back to the index](../README.md)
