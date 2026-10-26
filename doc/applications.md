# Applications and interoperability

Part of the [PR3 documentation](README.md). Section numbers are stable; code comments cite them as “docs Sec. N”.

## 9. Domain Applications

PR3 is application-neutral. Domain applications - productivity, developer tools, games or anything else - are built outside this repository on PR3's SDK, plugins and connectors ([Sec. 4.7](architecture.md#47-connector-and-plugin-model)). They are extensions, never dependencies of the core, and this document does not plan any of them.

### 9.1 Rules for applications that take consequential actions

- Every consequential action goes through the structured-intent sequence ([Sec. 4.6](architecture.md#46-structured-intent-and-execution-sequence)); an application may add checks, never remove them.
- Values an action depends on - amounts, limits, recipients - are computed or resolved by deterministic, validated code; the model explains and summarizes, it does not decide.
- Limits and risk settings are configured by the user, never inferred silently from conversation.
- Kill switch, per-action maximums, stale-data cutoffs, expiry, idempotency and a full audit trail.
- *No action* is always a valid output, and notification throttling stops the application from prompting users into unnecessary actions.
- No private key or unrestricted provider credential ever reaches the model.

### 9.2 Fitting the on-device model

On-device models are around the 3B class. Domain applications should give them narrow, structured jobs - explain a deterministic engine's output, summarize a signed event, answer a question from retrieval - and escalate long reasoning to the node. The deterministic engine, not the LLM, carries the correctness burden.

## 10. Application Areas & Interoperability

| Area | Example | Additional challenges |
|---|---|---|
| Personal assistant | A private, voice-first PR3 on the user's own phone and node: voice notes, email, calendar, documents, proactive briefings, drafts in the user's style | Provider OAuth permissions, background restrictions |
| Developer tools | Embedded local coding/review agent | Codebase context, performance, sandboxing |
| Games / Unreal | Local NPC cognition, persistent characters; the game as a client of the player's own PR3 node | Frame-time, deterministic game logic, platform policy on cross-title user data |
| Automotive infotainment | In-car assistant for non-safety-critical functions | NPU/OS SDKs, voice, safety separation |
| Organizations | On-prem private agent with verifiable data access | Identity, governance, integration |
| Edge / appliances | Embedded decision and tool runtime | Extremely small RAM, optional remote inference |

**Priority:** (1) repeatable benchmarks ([Sec. 8](benchmarks.md#8-benchmarking)); (2) a **private personal PR3** - the user's own phone and node, with all of their data under their control - as the first reference deployment, used daily by its own developers and exercising the trust architecture end to end; (3) other areas only after documented field reliability. Avoid opening all areas in parallel.

**Interop note (optional, post-M2):** if `pr3-node` exposes an OpenAI-compatible endpoint, third-party runtimes such as ZeroClaw can use it as a model provider, which gives a PR3 user those runtimes' channel integrations (Telegram, Discord, email) without PR3 writing channel code. This is an adoption lever to revisit after the hybrid gate, not a milestone and not a dependency.

**Open source.** PR3 is Apache-2.0 ([Sec. 15](risks-and-governance.md#15-governance--community)). The core stays fully usable by anyone, nothing is held back, and users' private memories and data are never sold or shared.

**User research.** Talk to 10–15 developers/OEM integrators and 15–20 prospective end users to learn deployment blockers, privacy requirements and workflow value. Interview counts are research goals, not evidence.
