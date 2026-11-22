# Delivery plan: milestones, epics, issues

Nine milestones, M0–M8, in the order of the [roadmap](16-roadmap.md). Each epic is an issue-tracker epic; each issue has an ID, a deliverable and the check that closes it. Issues inherit their milestone. The [delivery checklist](18-delivery-checklist.md) puts the issues into one ordered list. Exit criteria are measurable, so a milestone is closed by evidence, not by feel.

Four gates run through the plan: golden vectors replayed byte for byte ([D-15](19-decisions-and-risks.md)), named tests in the [test ledger](20-test-ledger.md), module budgets from M1 and MLPerf LoadGen for inference ([part 15](15-benchmarks.md)), and accessibility as its own issues ([R-06](19-decisions-and-risks.md)).

| Milestone | Goal | Exit criteria |
| --- | --- | --- |
| M0 Specification and foundations | The specification, its oracles, and the ground every module stands on | The specification reviewed; the test ledger and the golden vectors checked in; the base library passing its tests under `asan`, `ubsan` and `release` and its CBMC harnesses; the same test runner passing on the Mac and the physical Android phone; the MLPerf harness producing `VALID` SingleStream and Offline runs; llama.cpp's baselines captured on both devices; the benchmark manifest hashed |
| M1 Portable core and first chat on macOS and Android | The core, trusted side and inference host in C, fast, on the owner's devices | Every golden vector replayed with identical bytes; every M1 ledger row passing; an offline chat with the pinned Qwen3.5-2B on the llama.cpp backend that streams, cancels and accepts the next request, on both devices; under LoadGen, PR3 on the llama.cpp backend shows no measurable overhead over llama.cpp alone (the 95% interval of the paired difference includes zero or favours PR3); BEN-03: each of `core`, `json`, `trusted`, `cbor` and the text pipeline within its budget |
| M2 SDL3 chat app on macOS and Android | The chat in a native app with no WebView | The chat screen on both devices with streaming, stop and retry; Persian and English mixed text shaped, ordered and edited correctly; VoiceOver and TalkBack operate every control; the app targets of [part 15](15-benchmarks.md#app-targets) met (idle CPU, memory, cold start, size, frame time) |
| M3 The full product | Everything PR3 promises, on its own data and with its peers | Every M3 ledger row passing; databases, receipts and audit logs written by an earlier build opened, verified and continued; two PR3 devices paired in both roles (Mac node with phone client, and the other way round); the task, documents, research and AgentDojo suites passing through PR3's harnesses |
| M4 Embedded clients on a real node | A Pico and an ESP32-C3 asking the owner's node; a Pi 5 as a node | Both boards complete, cancel and recover requests against the PR3 node; the Pico within 32 KiB incremental RAM and 96 KiB incremental flash; the Pi 5 answers from a document offline with a verified quotation |
| M5 The app on five operating systems | One app on macOS, iOS, Android, Windows and Linux | The app ledger passing on each OS, with its platform services, key custody and screen reader; an install upgraded in place from the previous build; five installable packages |
| M6 The PR3 engine | Standalone C inference | Prefill, decode and checkpoint fixtures passing on the Mac, the phone and the Pi 5 with no llama.cpp or ggml linked; SIMD and Metal paths matching the scalar oracle; every shipping model configuration qualified; the engine at or above the llama.cpp backend under LoadGen (SingleStream and Offline) on each named device, or the gap recorded and llama.cpp kept for that profile |
| M7 Release candidates and qualification | Candidates that can be installed, recovered and trusted | Sanitizers, fuzzing and failure injection clean; the trusted side's proofs passing; every mandatory ledger row with evidence; installable candidates with recovery drills; a second builder reproduces the board images from the port guide |
| M8 Published benchmarks and 1.0 | Public numbers, and a ship or hold per profile | MLPerf Inference runs (Closed and Open, SingleStream and Offline) and the end-to-end dimensions, reproducible with raw samples and a replay command, labelled as [part 15](15-benchmarks.md#publishing-and-labelling) requires; a second builder reproduces the summaries; a ship or hold decision per board and OS |

## Long-lead items

- [ ] The development Mac and the Android phone identified by model, OS and build, and kept for every M0–M2 measurement (FND-02, BEN-02).
- [ ] The pinned model files downloaded and their hashes checked against `config/pr3.toml` (INF-03); Llama 3.1 8B Instruct for MLPerf Closed runs, under its licence, which the owner accepts (BEN-02).
- [ ] A decision on MLCommons membership, which verified results require ([D-26](19-decisions-and-risks.md)).
- [ ] A physical iPhone (APP-05); Windows and Linux machines with their screen readers (APP-07).
- [ ] A Raspberry Pi 5 8 GB, an original Raspberry Pi Pico, an ESP32-C3-DevKitM-1, a USB serial adapter (M4).
- [ ] A power meter for the energy rows, or a decision to report energy only where the OS exposes it (BEN-05).
- [ ] Apple and Google developer accounts for signing; GitHub repository settings for CI (FND-01, APP-09).

## M0 Specification and foundations

*Epic SPEC: The specification and its oracles*

- [x] **SPEC-01** The architecture decision: this specification, with the profile matrix ([part 02](02-architecture.md#profiles)), the dependency list and licences ([part 12](12-tech-stack.md)). Check: the owner's review. Written 21 Nov 2026; waiting for the review.
- [ ] **SPEC-02** The test ledger: every promised behaviour as a named test with an owner issue and a profile ([part 20](20-test-ledger.md)). Check: no row without an owner; `tools/check-ledger.sh` finds every test function in the ledger and every ledger row in the tests.
  - 22 Nov 2026: the base library's 40 tests and canonical JSON's 23 are named rows, all passing; `tools/check-ledger.sh` reports 63 test functions and 0 problems. The other areas' rows are named as their issues start.
- [ ] **SPEC-03** Golden vectors: `tools/gen-vectors/` writes JSON Lines from the specification, independently of the C code, for canonical JSON, core step sequences, argument validation and canonicalization, trusted hashes, receipts and audit entries, CBOR frames, prompt renders and the Unicode tables; each file names its format version and the command that made it; cross-checked against published vectors where they exist (RFC 8949's CBOR examples, NIST's SHA-256 and HMAC vectors, the Noise and BLAKE2 test vectors). Check: regenerating changes nothing; every file reviewed.
  - 22 Nov 2026: `tools/gen-vectors/` writes `base/siphash.jsonl` (128 vectors, self-checked against the SipHash paper's example) and canonical JSON's `canonical.jsonl`, `numbers.jsonl` and `invalid.jsonl` (441, 1431 and 83 vectors) from Python's standard library under PR3's rules; `--check` regenerates nothing new. Waiting for the owner's review of each file; the other formats come with their issues.

*Epic FND: Foundations*

- [ ] **FND-01** Build system and gates: the root `CMakeLists.txt`, the presets of [part 14](14-build-and-quality.md), the warning set as errors, sanitizer and hardening modules, `pr3_add_module`, `pr3/poison.h`, `tools/check-banned.sh`, `check-layers.sh`, `check-traps.sh`, `.clang-format`, `.clang-tidy`, `ci.yml`. Check: `dev`, `asan`, `ubsan` and `release` on the Mac; CI on a pull request the owner pushes.
  - Native run on the Mac passed 22 Nov 2026: all of the above exist; `dev`, `asan`, `ubsan`, `tsan` and `release` build with no warnings and pass every test (66 with the benchmark and MLPerf smoke tests); `clang-tidy` is clean on the sources; every check script passes and catches a planted violation. Not yet run: CI, which waits for the owner's push.
- [ ] **FND-02** One test runner on both devices: `test/pr3_test.h`; the runner built for macOS and for Android arm64 with the NDK, pushed and run over `adb` with its exit code and log collected; GCC, Clang and MSVC compile checks; Arm and RV32 cross-builds; a freestanding link of the portable libraries with its list of external symbols checked. Check: a native run on the Mac; the physical Android phone.
  - 22 Nov 2026: the runner passes on the Mac and, through `test/device/android.sh`, on the Android emulator (`sdk_gphone64_arm64`, Android 16, API 36, NDK r27c): `test_base` 40 of 40 and `test_json` 23 of 23. `android-arm64`, `armv6m` and `rv32imc` cross-build on the Mac; the two freestanding images link with their symbol lists checked. Blocked: the physical phone (not attached); the GCC and MSVC compile checks run only in CI, which waits for the owner's push.
- [x] **FND-03** The base library: status codes, arenas with ASan poisoning, generational pools, the buddy heap, SPSC rings, slices and builders, UTF-8, checked arithmetic, SipHash, each with unit tests, and CBMC harnesses for the arena, pool, buddy heap, ring, builder and UTF-8 decoder ([part 03](03-memory-safety.md)). Check: `asan`, `ubsan`, `release`; CBMC.
  - 21 Nov 2026: the library and its 40 tests pass under `asan`, `ubsan` (with `integer`, `bounds` and `nullability` on PR3's targets), `tsan` and `release`. Passed 22 Nov 2026 under CBMC 6.11 (`tools/run-cbmc.sh`): the arena (212 checks), pool (543), buddy heap (654), ring (345, in its fenced-word mode, since CBMC does not model Clang's C11 atomic builtins), builder (719) and UTF-8 decoder (725) verify with no failure. The buddy harness found three narrowing conversions in `pr3_store_le32`, which now masks before it narrows.
- [ ] **FND-04** Ownership and the board contract: the API conventions in headers (opaque handles, pointer and length, status codes, caller-owned memory, an explicit step API; lifetime, cancellation and reentrancy stated per function); `pr3_board.h` and its callbacks ([part 11](11-embedded-profiles.md)); a freestanding demo that links without threads, files, heap or network, where every allocation failure returns a status. Check: the freestanding link's symbol list in CI.
  - 22 Nov 2026: `include/pr3/board.h`, `board/freestanding/pr3_board.h`, the demo and a second link of the json module; `ctest --preset armv6m` and `rv32imc` check both images' symbol lists on the Mac (text bytes for Cortex-M0+ and RV32IMC: the demo 1523 and 1711, the json image 15 762 and 18 288). Not yet run in CI.

*Epic BEN: Measurement*

- [x] **BEN-01** The MLPerf harness: `tools/fetch-third-party.sh` and `third_party/manifest.txt`; LoadGen built from the pinned sources by our CMake; the C ABI shim, the `pr3_mlperf` command and the loopback SUT in `bench/mlperf/` ([part 15](15-benchmarks.md#the-harness)); SingleStream and Offline smoke runs reaching `VALID`. Check: a native run on the Mac. Done 21 Nov 2026, started early at the owner's request: `ctest --preset release`, `asan` and `tsan` (8 repeats) pass both smoke runs; an Offline accuracy run writes 64 entries for 64 samples.
- [ ] **BEN-02** Baselines: the `llamacpp` SUT; llama.cpp's SingleStream and Offline runs for the pinned Qwen3.5-2B (Open) and Llama 3.1 8B Instruct (Closed, with an accuracy run) on the Mac and the Android phone; `bench/c` writing the sample format of [part 15](15-benchmarks.md#evidence); the manifest hashed before any module is tuned; a held-out set reserved and scored separately. Check: a native run on the Mac; the physical Android phone; the manifest archived.
  - 22 Nov 2026: `bench/c/` writes the sample format (`pr3_bench.h`; one JSON line per run), on the Mac and the Android emulator. Blocked: the pinned models, the Llama 3.1 licence and the physical phone (long-lead items).

## M1 Portable core and first chat on macOS and Android

*Epic CORE: The portable core*

- [x] **CORE-01** JSON: the bounded tape parser, the schema-subset validator's primitives and the canonical writer to the canonical JSON specification ([part 04](04-core-runtime.md#tool-registry-and-the-schema-subset), [D-08](19-decisions-and-risks.md)); a fuzz target; CBMC on the number path. Check: the JSON vectors; fuzzing for one hour; its budget row.
  - Passed 22 Nov 2026 (native run on the Mac): the 1955 vectors and 23 tests pass under `dev`, `asan`, `ubsan`, `tsan` and `release`, and on the Android 16 emulator; `fuzz_json` ran one hour with the Mac kept awake, 50 549 773 inputs, 801 edges, no failure; CBMC 6.11 verifies the number path's 1012 checks for every input of up to six bytes that does not need the exact decimal ([part 14](14-build-and-quality.md#model-checking)); the `json` row's first measurement is in [part 15](15-benchmarks.md#results-so-far). The phone's measurement goes with BEN-03.
- [x] **CORE-02** The agent runtime: `pr3_core` with the turn, the tool registry and schemas, the loop guard, history and its trimming, and traces ([part 04](04-core-runtime.md)); the 50 runtime tests of the [test ledger](20-test-ledger.md#agent-runtime-core-02); the core vectors replayed, including stale events, capacity exhaustion and cancellation; the fuzz target `fuzz_core_step`. Check: `asan`, `ubsan`, `release`; vectors; its budget rows.
- [ ] **CORE-03** The trusted side: intent, the invisible-character tables, policy, gate, receipt, audit and hash ([part 05](05-trusted-side.md)); the 21 trusted tests of the ledger; hashes, receipts and audit entries equal to the golden vectors. Check: vectors; `asan`, `ubsan`; its budget rows.
- [ ] **CORE-04** CBOR and the wire v0 messages: the bounded canonical codec ([D-09](19-decisions-and-risks.md)), the 1 MiB ceiling separate from each endpoint's own cap, version refusal; the protocol tests; golden frames decoded on 32- and 64-bit builds. Check: vectors; fuzzing; its budget row.

*Epic INF: The inference host*

- [ ] **INF-01** Prompt formats and texts: the Qwen3 and Qwen3.5 templates, every policy text in `src/infer/prompt_text.h` with its id, budget fitting with the piece-token cache ([part 06](06-inference.md)); the 22 prompt tests; every prompt vector rendered and compared. Check: vectors.
- [ ] **INF-02** The streaming text pipeline and tool-call parsers: the single-pass state machine for visible text, think blocks and tool calls; the Hermes JSON and XML parsers; their tests in the ledger; the recorded token-stream corpora replayed. Check: vectors; fuzzing; its budget row.
- [ ] **INF-03** The llama.cpp backend: `llama.h` called directly, the sampling chain, KV reuse, checkpoints in a pool of three state buffers, read-ahead and cancellation; the KV-plan, session and backend tests, and the installed-model tests (`kv_reuse`, `qwen_tools`, `answer_injection`) with the pinned model. Check: a native run on the Mac with the pinned model.
- [ ] **INF-04** The first chat on both devices: `pr3 chat`, offline, streams, cancels mid-stream and completes a fresh request; the device record (device and OS, build and model hashes, fixture results, peak memory, time to first visible token, time to acknowledge a cancel) archived for each. Check: a native run on the Mac; the physical Android phone.

*Epic BEN: Measurement*

- [ ] **BEN-03** The first module budgets and the host's overhead: `core`, `json`, `trusted`, `cbor` and the text pipeline on the Mac and the phone, each budget set and met ([part 15](15-benchmarks.md#module-budgets)); the `pr3-llamacpp` SUT against llama.cpp alone under LoadGen. Check: results archived under `bench/results/`.

## M2 SDL3 chat app on macOS and Android

*Epic APP: The app*

- [ ] **APP-01** The app shell: SDL3 main callbacks with `waitevent`, the renderer, the lifecycle events, the data locations, the host thread and its rings ([part 10](10-app-sdl3.md)). Check: a native run on the Mac and the physical phone; idle CPU 0% with the window open; the app targets settled.
- [ ] **APP-02** `pr3_ui` and the text stack: layout, SDL3_ttf with HarfBuzz, SheenBidi, IME composition, selection, clipboard, keyboard shortcuts and touch; mixed Persian and English. Check: the rendering fixtures; the physical devices.
- [ ] **APP-03** The chat screen on the host commands: streaming, stop, retry, stop before start; the chat host commands with their tests. Check: the physical devices; the app rows of [part 15](15-benchmarks.md#app-targets).
- [ ] **APP-04** Accessibility on macOS and Android: the accessibility tree from `pr3_ui`, the NSAccessibility and `AccessibilityNodeProvider` bridges; VoiceOver and TalkBack read, focus and activate every chat control ([D-11](19-decisions-and-risks.md)). Check: the physical devices with their screen readers.
- [ ] **APP-05** The chat on a physical iPhone: the UIKit accessibility bridge, the keyboard, safe areas. Check: a physical iPhone.

## M3 The full product

*Epic DATA: The product on its own data*

- [ ] **DATA-01** Approval history across builds: receipts signed by an earlier build verify, audit logs resume, decision vectors pass including denial, interruption and restart; model output, source text and peer replies keep proposal-only authority. Check: vectors; a run across two builds on the Mac.
- [ ] **DATA-02** Encrypted stores: the SQLCipher C API with the schemas, the 11 migrations, recovery codes, key derivation and FTS5 ([part 07](07-memory-and-storage.md)); databases written by earlier builds and on other OSes; wrong keys, interrupted migration, low disk and restore. Check: the fixtures; the store tests.
- [ ] **DATA-03** Memory and task lifecycle: the memory-model tests; capture, correction, confirmation, forgetting, task selection, new topic, follow-up, stop, retry, export, retention and recovery, with persisted events compared across a restart. Check: the fixtures.
- [ ] **DATA-04** Document workflows: import, search, quote with an exact range, reopen, remove with dependent tasks; the document corpus replayed. Check: the fixtures; the physical devices.
- [ ] **DATA-05** Research workflows: web opt-in and scope, libcurl and lexbor within their bounds, saved evidence, prompt fitting, the report grammar, citation checks, correction, restart, deletion; frozen pages replayed; a consented live run recorded separately ([part 08](08-tools-and-research.md)). Check: the research suite.
- [ ] **DATA-06** Tools and their contracts: calculator, clock, documents, memory, web and research, with the tool tests; malformed output, permission denial and grounded responses replayed. Check: the fixtures.
- [ ] **DATA-07** Settings, models and observability: configuration precedence and the TOML subset reader, the model registry with hash checks, download, cancel and delete, trace opt-in, rotation and export, backup exclusions. Check: the fixtures; no private text in default telemetry.
- [ ] **DATA-08** Pairing, node and client: Noise, SPAKE2, revocation, routing and local fallback ([part 09](09-protocol-and-hybrid.md)); a Mac node with a phone client and a phone paired to a Pi or Mac node; streaming, cancel, disconnect and the sensitive-memory fixtures; the hybrid tests. Check: the physical Mac and phone, both roles.
- [ ] **DATA-09** Private continuity: sharing, moving, ownership epochs, cursors, tombstones, interruption and a replacement device, between two PR3 devices both ways. Check: the fixtures; two devices.
- [ ] **DATA-10** Every harness: the task, personal, documents, research, AgentDojo, fuzz and native-app suites run through PR3's harnesses; consequential actions only through the sandbox connector under `PR3_BENCH_MAIL`, never in a release build. Check: the ledger.

## M4 Embedded clients on a real node

*Epic EMB: Boards*

- [ ] **EMB-01** The accessory protocol: COBS framing, CRC-32C, the 256-byte payload cap, credits, timeouts and reset ([part 09](09-protocol-and-hybrid.md#accessory-protocol-serial)); host loopback tests that split frames, fill queues, cancel and reconnect. Check: CI; fuzzing.
- [ ] **EMB-02** The Pico firmware: local status offline; a fixture answer streamed and cancelled over UART; a power cycle mid-answer and a fresh request; firmware, link map, stack watermark and budgets archived. Check: a physical Pico.
- [ ] **EMB-03** The serial gateway: `pr3 gateway` bridges `ask` to a `generate` on the owner's node, within the board's cap, with cancellation; an oversized final message and an unreachable node become `failed`. Check: the Pico with the gateway on the Mac or Pi 5.
- [ ] **EMB-04** Both embedded paths: Pico, gateway, node; ESP32-C3 directly to the node over Wi-Fi; stream, cancel, reconnect and unpair, against the PR3 node. Check: the physical boards.
- [ ] **EMB-05** Failure and resource limits: power cuts, Wi-Fi drops mid-answer, exhausted queues, repeated connect and cancel; watchdog, stack, arena peaks and recovery time recorded; zero duplicated effects. Check: the physical boards.
- [ ] **EMB-06** The board port kit: `board/README.md`, the manifest, a sample loop, cross-build instructions and a hardware test script; both demos reproduced from clean checkouts; the procedure for a reduced profile on a smaller board; the budgets frozen for each configuration. Check: a clean checkout.
- [ ] **EMB-07** The Pi 5 node: networking off, the pinned model on the llama.cpp backend, an answer from an imported document with a verified quotation, the task reopened and deleted; a paired phone served. Check: a physical Pi 5.

## M5 The app on five operating systems

*Epic APP: The app*

- [ ] **APP-06** Every screen and host command: tasks, documents, sources, memory, research, approvals, settings, devices and models; all 62 host commands with their tests; the native workflow suite; keyboard-only and touch. Check: the physical devices.
- [ ] **APP-07** Windows and Linux: UI Automation and AT-SPI bridges, IME, high-DPI. Check: physical machines with Narrator or NVDA and Orca.
- [ ] **APP-08** Installed app data: the bundle identifier, storage locations, vault access and paired peers kept across an upgrade; an install with a document, a memory, an active task and a paired node upgraded in place; an interrupted upgrade; rollback qualified on copies. Check: packaged apps on physical devices.
- [ ] **APP-09** Five packages: macOS, Windows, Linux, iOS and Android, with suspend, resume, termination, model-load failure and shutdown. Check: installed packages.

*Epic PLT: Platform services*

- [ ] **PLT-01** Speech in and out, language choice, biometrics, file selection, sharing, export and permissions; allow, deny, cancel and unavailable on each OS. Check: the physical devices.
- [ ] **PLT-02** Key custody: the Keychain, the Android Keystore wrapping, DPAPI on Windows and the Secret Service on Linux, failing closed ([part 07](07-memory-and-storage.md#keys-and-encryption)). Check: the physical devices; installs from the previous build open.

## M6 The PR3 engine

*Epic ENG: The PR3 engine*

- [ ] **ENG-01** GGUF and the tokenizer: the bounded reader, every offset and size checked; token IDs and detokenized text for English, Persian, Unicode and control markers equal the pinned tokenizer's; truncated files and low memory handled. Check: fixtures.
- [ ] **ENG-02** Scalar kernels for the pinned files' actual inventory of types and operations, each against recorded llama.cpp outputs. Check: per-operation vectors.
- [ ] **ENG-03** Qwen3.5 execution: full attention, the recurrent blocks, the feed-forward layers and the output projection; intermediate outputs and next-token scores in prefill and decode; every scratch buffer in the plan. Check: vectors.
- [ ] **ENG-04** Decoding on the engine: sampling, grammar, stopping and UTF-8 streaming through INF-02's pipeline. Check: the INF-02 corpora on the engine.
- [ ] **ENG-05** Model state: prefix reuse, the KV cache, recurrent checkpoints and read-ahead; a restored continuation equals a cold replay; a cancelled or divergent conversation never reuses the wrong state. Check: vectors.
- [ ] **ENG-06** The headless engine on the Mac, the Android phone and the Pi 5: prefill, decode, restore, cancel and a fresh request; peak memory, load time and decode rate; no llama.cpp or ggml in the binary. Check: the physical devices.
- [ ] **ENG-07** SIMD: NEON, AVX2 and AVX-512 paths with the scalar ones as the oracle; throughput, memory, threads and sustained thermals. Check: the physical devices.
- [ ] **ENG-08** GPU: Metal on Apple devices against llama.cpp's Metal path on the same hardware; Vulkan on a named Android or Linux device. Check: the physical devices.
- [ ] **ENG-09** Every shipping model configuration: Qwen3 with JSON tools, the Qwen3.5 family with XML tools, every size and quantization, reasoning modes and device fallback; 4B and 27B on hardware with the memory; the `pr3-engine` SUT under LoadGen. Check: the qualification records.
- [ ] **ENG-10** Per-profile dependencies: `tools/check-profile.sh` on every artifact's link map; the board images without weights, UI or SQLCipher; release builds without benchmark-only code (`bench/`, `PR3_BENCH_MAIL`). Check: CI.

## M7 Release candidates and qualification

*Epic REL: Release*

- [ ] **REL-01** Memory safety and failure paths: every suite under the sanitizers, every fuzz corpus, injected allocation, short-I/O, flash-write and power failures; the CBMC harnesses and Frama-C WP proofs for the trusted side's validator, policy and audit chain; stack and watchdog on the boards. Check: CI and nightly; the physical boards.
- [ ] **REL-02** The test ledger closed: every applicable suite on every profile; every missing behaviour an open issue that blocks its profile. Check: the ledger.
- [ ] **REL-03** Packages and recovery: SDK, firmware, headless and graphical artifacts with checksums, SBOMs, licences and signatures; install, update, interrupted update and recovery on each claimed device; the Pico's reflash path. Check: the physical devices.
- [ ] **REL-04** A clean-room build and a soak: a second builder reproduces the board images from the port guide; one more board from another vendor's SDK qualified or left pending; resource growth over sustained runs. Check: the second builder's record.

## M8 Published benchmarks and 1.0

*Epic BEN: Measurement*

- [ ] **BEN-04** Freeze the final artifacts and instruments: the candidates frozen, LoadGen's version recorded, every hash verified, the collectors calibrated. Check: the benchmark manifest.
- [ ] **BEN-05** Memory, footprint and energy ([part 15](15-benchmarks.md#end-to-end-dimensions)). Check: the archived samples.
- [ ] **BEN-06** MLPerf Inference and whole-task performance: Closed and Open, SingleStream and Offline, performance and accuracy, for llama.cpp alone, PR3 on the llama.cpp backend and the PR3 engine on each profile's devices ([part 15](15-benchmarks.md#mlperf-inference)). Check: the archived runs.
- [ ] **BEN-07** Quality and reliability, with fresh held-out answers judged blind. Check: the archived samples.
- [ ] **BEN-08** Publish the results under MLCommons' naming and labelling rules ([part 15](15-benchmarks.md#publishing-and-labelling)); a second builder reproduces the summaries. Check: the report.
- [ ] **BEN-09** 1.0 by profile: ship or hold per board and OS, with rollback and every remaining gap linked to an issue. Check: the owner's decision.

---

[Back to the index](../README.md)
