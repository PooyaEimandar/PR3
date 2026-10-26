# ADR-002 - Trust architecture: a verified-microkernel tier preserved, not built

Part of the [PR3 documentation](../README.md).

**Status:** Accepted, 19 October 2026. **Decision:** adopt the security constitution of the earlier pure AI OS plan as PR3's trust architecture; implement it at the `dev` tier on today's operating systems; keep `protected` and `secure` reachable through the `no_std` trusted-side modules; do not build the operating system, shells, VMM or hardware.

**Context.** The AI OS plan proposed PR3 as a platform hosted on a formally verified microkernel, with AOSP and Debian as untrusted guests, native microkernel services for identity, vault, policy, capability, audit, update and confirmation, and a seven-person team to build it. v1.2 redefined PR3 as an embeddable personal-AI runtime. The question was what of the AI OS plan survives.

**What is adopted.** Model ≠ authority; the propose/dispose split; the invariants in [Sec. 7.1](../security.md#71-threat-model); structured intent, validator and execution sequence ([Sec. 4.6](../architecture.md#46-structured-intent-and-execution-sequence)); capability/approval receipts ([Sec. 7.2](../security.md#72-required-controls)); hash-chained audit ([Sec. 4.4](../architecture.md#44-memory-and-knowledge-architecture)); identity with revocation ([Sec. 6.3](../deployment.md#63-private-hybrid-mobile--desktop)); the three-target ladder as trust tiers ([Sec. 7.3](../security.md#73-trust-tiers---dev-protected-secure)); narrow formal methods ([Sec. 12](../quality-gates.md#12-build-cicd--quality-gates)); the claims discipline - security claims are per tier and configuration, never "the whole system is verified".

**What is not adopted.** The microkernel as a product, AOSP and Debian guests, custom SystemUI/launcher and Wayland compositor, a VMM, reference hardware and DMA/IOMMU strategy, a dedicated phone, and the team, budget and valuation built around them.

**Drivers.**
1. v1.2's thesis is an embeddable runtime inside existing apps and OSes; an OS is a different project with different users.
2. In the AI OS plan, microkernel, VMM and board bring-up consumed most of the team and most of the risk register while delivering nothing an embeddable runtime's first users would notice.
3. The AI OS plan's own pivot conditions apply: a verified microkernel is not worth its cost until a deployment requires host-compromise resistance; none does yet.
4. Platform key storage plus phone-side confirmation deliver the user-visible guarantee for consequential actions at the `dev` tier: the model never sees a credential, and a compromised node cannot approve.
5. The `no_std`, sans-I/O, panic-free trusted-side modules are exactly what a protection domain on a verified microkernel would run, so the option is preserved at zero marginal cost.

**Costs accepted.** The `dev` tier does not protect against a compromised OS kernel and every claim must say so. If a deployment requires that protection, PR3 is one tier away rather than there.

**Revisit triggers.** A funded partner (government, key custody, OEM) requiring host-compromise-resistant policy or keys, together with a chosen reference board; or the `protected` tier proving insufficient for live execution at the M5 gate. Either is raised as an RFC with a budget, never started by drift.
