# Related work and reuse

Part of the [PR3 documentation](README.md). Section numbers are stable; code comments cite them as “docs Sec. N”.

## 5. OpenJarvis-Inspired Implementation Mapping

The current upstream OpenJarvis contains Python modules for memory, training and learning orchestration, as well as multiple Rust crates for agents, engines, skills, traces, learning and security. The inspected Rust llama.cpp adapter calls a **local HTTP `/completion` server**; PR3 links the model **in process** on mobile and desktop. A Rust ReAct implementation extracts `Action` and `Action Input` with regex; PR3 prefers provider-supported structured tool calling, strict JSON schema validation and deterministic permission checks.

| OpenJarvis component / source area | Why it matters | PR3 approach | Priority |
|---|---|---|---|
| `rust/crates/openjarvis-core` | Types/events/registry | Minimal `no_std` Rust traits/events; C ABI and protocol for non-Rust hosts | P0 |
| `rust/crates/openjarvis-agents` | ReAct/monitor/orchestrator | Single bounded sans-I/O state machine, structured actions, cancellation | P0 |
| `rust/crates/openjarvis-engine` | Provider abstraction, llama.cpp HTTP bridge | In-process local inference, platform-model backends, authenticated private-node adapter | P0 |
| `src/openjarvis/memory` | Provenance, extracted facts, persistence | SQLite provenance, trust tiers, policy-aware retrieval and deletion | P0–P1 |
| `rust/crates/openjarvis-learning` | Trace policy/bandit/reward | Native adaptive policy with offline evaluation and rollback | P1 |
| `src/openjarvis/learning` | Data extraction, optimization, LoRA | Trace-driven policy evaluation in Rust; neural research outside runtime | P2 |
| `agents/monitor_operative`, scheduler | Persistent monitoring | Durable scheduled task/events on `pr3-node` for 24/7 jobs | P1 |
| OpenJarvis benchmarking | Accuracy/cost/energy emphasis | Reproducible shared harness with matched baselines | P0 |
| Skills and tool ecosystem | Reusable capabilities | Minimal plugin manifest + permission schema; don't import arbitrary code by default | P1 |

