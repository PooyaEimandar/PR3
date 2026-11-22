# Build, CI and quality gates

How PR3 is built, and the checks that make the safety claims in [part 03](03-memory-safety.md) true on every change rather than on a good day. A change merges only when every gate below that applies to it passes; the owner reviews every change.

## Building

```bash
tools/fetch-third-party.sh
cmake --preset dev && cmake --build --preset dev && ctest --preset dev
```

`tools/fetch-third-party.sh` checks out every entry of `third_party/manifest.txt` at its pinned commit and skips entries already there; the build never downloads anything.

| Preset | Purpose | Flags beyond the warning set |
| --- | --- | --- |
| `dev` | Day-to-day | `-O1 -g`, assertions on, `-ftrivial-auto-var-init=pattern` |
| `asan` | Memory errors, arena and pool poisoning | `-fsanitize=address,undefined -fno-omit-frame-pointer`, `PR3_ASAN_POISON=1` |
| `ubsan` | Undefined behaviour, trapping | `-fsanitize=undefined -fno-sanitize-recover=all` on every target; `integer,bounds,nullability` on PR3's own targets only (`PR3_SANITIZE_OWN`), where code that wraps on purpose is marked `PR3_WRAPPING` |
| `msan` | Uninitialized reads (Linux, Clang) | `-fsanitize=memory -fsanitize-memory-track-origins`; no benchmarks |
| `tsan` | Host threads and rings | `-fsanitize=thread` |
| `fuzz` | libFuzzer targets ([Fuzzing](#fuzzing)) | Upstream Clang; `-fsanitize=address,undefined,fuzzer-no-link` on every target, `-fsanitize=fuzzer` on the fuzz runners |
| `release` | Shipping | `-O2`, LTO, the hardening set below, assertions off, `PR3_CHECK` kept |
| `android-arm64` | Android with the NDK (`ANDROID_NDK_HOME`), API 26, arm64-v8a | The NDK's toolchain file; tests and `bench/c`, no MLPerf harness (`PR3_BUILD_MLPERF=OFF`) |
| `armv6m`, `rv32imc` | Freestanding link checks for the Pico's Cortex-M0+ and the ESP32-C3's RV32IMC ([part 11](11-embedded-profiles.md)) | Upstream Clang and `ld.lld` with no C library, `-Os`, `-ffreestanding`; link maps and symbol lists kept |
| `ios`, `linux-aarch64`, `pico`, `esp32c3` | Planned: iOS and Linux arm64 cross builds; the board SDK builds | The toolchain files under `cmake/toolchains/` |

**Warnings, as errors, on Clang and GCC:** `-Wall -Wextra -Wpedantic -Wconversion -Wsign-conversion -Wshadow -Wcast-qual -Wcast-align=strict -Wvla -Wswitch-enum -Wimplicit-fallthrough -Wstrict-prototypes -Wmissing-prototypes -Wmissing-variable-declarations -Wnull-dereference -Wformat=2 -Wundef -Wdouble-promotion -Wpointer-arith -Wwrite-strings`. **MSVC:** `/W4 /WX /sdl /permissive-` plus the equivalent conversion warnings.

**Hardening in `release`:** `-D_FORTIFY_SOURCE=3`, `-fstack-protector-strong`, `-fstack-clash-protection`, `-fcf-protection=full` on x86-64, `-mbranch-protection=standard` on AArch64, PIE, `-Wl,-z,relro,-z,now` on ELF, `-ftrivial-auto-var-init=zero`; `/guard:cf /CETCOMPAT /DYNAMICBASE /HIGHENTROPYVA` on Windows.

Third-party libraries are built by our CMake with their own warnings left alone but with our sanitizer and hardening flags, so ASan sees across the boundary.

## Gates

| Gate | Check | When |
| --- | --- | --- |
| Builds | `dev` and `release` on macOS arm64 (Apple Clang), Linux x86-64 (GCC and Clang), Linux arm64, Windows x64 (MSVC and `clang-cl`); cross builds for Android arm64, iOS, Pico and ESP32-C3 | Every pull request |
| Tests | Every unit and vector test in `asan`, `ubsan` and `release`; `msan` on Linux; `tsan` on the host tests | Every pull request |
| Vectors | Every golden vector under `test/vectors/` replays with identical output, and `tools/gen-vectors/gen_vectors.py --check` and `tools/gen-workloads.py --check` regenerate nothing new; a changed vector is a format version change ([D-15](19-decisions-and-risks.md), [D-21](19-decisions-and-risks.md)) | Every pull request |
| Banned functions and layers | `tools/check-banned.sh`, `tools/check-layers.sh` | Every pull request |
| Static analysis | Clang static analyzer, `clang-tidy` with the repository profile, GCC `-fanalyzer`; no new findings | Every pull request |
| Fuzzing | Each target 60 s on the pull request; each target 1 hour nightly; crashes become regression inputs under `test/fuzz/<target>/regressions/` | Pull request, nightly |
| Model checking | CBMC harnesses for arena, pool, buddy, ring, builder, UTF-8 and the JSON number path | Nightly, and before each milestone closes |
| Budgets | Arena high-water marks of the worst case each limit allows fit the sizes `pr3_core_size` computes; board images within their budgets | Every pull request |
| Profiles | `tools/check-profile.sh` on every artifact's link map | Every pull request |
| Traps | `tools/check-traps.sh` lists every `PR3_CHECK` in `tools/traps.txt`, and refuses one split over lines, which the list would miss; a new one needs a reason in review | Every pull request |
| Freestanding links | `ctest --preset armv6m` and `rv32imc`: each freestanding image's undefined symbols equal its reviewed list in `board/freestanding/symbols-<image>-<target>.txt` | Every pull request |
| Format | `clang-format` 22.1.8 with the repository's style (other versions lay some lines out differently); documentation links checked by `tools/check-links.sh` | Every pull request |
| Benchmarks | The module microbenchmarks against the archived baseline; a regression over 5% at the median blocks the merge unless explained ([part 15](15-benchmarks.md#module-budgets)) | Every pull request touching a measured module |
| MLPerf smoke | `bench/mlperf` builds LoadGen and runs SingleStream and Offline against the loopback system under test to a `VALID` result | Nightly, and every pull request touching `bench/mlperf/` |
| Device runs | The test runner on the physical Android phone over `adb` and on the Mac; on boards over UART | Each checklist step that names them, recorded by the owner |

## Fuzzing

Each target is `test/fuzz/fuzz_<target>.c`, built by the `fuzz` preset with a dictionary beside it where one helps. `tools/fuzz-seeds.py` turns every input and output of the target's golden vectors into its seed corpus, or, for a target that reads its input as a script, writes a few scripts by hand and random ones from fixed seeds; `ctest -L fuzz` replays the seeds and `test/fuzz/<target>/regressions/`. `tools/fuzz.sh <target> <seconds>` fuzzes from the seeds and the corpus kept in `build/fuzz/corpus/<target>/` and writes any failing input to `build/fuzz/crashes/<target>/`; a failing input is minimized and checked in under `regressions/` with the fix. A target asserts properties, not just the absence of crashes: for `fuzz_json`, an arena sized by the published bounds never runs out, every refusal is rewound and says where, every accepted tape is well formed, and the canonical form is a fixed point. For `fuzz_core_step`, two cores run the same script of steps, registrations, session calls and queries in lockstep and must agree byte for byte; every refusal names its error and changes nothing; requests, calls, proposals and traces keep the core's invariants; and nothing stays in use once every session is forgotten ([part 04](04-core-runtime.md#tests)). A failing check prints its line before the target aborts.

Fuzzing time is counted only while the machine is awake: a laptop that sleeps mid-run shows a false timeout as long as the sleep (libFuzzer's clock is wall time), so long local runs are wrapped in `caffeinate -i` on macOS.

## Model checking

`tools/run-cbmc.sh` runs each harness in `test/cbmc/` under CBMC 6 with bounds, pointer, overflow, conversion, shift and division checks and unwinding assertions, so a loop bound too small for the code fails the run instead of hiding a path. A harness drives one primitive through every sequence of operations up to a small bound. Bounds are set per loop where one loop needs far more passes than the rest (the buddy heap clears every order its type allows, then walks at most four). Two harnesses are narrower than the code they cover, on purpose:

- The ring is checked in its fenced-word mode (`PR3_RING_FENCED_WORDS=1`), because CBMC does not model Clang's C11 atomic builtins. The index arithmetic is the same in both modes; ordering between threads is ThreadSanitizer's (`test_ring_transfers_across_threads`).
- The JSON number harness covers every input of up to six bytes except those whose exponent is between 10 and 399: those reach the exact 800-digit decimal, whose loops run hundreds of times. Its loops are bounded at zero, so an input the harness keeps that reaches them fails the run. The exact decimal is covered by the number vectors, the random round trips in `test_json.c` and `fuzz_json`.

## Kinds of check

A step in the [checklist](18-delivery-checklist.md) says which kind of check ticks it, and a lower kind never ticks a step that asks for a higher one:

1. CI on hosted runners.
2. A native run on the development Mac.
3. An emulator or simulator.
4. A physical device (the Android phone, an iPhone, the Pi 5, a board).
5. A packaged, installed app on a physical device.

## Workflows

| Workflow | Runs |
| --- | --- |
| `ci.yml` | Pull requests and pushes to `main`: the gates above that say "every pull request" |
| `nightly.yml` | Long fuzzing, CBMC, the full benchmark matrix on the hosted runners, dependency advisories |
| `release.yml` | Tags: reproducible release builds for each profile, SBOMs, signatures, checksums; packages per OS |

CI never holds signing keys for the app stores; signing is a separate, manual job on the owner's machine until a release process is decided.

---

[Back to the index](../README.md)
