# Build, CI and quality gates

Part of the [PR3 documentation](README.md). Section numbers are stable; code comments cite them as “docs Sec. N”.

## 12. Build, CI/CD & Quality Gates

**Toolchain:** Rust stable + Cargo workspaces, `rust-toolchain.toml`, `cargo fmt`, Clippy (with the `pr3-core` deny-list), `cargo test`, `cargo deny`/`cargo audit`, reproducible dependency locks. A second, separately pinned nightly runs only Miri, cargo-fuzz and `-Zsanitizer` jobs, all of which require it; product builds never use it. CMake/Clang/Android NDK/Xcode apply only to building the vendored llama.cpp prebuilts, `pr3-client-c` and platform packaging - never to a downstream `cargo build`.

**CI matrix:**

- **Hosts:** macOS ARM64 and x86_64, Linux x86_64/ARM64, Windows x64/ARM64 where available; Android ARM64; iOS ARM64 plus simulator under supported Xcode.
- **`no_std` discipline:** `cargo check --no-default-features` for every `no_std` crate on `thumbv7em-none-eabihf` and `wasm32-unknown-unknown`, as a separate job (feature unification otherwise masks leaks); panic-site allowlist check of the `pr3-ffi` core build ([Sec. 4.2](architecture.md#42-rust-workspace-host-api-c-abi-and-protocol-client)).
- **Safety:** `#![forbid(unsafe_code)]` verified by a lint job that fails on any new `unsafe` outside `pr3-llamacpp`, `pr3-ffi` and the `sys` modules of `pr3-platform-llm`/`pr3-vault`/`pr3-speech`; cargo-fuzz corpora for JSON, schema, protocol frames and model-template parsers; Miri on `pr3-core` tests; ASan on the audited `unsafe` crates; UBSan on the llama.cpp and whisper.cpp shims and `pr3-client-c` (UBSan does not apply to Rust code; Miri is its Rust counterpart).
- **Webview:** the app's TypeScript type-checked in strict mode by its pinned compiler, kept to syntax any type-stripping compiler can erase.
- **App builds:** the app packaged for the iOS simulator, Android arm64, macOS on Apple Silicon, Windows x64 and Linux x64 on every push and pull request. Packages are built with no secret in reach; the Android dev key is used only by a separate job that runs nothing the build made and takes the APK by the digest the build reported. Builds of `main` are attested and kept 7 days; a release, started from the Actions page, tags `main`'s newest commit with its own time and publishes every platform as a permanent release. Every version comes from the commit's time ([development.md](development.md#dev-builds)).
- **Prebuilts:** llama.cpp + shim static libs built per target from the pinned commit and published as versioned artifacts with checksums, fetched by the explicit step in [Sec. 4.3](architecture.md#43-inference-strategy) - never by `build.rs`.
- **C client:** Linux build from M0; from M4, freestanding build with stubbed callbacks and Emscripten build on the emsdk pinned by the recorded Unreal fork; libFuzzer on the frame parser.
- **Smoke tests:** direct Cargo/Tauri host, C ABI minimal host, UniFFI Swift/Kotlin bindings, protocol round-trip Rust↔C.
- **Determinism:** protocol/unit tests; model-backed tests with version-pinned weights; platform-model tests marked as device-gated.
- **Supply chain:** security scanning, SBOM, reproducible builds where feasible.

**Verification beyond tests - narrow by design.** Do not attempt to verify the whole product. Apply formal methods where a bug costs money or trust, and nowhere else:

| Component | Method | What is proven or checked |
|---|---|---|
| Policy decision, capability issuance/expiry, approval-receipt binding (`pr3-core`, `no_std`) | Kani bounded model checking on the Rust source; Proptest properties | No path grants a capability without a matching receipt; expired or replayed receipts never authorize; a plugin's grants cannot widen at runtime |
| Hybrid handoff and sync protocol (`pr3-protocol`, `pr3-hybrid`) | TLA+ (or Quint) specification checked with TLC under node loss, message duplication, reordering and replay | No duplicated consequential action; state reconciles; tombstones win over stale writes |
| Audit chain | Property tests plus a standalone verifier tool | Any rewrite breaks the chain; checkpoints verify against the device key |
| Model and policy-pack updates | State-machine model plus power-loss tests | Never left without a runnable model or policy; rollback always possible |

Required test layers in addition: unit tests for every deterministic policy rule; malformed-model-output and hostile-protocol-frame fuzzing; replay and race tests; host-compromise simulation (a node that lies, a plugin that over-reaches, a UI that spoofs); power-loss and interrupted-download tests; cross-version protocol conformance; a red-team pass before any live execution; an independent security review before live consequential actions ship.

**Release channels:** `dev` (no security claims) → `stable` (signed release) → `secure-reference` (fixed hardware and reviewed configuration, if and when a `protected` or `secure` tier ships). Claims are scoped per channel and per tier ([Sec. 7.3](security.md#73-trust-tiers---dev-protected-secure)). [Sec. 11](roadmap.md#11-delivery-roadmap-epics--issues) maps channels to milestones.

**Portability warning:** Cargo compilation and the C ABI do not eliminate Apple signing, Android/iOS background limits, GPU/NPU backend availability or platform-specific SDK restrictions. Browser Wasm requires browser-adapted storage/network/scheduler adapters. Console support is not part of initial acceptance criteria.
