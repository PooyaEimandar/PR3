# Delivery checklist

The [delivery plan](17-delivery-plan.md) as one ordered list of steps. Work top to bottom, one step at a time. Each step names the issues it closes and the [kind of check](14-build-and-quality.md#kinds-of-check) that ticks it. The plan stays the record of each issue's status and evidence; this list is the order.

**Next step (22 Nov 2026):** step 1.3 (CORE-03), the trusted side. Steps 0.2a, 0.2b, 0.3a, 0.4a, 0.6a, 1.1 and 1.2 passed on the development Mac; 0.2c, 0.3b, 0.4b and 0.6b wait on the owner, as each says. Step 0.1 still waits for the owner's review; 0.5 was done early at the owner's request.

## How to work through it

1. Take the first unticked step. Start a later step early only if it is a long-lead item, belongs to M6 after M1 has passed, or the owner reprioritizes it.
2. Do the work, then run the step's check.
3. Tick the step only after its check passes. In the same change: tick the issues in the delivery plan with the date and the evidence or command; move **Next step** above and the immediate objective in [the roadmap](16-roadmap.md); update the [test ledger](20-test-ledger.md) rows the step closes; add an entry to [the changelog](CHANGELOG.md), newest first; update [decisions and risks](19-decisions-and-risks.md) when the step settles or changes a decision.
4. Say which kind of check passed: CI, a native run on the Mac, an emulator or simulator, a physical device, or a packaged app on a physical device. A lower kind never ticks a step that asks for a higher one.
5. A blocked step stays unticked with the blocker and its date under it. Never record a test or a measurement that did not happen.
6. When only part of a step is done, split it so the rest stays visible.
7. A module step is not done until its budget row is recorded, even when it misses; a miss is recorded with its reason and an issue ([part 15](15-benchmarks.md#module-budgets)).
8. A number published outside this repository follows [part 15](15-benchmarks.md#publishing-and-labelling)'s labelling rules.

## M0 Specification and foundations

- [ ] **0.1** SPEC-01: this specification. Check: the owner's review. Written 21 Nov 2026.
- [x] **0.2a** FND-01, FND-03 on the Mac: CMake and presets, the warning set, sanitizers and hardening, `pr3/poison.h` and the check scripts; the base library with unit tests. Check: a native run on the Mac under `dev`, `asan`, `ubsan` and `release`. Passed 22 Nov 2026 (native run on the Mac), with `tsan` too.
- [x] **0.2b** FND-03 under CBMC: the harnesses for the arena, pool, buddy heap, ring, builder and UTF-8 decoder, and CORE-01's number path. Check: CBMC. Passed 22 Nov 2026 (CBMC 6.11 on the Mac): all seven verify, with the scope [part 14](14-build-and-quality.md#model-checking) states.
- [ ] **0.2c** FND-01, FND-03 in CI: `ci.yml` on a pull request. Check: CI.
  - Blocked 22 Nov 2026: waits for the owner's push.
- [x] **0.3a** SPEC-02, SPEC-03 for the base library and canonical JSON: their ledger rows named with owners; `tools/gen-vectors/` writes their vectors from the specification. Check: regenerating changes nothing; `tools/check-ledger.sh`. Passed 22 Nov 2026 (native run on the Mac).
- [ ] **0.3b** SPEC-02, SPEC-03: the rest of the ledger named; the remaining formats' vectors with their issues. Check: every vector file reviewed by the owner.
  - Waiting 22 Nov 2026: the owner's review of `test/vectors/base/` and `test/vectors/json/`.
- [x] **0.4a** FND-02, FND-04 without the phone: the test runner on the Mac and on the Android emulator over `adb`; cross-builds; the board contract and freestanding links with their symbol lists. Check: a native run on the Mac; an emulator. Passed 22 Nov 2026 (native run on the Mac; Android 16 emulator).
- [ ] **0.4b** FND-02, FND-04 on the phone and in CI: the runner on the physical Android phone; GCC and MSVC compile checks and the cross-builds in CI. Check: the physical Android phone; CI.
  - Blocked 22 Nov 2026: no physical phone attached; CI waits for the owner's push.
- [x] **0.5** BEN-01: the third-party fetcher; LoadGen built from its pinned sources; the C shim, `pr3_mlperf` and the loopback SUT; SingleStream and Offline smoke runs. Check: a native run on the Mac, both runs `VALID`. Passed 21 Nov 2026 under `release`, `asan` and `tsan`.
- [x] **0.6a** BEN-02's `bench/c` sample format. Check: a native run on the Mac. Passed 22 Nov 2026 (native run on the Mac; also on the Android emulator).
- [ ] **0.6b** BEN-02: the `llamacpp` SUT and llama.cpp's baselines on both devices; the manifest hashed. Check: a native run on the Mac; the physical Android phone; M0 closed.
  - Blocked 22 Nov 2026: the pinned model files, the Llama 3.1 licence and the physical phone (long-lead items).

## M1 Portable core and first chat on macOS and Android

- [x] **1.1** CORE-01: JSON, with its fuzz target and its budget row. Check: vectors; fuzzing; the row recorded. Passed 22 Nov 2026 (native run on the Mac): the vectors; one hour of `fuzz_json` with no failure; the `json` row's first measurement in [part 15](15-benchmarks.md#results-so-far). The phone's measurement goes with BEN-03.
- [x] **1.2** CORE-02: the agent runtime and its ledger tests. Check: `asan`, `ubsan`, `release`; vectors; the rows recorded. Passed 22 Nov 2026 (native run on the Mac): `dev`, `asan`, `ubsan`, `tsan` and `release` pass all 117 tests, the core's 50 among them; the core vectors (61 scenarios, 2,378 calls) replay byte for byte; ten minutes of `fuzz_core_step` with no failure; the three `core` rows' first measurement in [part 15](15-benchmarks.md#results-so-far). The phone's measurement goes with BEN-03.
- [ ] **1.3** CORE-03: the trusted side and its ledger tests. Check: vectors; `asan`, `ubsan`; the rows recorded.
- [ ] **1.4** CORE-04: CBOR and the wire messages. Check: vectors; fuzzing; the row recorded.
- [ ] **1.5** INF-01, INF-02: prompts, the text pipeline and the tool-call parsers. Check: vectors; fuzzing; the row recorded.
- [ ] **1.6** INF-03: the llama.cpp backend with the pinned model. Check: a native run on the Mac.
- [ ] **1.7** INF-04: the first chat on both devices. Check: a native run on the Mac; the physical Android phone; device records archived.
- [ ] **1.8** BEN-03: the first module budgets and the host's overhead under LoadGen, on both devices. Check: results archived; M1 closed.

## M2 SDL3 chat app on macOS and Android

- [ ] **2.1** APP-01: the app shell. Check: the Mac and the physical phone; idle CPU 0%.
- [ ] **2.2** APP-02: `pr3_ui` and the text stack. Check: rendering fixtures; the physical devices.
- [ ] **2.3** APP-03: the chat screen. Check: the physical devices; the app targets.
- [ ] **2.4** APP-04: VoiceOver and TalkBack. Check: the physical devices with their screen readers.
- [ ] **2.5** APP-05: the chat on an iPhone. Check: a physical iPhone; M2 closed.

## M3 The full product

- [ ] **3.1** DATA-01: approval history across builds. Check: vectors; a run across two builds.
- [ ] **3.2** DATA-02: the encrypted stores. Check: the fixtures.
- [ ] **3.3** DATA-03, DATA-06: memory, tasks and tools. Check: the fixtures and their ledger tests.
- [ ] **3.4** DATA-04, DATA-05: documents and research. Check: the suites; the physical devices.
- [ ] **3.5** DATA-07: settings, models and observability. Check: the fixtures.
- [ ] **3.6** DATA-08, DATA-09: pairing, node, client and continuity. Check: the physical Mac and phone, both roles.
- [ ] **3.7** DATA-10: every harness. Check: the ledger; M3 closed.

## M4 Embedded clients on a real node

- [ ] **4.1** EMB-01: the accessory protocol. Check: CI; fuzzing.
- [ ] **4.2** EMB-02, EMB-03: the Pico and the gateway. Check: a physical Pico.
- [ ] **4.3** EMB-04, EMB-05: both paths and their failures. Check: the physical boards.
- [ ] **4.4** EMB-07: the Pi 5 node. Check: a physical Pi 5.
- [ ] **4.5** EMB-06: the board port kit. Check: a clean checkout; M4 closed.

## M5 The app on five operating systems

- [ ] **5.1** APP-06: every screen and host command. Check: the physical devices.
- [ ] **5.2** PLT-01, PLT-02: platform services and key custody. Check: the physical devices.
- [ ] **5.3** APP-07: Windows and Linux. Check: physical machines with their screen readers.
- [ ] **5.4** APP-08, APP-09: installed data and five packages. Check: packaged apps on physical devices; M5 closed.

## M6 The PR3 engine

- [ ] **6.1** ENG-01, ENG-02: GGUF, the tokenizer and the scalar kernels. Check: vectors.
- [ ] **6.2** ENG-03, ENG-04, ENG-05: execution, decoding and state. Check: vectors.
- [ ] **6.3** ENG-06: the headless engine on the Mac, the phone and the Pi 5. Check: the physical devices.
- [ ] **6.4** ENG-07, ENG-08: SIMD and GPU. Check: the physical devices.
- [ ] **6.5** ENG-09, ENG-10: every model configuration; per-profile dependencies. Check: qualification records; CI; M6 closed.

## M7 Release candidates and qualification

- [ ] **7.1** REL-01: safety and failure paths, proofs. Check: CI and nightly; the physical boards.
- [ ] **7.2** REL-02: the ledger closed. Check: the ledger.
- [ ] **7.3** REL-03, REL-04: packages, recovery, the clean-room build and the soak. Check: the physical devices; the second builder; M7 closed.

## M8 Published benchmarks and 1.0

- [ ] **8.1** BEN-04: frozen artifacts and instruments. Check: the manifest.
- [ ] **8.2** BEN-05, BEN-06, BEN-07: the measurements. Check: the archived samples and runs.
- [ ] **8.3** BEN-08, BEN-09: the published report and 1.0 per profile. Check: the owner's decision; M8 closed.

---

[Back to the index](../README.md)
