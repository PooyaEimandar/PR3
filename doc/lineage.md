# Lineage from the AI OS plan

Part of the [PR3 documentation](README.md).

PR3 began as a plan for a pure AI OS, the direct ancestor of these documents. Its thesis - a trusted execution, identity, memory and policy layer for AI agents, with the model treated as an untrusted proposal generator - is PR3's thesis. What changed is the vehicle: a secure operating platform there, an embeddable runtime here. The plan's Sec. 31.5 listed the conditions under which it should pivot to "an Android/Linux agent runtime without a complete OS", a signing vault or an OEM security SDK; v1.2 and v1.3 are that pivot, taken before the costs of the OS route were paid rather than after.

| AI OS plan section | Disposition | Where in v1.3 |
|---|---|---|
| Sec. 1 Thesis: trusted execution, identity, memory, policy layer | **Reused** as the trust architecture, not as an OS | Sec. 3, Sec. 7, ADR-002 |
| Sec. 2, Sec. 9, Sec. 10 Verified microkernel + AOSP + Debian, VMM, SystemUI, compositor | **Dropped**; the microkernel kept as a preserved tier | Sec. 7.3, ADR-002 |
| Sec. 3.2 "What PR3 is not" | **Reused** and extended | Sec. 1.2 |
| Sec. 4 Why now: OS vendors moving into agent infrastructure; differentiate below the assistant layer | **Reused** | Sec. 2 |
| Sec. 5 Humane/Rabbit lessons | **Reused** as a principle | Sec. 3 "stay useful when the model is gone" |
| Sec. 6 Enterprise-first wedge and beachheads | **Not carried over** (commercial) | - |
| Sec. 7 Workflows | **Partially reused**: controlled communications shape Sec. 4.6; the executive briefing is the node's proactive job | Sec. 4.6, Sec. 6.3 |
| Sec. 8 Security philosophy, threat model, invariants, claims discipline | **Reused** | Sec. 3, Sec. 7.1, Sec. 7.3 |
| Sec. 11 Native services: identity, vault, policy, capability, audit, update, confirm | **Reused** as `no_std` modules plus the `pr3-vault` adapter; update verifier reduced to model/policy-pack rollback; confirm realized as phone-side confirmation | Sec. 4.1, Sec. 4.2, Sec. 6.3, Sec. 7.2, Sec. 12 |
| Sec. 12 Structured intent and execution sequence | **Reused**, adapted to `mail.send` | Sec. 4.6 |
| Sec. 13 Memory ownership, data separation, disclosure policy | **Reused** | Sec. 4.3, Sec. 4.4 |
| Sec. 14 Connector model | **Reused** | Sec. 4.7 |
| Sec. 15 Hardware strategy, DMA/IOMMU | **Dropped** (relevant only to a `secure` tier) | ADR-002 |
| Sec. 16 `pr3-dev` / `pr3-protected` / `pr3-secure` | **Reused** as trust tiers | Sec. 7.3 |
| Sec. 17 Repository structure | **Adapted**: trusted-side modules inside `pr3-core`, `pr3-vault`, `pr3-protocol` | Sec. 4.2 |
| Sec. 18 Test layers, formal methods, release channels | **Reused** | Sec. 12 |
| Sec. 19 Open-core licensing | **Adapted**: Apache-2.0 open-source core | Sec. 15 |
| Sec. 20 Pricing | **Not carried over** (commercial) | - |
| Sec. 21 Go-to-market and design-partner package | **Not carried over** (commercial) | - |
| Sec. 22 Competitive positioning | **Partially reused** (platform vendors as agent infrastructure) | Sec. 2 |
| Sec. 23 Moat | **Not carried over** (commercial) | - |
| Sec. 24–25 Roadmap and 90-day plan | **Dropped**, superseded | Sec. 11 |
| Sec. 26–30 Team, budget, fundraising, cap table, scenarios | **Not carried over** (commercial) | - |
| Sec. 31 KPIs and gates | **Reused**: reliability targets and gate discipline | Sec. 14 |
| Sec. 31.5 Pivot conditions | **The clause this document executes** | Sec. 0, ADR-002 |
| Sec. 32 Risk register | **Partially reused** (trusted-path spoofing, scope creep, "users do not pay for security") | Sec. 13 |
| Sec. 33 Compliance roadmap | **Dropped**: PR3 is an open-source research project | - |
| Sec. 34 Naming | **Not carried over**; the trust tiers keep the `dev` / `protected` / `secure` names | Sec. 7.3 |
| Sec. 35 Investor pitch | **Not carried over** (commercial) | - |
| Sec. 36 Final recommendation | Superseded | Sec. 1.3 |

The AI OS plan's commercial sections - go-to-market, pricing, design partners, team and financing - are not carried over; PR3 is an open-source research project.
