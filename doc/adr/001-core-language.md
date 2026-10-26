# ADR-001 - Implementation language for the PR3 core

Part of the [PR3 documentation](../README.md).

**Status:** Accepted, 19 October 2026. **Decision:** Rust-first (v1.1/v1.2) over C++20 with C ABI (v1.0).

**Context.** Two architectures were drafted: a C++20 core with a stable C ABI, and a Rust core with direct Cargo integration and an optional C ABI. Additional targets were then raised: embedded and mobile-desktop apps, Emscripten Unreal HTML5 games, and native games on desktop, mobile and consoles.

**Decision drivers (for Rust).**
1. The first host applications are Rust/Tauri; a C++ core would put a C ABI between them and their own engine for no benefit.
2. The benchmark baseline's native layer (OpenJarvis) is Rust crates; porting, diffing and selective reuse are direct.
3. The threat model is dominated by parsing attacker-controlled bytes (prompt injection, tool output, protocol frames, connector responses); safe-by-default parsing is a product feature for a runtime that authorizes consequential actions.
4. `wasm32-unknown-unknown` with a thin JS bridge is first-class; C++→Wasm drags an Emscripten runtime along.
5. Cargo workspaces, lockfiles, `cargo deny` and a unified toolchain across eight-plus targets suit a one-person team far better than CMake.

**Costs accepted (where C++ would have been easier).**
- llama.cpp is C/C++; Rust owns the bindings. Mitigated by the `pr3-llamacpp` strategy ([Sec. 4.3](../architecture.md#43-inference-strategy)).
- Toolchain reach: console SDKs, some certified OEM toolchains and exotic MCUs are C/C++-only. Mitigated by the protocol-first reach model and `pr3-client-c` ([Sec. 4.2](../architecture.md#42-rust-workspace-host-api-c-abi-and-protocol-client), [Sec. 6.5](../deployment.md#65-what-ships-where)).
- A single Rust+Unreal Emscripten binary is fragile. Mitigated by compiling the C client with the Unreal fork's own emsdk ([Sec. 6.6](../deployment.md#66-browser-wasm-and-unreal-html5-integration)).
- Core ML/MLX/ExecuTorch have native C++/ObjC APIs. Not blocking; llama.cpp and the platform-model shims cover the baseline.

**Alternatives rejected.**
- *C++20 core (v1.0):* right only if in-process LLM inference on consoles or inside a single Unreal HTML5 binary were a day-one requirement. It is not ([Sec. 1.2](../overview.md#12-non-goals-for-the-initial-release), [Sec. 6.7](../deployment.md#67-game-consoles---deferred-path-preserved)).
- *Dual cores (C++ for consoles, Rust elsewhere):* two implementations of agent semantics diverge; rejected.
- *C core:* maximal toolchain reach, worst memory-safety story for a parser-heavy runtime; rejected for the core, adopted only for the thin protocol client.
- *Build on ZeroClaw:* a mature Rust agent daemon with strong security engineering and a large community. Rejected as a base because its center is what PR3 must replace - a Tokio-bound runtime that depends on every other crate, a daemon operating model phones forbid, inference only through HTTP providers, and no mobile or embedding story - while its edges (channels, gateway, service management) are what PR3 does not need. Forking would mean removing the runtime and diverging from a fast-moving upstream whose obvious next steps (in-process inference, an eval harness) will be built there anyway. Retained as a landscape comparable ([Sec. 2.1](../overview.md#21-landscape-and-related-work)), a benchmark comparator for tool reliability and security ([Sec. 8.1](../benchmarks.md#81-equal-conditions-openjarvis-comparison)), and a per-component reuse source at the adapter layer ([Sec. 5.1](../related-work.md#51-zeroclaw-reuse-candidates-at-the-adapter-layer)).

**Revisit triggers.** This decision is reopened if (a) in-process console inference becomes a day-one requirement for a signed, funded game integration, or (b) `pr3-core` can no longer be kept `no_std + alloc` and panic-free, which would remove the custom-target console path that makes Rust viable there. Either condition is to be raised as an RFC with evidence, not decided by drift.
