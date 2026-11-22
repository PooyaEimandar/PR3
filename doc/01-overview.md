# Overview

PR3 is a privacy-first personal AI runtime. It runs a local model, keeps the user's memory and documents on the user's own devices, and decides what an agent may do through a deterministic trusted path: structured intent → policy → the user's approval → one-use capability → hash-chained audit. It is written in C17 from the bare metal up, from a microcontroller accessory to a phone and a desktop, with a native SDL3 app for the graphical part.

**The bet.** A runtime that authorizes consequential actions parses attacker-controlled bytes all day: model output, tool results, web pages, protocol frames. PR3 bets that disciplined C is as safe as a memory-safe language in practice and measurably faster and smaller, because it never pays for clones, per-call allocations, DOM trees or a WebView. The discipline is concrete: caller-owned arenas and pools, bounded slices instead of raw strings, checked arithmetic, no general heap in the portable libraries, and sanitizers, fuzzing and formal checks on every parser ([memory safety](03-memory-safety.md), [D-01](19-decisions-and-risks.md), [D-03](19-decisions-and-risks.md)). The [delivery plan](17-delivery-plan.md) does not let a module close until both halves are shown: its tests and golden vectors pass under every sanitizer, and its benchmark rows are recorded against their budgets ([benchmarks](15-benchmarks.md)).

**Scope (owner, 21 Nov 2026).** C for all product code; SDL3 for the graphical app; memory pools and arenas so safety costs no bare-metal performance ([D-01](19-decisions-and-risks.md), [D-02](19-decisions-and-risks.md), [D-03](19-decisions-and-risks.md)). Thin OS bridges use their platform languages (Swift and Objective-C on Apple, Java through JNI on Android), and GPU kernels use theirs (Metal, Vulkan GLSL).

## Design at a glance

| Area | Design |
| --- | --- |
| Language | C17 floor with checked C23 features behind macros ([D-05](19-decisions-and-risks.md)); the portable libraries have no heap at all ([part 03](03-memory-safety.md)) |
| Memory | Caller-owned arenas (per session, per turn, per step) and generational pools; zero-copy slices; budgets fixed at init and reported ([part 03](03-memory-safety.md)) |
| Core | `pr3_core`: a sans-I/O state machine, `step(input) → actions`; actions borrow from the core's arenas until the next step ([part 04](04-core-runtime.md)) |
| Trusted side | Intent validator, policy, gate, receipts, one-use capabilities, hash-chained audit; mbedTLS 3.6 LTS for SHA-256 and P-256 ([part 05](05-trusted-side.md), [D-07](19-decisions-and-risks.md)) |
| Parsing | PR3-owned bounded parsers that never allocate outside their arena: a tape JSON parser with a canonical writer, and a CBOR codec ([D-08](19-decisions-and-risks.md), [D-09](19-decisions-and-risks.md)) |
| Inference | llama.cpp through `llama.h` directly as the first backend, then the PR3 engine: a CPU engine for the Qwen3.5 family with Metal and Vulkan paths ([part 06](06-inference.md)) |
| Storage | SQLCipher through its C API: memories, tasks, documents, research evidence and continuity in one encrypted database ([part 07](07-memory-and-storage.md)) |
| App | SDL3 with a PR3-owned immediate-mode UI, HarfBuzz shaping, bidirectional text and native accessibility bridges ([part 10](10-app-sdl3.md), [D-10](19-decisions-and-risks.md)) |
| Embedded | Raspberry Pi Pico (serial accessory behind a gateway), ESP32-C3 (networked client), Raspberry Pi 5 (headless node) ([part 11](11-embedded-profiles.md)) |
| Build | CMake presets and Ninja; one static library per module and one amalgamated `libpr3` ([part 13](13-project-structure.md), [D-06](19-decisions-and-risks.md)) |
| Benchmarks | MLPerf Inference through MLCommons LoadGen for comparisons with other projects, module budgets for everything else ([part 15](15-benchmarks.md), [D-25](19-decisions-and-risks.md)) |

## Modules

| Module | What | Where |
| --- | --- | --- |
| Core | The agent loop, the tool registry and its schema subset, the loop guard, traces | `src/core/` ([part 04](04-core-runtime.md)) |
| Trusted | Intent validator, policy, gate, receipts, capabilities, audit chain | `src/trusted/` ([part 05](05-trusted-side.md)) |
| Memory | Memory types and provenance, retrieval scoring, quote references | `src/memory/` ([part 07](07-memory-and-storage.md)) |
| Store | Encrypted stores: memories, tasks, documents, research evidence, continuity | `src/store/` ([part 07](07-memory-and-storage.md)) |
| Inference | Prompt formats for Qwen3 and Qwen3.5, the streaming text pipeline, tool-call parsers, KV reuse and checkpoints, evidence fitting, the report grammar | `src/infer/`, `src/engine/` ([part 06](06-inference.md)) |
| Tools | Calculator, clock, documents, memory tools, web reader, research | `src/tool/` ([part 08](08-tools-and-research.md)) |
| Protocol and hybrid | Wire protocol v0, pairing, encrypted sessions, node and client, the serial accessory protocol | `src/proto/`, `src/hybrid/` ([part 09](09-protocol-and-hybrid.md)) |
| Platform and app | Key custody and approval signing, speech, host commands and screens | `src/platform/`, `app/pr3-app/` ([part 10](10-app-sdl3.md)) |
| Configuration | The model registry with pinned revisions and hashes, device selection, feature switches | `config/pr3.toml`, read by a bounded TOML subset reader ([part 07](07-memory-and-storage.md)) |

## Principles

1. **The model proposes, PR3 decides.** Every model output, tool result and peer reply is an untrusted proposal. Only the trusted path turns a proposal into an effect, and only with the user's approval.
2. **Correct first, then fast.** A behaviour lands with its tests and golden vectors, and is optimized only while they keep passing. Every limit and every deliberate trade-off is written down in [decisions](19-decisions-and-risks.md).
3. **No heap in the portable libraries.** The core, trusted side, parsers, protocol and memory types take memory from arenas and pools the host hands them. An allocation failure is a status, never a crash, and leaves the state machine as it was ([part 03](03-memory-safety.md)).
4. **Bounded by construction.** Every input has a byte limit, every container a capacity, every loop a bound, every recursion an explicit depth, all declared in one header, `pr3/limits.h`.
5. **Sans-I/O at the centre.** Clock, entropy, storage, network and threads are host capabilities passed in, so the same core runs on a phone, a desktop, a Pi and a microcontroller ([part 02](02-architecture.md)).
6. **Measured, not asserted.** Safety is a set of passing checks (sanitizers, fuzzers, static analysis, bounded model checking), and speed is a benchmark row with raw samples, against public benchmarks where one exists ([part 14](14-build-and-quality.md), [part 15](15-benchmarks.md)).
7. **Link only what a profile needs.** A microcontroller image never contains SQLCipher, llama.cpp or SDL3; a headless node never contains SDL3 ([part 11](11-embedded-profiles.md)).


---

[Back to the index](../README.md)
