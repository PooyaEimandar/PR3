# PR3

**PR3** is a privacy-first personal AI runtime written in C17. It runs a local model, keeps the user's memory and documents on the user's own devices, and lets an agent act only through a deterministic trusted path: structured intent, policy, the user's approval, a one-use capability and a hash-chained audit. It runs from a microcontroller accessory to phones and computers, with a native SDL3 app for the graphical part. Memory safety comes from caller-owned arenas, generational pools and bounded slices, not from a runtime ([part 03](doc/03-memory-safety.md)).

## Status

M0's foundations are built and pass on the development Mac: the build and its gates, the base library, the test runner, the freestanding links, the MLPerf Inference harness and the microbenchmark format. In M1, canonical JSON and the agent runtime pass; the trusted side is next ([roadmap](doc/16-roadmap.md), [checklist](doc/18-delivery-checklist.md)). The specification below waits for review.

## Building

```bash
tools/fetch-third-party.sh
```

```bash
cmake --preset release && cmake --build --preset release
```

The MLPerf harness with its loopback system under test, as a smoke run:

```bash
build/release/bench/mlperf/pr3_mlperf --sut loopback --scenario SingleStream --performance 256 --user-conf bench/mlperf/conf/smoke.conf --out bench/runs/smoke-singlestream
```

## Specification

**Architecture**

| Part | About |
| --- | --- |
| [Overview](doc/01-overview.md) | Design at a glance, modules, principles, what is out of scope |
| [System architecture](doc/02-architecture.md) | Modules and layers, hosts, threads, a turn, trust boundaries, profiles |
| [Memory safety and the allocation model](doc/03-memory-safety.md) | Arenas, lifetimes, pools, rings, strings, bounds, errors, banned functions, verification |
| [Core runtime](doc/04-core-runtime.md) | The sans-I/O core, the turn, tools and schemas, canonical JSON, the loop guard, traces |
| [Trusted side](doc/05-trusted-side.md) | Intents, policy, the gate, byte formats, crypto |
| [Inference](doc/06-inference.md) | The engine interface, prompts, streaming text, tool-call parsing, the llama.cpp backend, the PR3 engine |
| [Memory and storage](doc/07-memory-and-storage.md) | Memories, tasks, documents, research evidence, continuity, keys and encryption |
| [Tools and research](doc/08-tools-and-research.md) | The tool interface, the tools, the web reader, research |
| [Protocol and private hybrid](doc/09-protocol-and-hybrid.md) | Wire protocol v0, pairing, node and client, the serial accessory protocol |
| [The SDL3 app](doc/10-app-sdl3.md) | Structure, the UI layer, text, screens, host commands, platform bridges, accessibility |
| [Device and embedded profiles](doc/11-embedded-profiles.md) | Profiles, the board contract, budgets, key storage on boards |

**Engineering**

| Part | About |
| --- | --- |
| [Tech stack](doc/12-tech-stack.md) | What runs where, licences, and what was dropped on purpose |
| [Project structure](doc/13-project-structure.md) | Repository layout and rules |
| [Build, CI and quality gates](doc/14-build-and-quality.md) | Presets, warnings, hardening, gates, kinds of check |
| [Benchmarks](doc/15-benchmarks.md) | MLPerf Inference through LoadGen, agent security, module budgets, app targets, evidence |

**Delivery**

| Part | About |
| --- | --- |
| [Roadmap](doc/16-roadmap.md) | Milestones, gates and the order of work |
| [Delivery plan](doc/17-delivery-plan.md) | Milestones with exit criteria, epics and issues |
| [Delivery checklist](doc/18-delivery-checklist.md) | The plan as one ordered list of steps |
| [Open decisions and risks](doc/19-decisions-and-risks.md) | What is still to decide, and what could go wrong |
| [Test ledger](doc/20-test-ledger.md) | Every promised behaviour and the test that pins it |

Every change to the specification adds an entry to [doc/CHANGELOG.md](doc/CHANGELOG.md); its newest entry is the current version.

## Licence

Apache-2.0 ([LICENSE](LICENSE)). Third-party sources are fetched at pinned commits and keep their own licences ([part 12](doc/12-tech-stack.md#licences)).
