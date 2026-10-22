# Risks and governance

Part of the [PR3 documentation](README.md). Section numbers are stable; code comments cite them as “docs Sec. N”.

## 13. Risks & Mitigations

| Risk | Severity | Mitigation |
|---|---|---|
| llama.cpp binding drift and build complexity leaking into every `cargo build` | High | Vendored pinned commit; frozen C shim; prebuilt per-target libs; upgrade cadence with conformance run; all `unsafe` in one crate |
| Unsafe Rust/FFI and native backend vulnerabilities | High | Safe-Rust core enforced by CI; `unsafe` confined to two audited crates and three platform-FFI `sys` modules; fuzzing, sanitizers and C ABI tests |
| `no_std` discipline erodes (a `std::fs` or Tokio dependency slips into core) | Medium | Separate `no_std` check job on a target with no `std`; forbid-unsafe lint job; [ADR-001](adr/001-core-language.md) names this as the condition that reopens the language decision |
| Platform models displace the bundled backend, or change behavior/availability | Medium | Model-neutral trait; platform models are backends not dependencies; M2 benchmark decides bundling per platform; telemetry measured before default-on |
| Mobile memory/thermal/battery limits | High | Platform model where available; quantized small bundled model; deterministic pre-filter before LLM; short-context profiles; graceful degradation |
| Different tool-calling behavior by model | High | Per-model templates, declared `ModelCapabilities`, structured schemas, conformance suite, reject malformed actions; evaluate reuse of `zeroclaw-tool-call-parser` ([Sec. 5.1](related-work.md#51-zeroclaw-reuse-candidates-at-the-adapter-layer)) |
| Emscripten/Unreal fork version drift breaks the HTML5 client | Medium | Pinned emsdk CI job; C11 with no toolchain-specific features; single-binary Rust+Unreal explicitly out of scope |
| Python-free runtime falls behind research | Medium–High | Separate training/research environment; export learned artifacts; avoid reinventing all ML |
| OpenJarvis improves upstream faster | High | Track upstream monthly; run fixed and rolling baselines; compete on specific measurable outcomes |
| Platform owners absorb the agent layer within their own ecosystems | High | Stay underneath: platform models as backends, OS hooks as connectors ([Sec. 2](overview.md#2-thesis--positioning)); differentiate on cross-platform policy, memory and audit; adoption through developers who embed PR3, since end users rarely adopt a security layer for its own sake |
| Unauthorized tool activity | Critical | Capability security, protected credentials, per-action signing, kill switch, fuzzing and audit |
| Background execution unreliable on phones | High | Proactive work on the user-controlled node; honest feature limits; notifications not guaranteed offline |
| E2EE sync data loss/conflicts | High | Event logs, stable IDs, tombstones, recovery backups, fault-injection tests |
| Console scope creep before a partner exists | Medium | [RFC-001](rfc/001-console-cognition-tiers.md) trigger; no console work beyond `no_std` hygiene and the C client until a funded partner or next-gen platform ML APIs appear |
| Trusted-path spoofing: a compromised node or UI shows one action and requests another | Critical | Phone-side confirmation rendered by PR3 from the validated intent, biometric approval, vault-signed receipt bound to exact parameters; the node holds neither an approval key nor a live provider credential ([Sec. 6.3](deployment.md#63-private-hybrid-mobile--desktop)) |
| Impersonation through a stolen personal model, voice or face data | Critical | Encrypted at rest under vault-held keys; never leaves the user's devices unencrypted; device revocation wipes it; nothing sent as the user without approval ([Sec. 7.1](security.md#71-threat-model)) |
| Approvals become rubber stamps as the personal model improves | High | Routine actions in pre-approved low-risk scopes so prompts stay rare and meaningful; deliberate friction on consequential ones; approval-time trend tracked ([Sec. 14](roadmap.md#14-success-metrics--gono-go-gates)) |
| Scope creeps back toward a full OS or microkernel platform | Medium | Trust-tier rule ([Sec. 7.3](security.md#73-trust-tiers---dev-protected-secure)): nothing beyond `dev` before M5, `secure` only on a funded RFC; [the lineage](lineage.md) records what was deliberately dropped |
| Capability or receipt schema frozen too early or too loosely | Medium | Schemas live in `pr3-protocol` with versioning; Kani/Proptest harnesses from M1; field comparison with ZeroClaw's receipt format before freezing |
| Unproven security claim | High | Pre-registered AgentDojo comparison with matched models; compare against CaMeL's published numbers; release all results including failures |

## 15. Governance & Community

Suggested governance: public RFCs/ADRs (starting with [ADR-001](adr/001-core-language.md)), versioned Rust SDK, protocol and C ABI compatibility commitments, transparent benchmark reports, contribution guidelines and a security reporting process. PR3 is licensed under Apache-2.0 (`LICENSE`).

Keep user data, credentials and personal trace logs outside the public codebase. Console SDK adapters, if ever built, live in the console partner's repositories; the public repository carries only `pr3-client-c` and the `no_std` core they build against. Public benchmarks use synthetic or consented datasets.

PR3 may claim "inspired by OpenJarvis" with attribution, but should avoid suggesting endorsement by OpenJarvis or Stanford unless such affiliation is explicitly established.