**Attribution:** any reused or modified OpenJarvis file keeps its notices, and the README credits OpenJarvis ([Sec. 15](risks-and-governance.md#15-governance--community)).

Useful source references:

- [OpenJarvis repository](https://github.com/open-jarvis/OpenJarvis)
- [OpenJarvis README](https://github.com/open-jarvis/OpenJarvis/blob/main/README.md)
- [Rust llama.cpp adapter](https://github.com/open-jarvis/OpenJarvis/blob/main/rust/crates/openjarvis-engine/src/llamacpp.rs)
- [Rust ReAct agent](https://github.com/open-jarvis/OpenJarvis/blob/main/rust/crates/openjarvis-agents/src/native_react.rs)
- [Rust trace-driven policy](https://github.com/open-jarvis/OpenJarvis/blob/main/rust/crates/openjarvis-learning/src/trace_policy.rs)
- [Python LearningOrchestrator](https://github.com/open-jarvis/OpenJarvis/blob/main/src/openjarvis/learning/learning_orchestrator.py)
- [Python memory store](https://github.com/open-jarvis/OpenJarvis/blob/main/src/openjarvis/memory/store.py)
- [Karpathy llm.c](https://github.com/karpathy/llm.c) - philosophical reference for readable code and cost of complexity; *not* an agent runtime.

### 5.1 ZeroClaw: reuse candidates at the adapter layer

ZeroClaw is the nearest engineering comparable to PR3 (Rust, trait-based kernel ABI, approval-gated autonomy, tool receipts, SQLite memory, WASM plugins). Its *shape* is the opposite of PR3's - a Tokio-bound daemon whose runtime depends on every other crate, inference only through HTTP providers, no mobile target - so it is not a base ([ADR-001](adr/001-core-language.md)). Specific pieces are worth evaluating for reuse under its MIT/Apache dual license, with notices retained:

| ZeroClaw component | Why it matters | PR3 approach | When |
|---|---|---|---|
| `zeroclaw-tool-call-parser` | Normalizes OpenAI-style, Anthropic-style and Qwen/Ollama tool-call formats, including streaming deltas | Candidate for output normalization in `pr3-inference` adapters; directly addresses the [Sec. 13](risks-and-governance.md#13-risks--mitigations) "different tool-calling behavior by model" risk | Evaluate before M1-E2 |
| Tool-receipt format and security policy types | Field-tested design for binding an approval to an action | Compare against PR3 approval receipts ([Sec. 7.2](security.md#72-required-controls)) before finalizing the schema; adopt field names where equivalent | M1 |
| `zeroclaw-api` trait surface | Shows which abstractions a large Rust agent community settled on | Reference when naming PR3 traits; no code dependency | M0 |

Rules: nothing from ZeroClaw enters a `no_std` crate; PR3 never depends on `zeroclaw-runtime`; each candidate is checked with `cargo tree` for Tokio/`reqwest` pull-in before adoption. Reuse is a per-component decision made on evidence, not a direction.

## References

- OpenJarvis source: https://github.com/open-jarvis/OpenJarvis
- OpenJarvis documentation: https://open-jarvis.github.io/OpenJarvis/
- OpenJarvis research paper: https://arxiv.org/abs/2605.17172
- CaMeL, "Defeating Prompt Injections by Design" (Debenedetti et al., 2025): https://arxiv.org/abs/2503.18813
- AgentDojo (Debenedetti et al., 2024): https://arxiv.org/abs/2406.13352
- Apple App Intents: https://developer.apple.com/documentation/appintents
- Karpathy `llm.c`: https://github.com/karpathy/llm.c
- llama.cpp: https://github.com/ggml-org/llama.cpp
- Rust: https://www.rust-lang.org/
- Tokio: https://tokio.rs/
- UniFFI: https://github.com/mozilla/uniffi-rs
- cbindgen: https://github.com/mozilla/cbindgen
- cargo-fuzz: https://github.com/rust-fuzz/cargo-fuzz
- Miri: https://github.com/rust-lang/miri
- cargo-deny: https://github.com/EmbarkStudios/cargo-deny
- hashbrown: https://github.com/rust-lang/hashbrown
- Ferrocene (qualified Rust toolchain): https://ferrocene.dev/
- Apple Foundation Models framework: https://developer.apple.com/documentation/foundationmodels
- ML Kit GenAI APIs (Gemini Nano): https://developers.google.com/ml-kit/genai
- Rig: https://github.com/0xPlaygrounds/rig
- Swiftide: https://github.com/bosun-ai/swiftide
- Burn (optional ML): https://github.com/tracel-ai/burn
- Qwen models: https://huggingface.co/Qwen
- Qwen3.8-27B GGUF (llama.cpp project): https://huggingface.co/ggml-org/Qwen3.8-27B-GGUF
- Qwen3.5 small GGUFs (Unsloth): https://huggingface.co/unsloth/Qwen3.5-4B-GGUF, https://huggingface.co/unsloth/Qwen3.5-2B-GGUF
- PyTorch/PEFT (optional offline training): https://pytorch.org/
- ExecuTorch: https://github.com/pytorch/executorch
- SQLite: https://sqlite.org/
- Cargo: https://doc.rust-lang.org/cargo/
- OpenClaw: https://github.com/openclaw/openclaw
- NullClaw: https://github.com/nullclaw/nullclaw
- ZeroClaw: https://github.com/zeroclaw-labs/zeroclaw
- ZeroClaw crate architecture: https://github.com/zeroclaw-labs/zeroclaw/blob/master/docs/book/src/architecture/crates.md
- Kani (Rust model checker): https://github.com/model-checking/kani
- Proptest: https://github.com/proptest-rs/proptest
- TLA+: https://lamport.azurewebsites.net/tla/tla.html
- Quint: https://github.com/informalsystems/quint
- Android Virtualization Framework: https://source.android.com/docs/core/virtualization

**Evidence note:** OpenJarvis source conclusions above derive from selective reading of the current repository structure and representative agent, engine, memory and learning files. ZeroClaw conclusions derive from its README and crate-architecture documentation as of 19 October 2026 and must be re-verified before any reuse decision. Platform-model capabilities (Apple Foundation Models, Gemini Nano) are as publicly described in October 2026 and must be re-verified against current vendor documentation at each milestone. Full upstream parity, mobile performance and security properties require dedicated test/audit work.
