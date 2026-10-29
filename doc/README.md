# PR3 documentation

Start with the overview; read the rest as you need it.

| Document | What it covers |
|---|---|
| [Overview](overview.md) | What PR3 is and is not, its positioning and research hypothesis, the engineering principles |
| [Architecture](architecture.md) | Modules, the Rust workspace and crate rules, inference, memory, learning, structured intents, connectors, tasks |
| [Security, privacy and trust](security.md) | Threat model, invariants, required controls, trust tiers |
| [Deployment modes](deployment.md) | Offline mobile and desktop, the private hybrid, OEM and embedded, browsers and game engines |
| [Benchmarking](benchmarks.md) | The comparisons PR3 commits to, evaluation dimensions, acceptance gates, and results so far |
| [Applications](applications.md) | Rules for applications that act, and the areas PR3 targets |
| [Roadmap](roadmap.md) | Spike status, milestones M0–M6, success metrics and go/no-go gates |
| [Build, CI and quality gates](quality-gates.md) | Toolchain, CI matrix, formal verification targets, release channels |
| [Risks and governance](risks-and-governance.md) | Risks with mitigations; governance and attribution |
| [Related work and reuse](related-work.md) | OpenJarvis and ZeroClaw, and what PR3 takes from each; references |
| [Development](development.md) | Building, running, configuring and testing PR3 today |
| [Change log](changelog.md) | What changed in the design, version by version, and why |

Decisions and proposals:

- [ADR-001](adr/001-core-language.md): Rust for the core.
- [ADR-002](adr/002-trust-architecture.md): the trust architecture, with a verified-microkernel tier preserved for later.
- [ADR-003](adr/003-llamacpp-binding.md): `pr3-llamacpp` wraps `llama-cpp-2` for the spike.
- [RFC-001](rfc/001-console-cognition-tiers.md): console cognition tiers (deferred).
- [Lineage](lineage.md): what PR3 kept, changed and dropped from an earlier plan for a pure AI OS.

## Section numbers

These documents were one roadmap; its section numbers are kept, and code comments cite them as “docs Sec. N”.

| Sec. | Document |
|---|---|
| 0 | [Change log](changelog.md) |
| 1–3 | [Overview](overview.md) |
| 4 | [Architecture](architecture.md) |
| 5 | [Related work and reuse](related-work.md) |
| 6 | [Deployment modes](deployment.md) |
| 7 | [Security, privacy and trust](security.md) |
| 8 | [Benchmarking](benchmarks.md) |
| 9–10 | [Applications](applications.md) |
| 11, 14 | [Roadmap](roadmap.md) |
| 12 | [Build, CI and quality gates](quality-gates.md) |
| 13, 15 | [Risks and governance](risks-and-governance.md) |
