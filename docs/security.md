# Security, privacy and trust

Part of the [PR3 documentation](README.md). Section numbers are stable; code comments cite them as “docs Sec. N”.

## 7. Security, Privacy & Trust Model

### 7.1 Threat model

Assume any of the following may be compromised or hostile: the model (local, platform or node) and its provider; prompts, documents, emails, web pages and audio heard by the microphone; retrieved memory; tool and connector outputs; a plugin; a protocol-client host (a game, an MCU); the paired node; the publisher's push relay ([Sec. 6.3](deployment.md#63-private-hybrid-mobile--desktop)); the host OS or kernel; the network; peripheral firmware; a generated script; and the app's own UI layer when spoofing is attempted.

Protect against: prompt injection through any of those inputs, exfiltration through tool arguments, replayed or forged approvals, impersonation through a stolen personal model or voice, leaked OAuth tokens, insecure pairing, malformed protocol frames, model hallucination presented as fact, logging of personal data, corrupted or poisoned local memory, UI spoofing of a confirmation, and unauthorized message, payment or other consequential execution.

**Invariants the design must preserve** (release gates, tested adversarially - [Sec. 12](quality-gates.md#12-build-cicd--quality-gates)):

- Private keys cannot be exported from the vault; the model never sees a credential.
- High-risk actions cannot bypass policy; the final authorization decision is never made by an LLM.
- Audit history cannot be silently rewritten.
- A model, plugin or learning policy cannot grant itself additional authority.
- A compromised plugin or host cannot inherit another's capabilities.
- Provider and connector tokens are scoped, short-lived and replaceable.
- The user can revoke any agent, device, host, plugin or provider.
- Consequential actions require a trusted confirmation path that a compromised node or guest UI cannot forge.
- Voice is never authentication; a spoken "yes" still goes through the trusted confirmation path.
- Nothing is sent, said or signed in the user's name without confirmation or a pre-approved scope.
- The personal model and any voice or face data never leave the user's devices unencrypted.

### 7.2 Required controls

1. **Capabilities, not broad access:** read-mail, create-draft, send-mail, create-calendar-event and share-file are distinct grants. Protocol clients receive only the scopes their host was granted.
2. **Secrets outside model context:** OAuth tokens, API keys and signing keys held by protected storage or a dedicated credential-broker process, never placed in prompts, never carried by `pr3-client-c`.
3. **Sandbox untrusted tools:** OS-specific enforcement, not merely a model instruction; mobile apps remain constrained by native app sandbox.
4. **Network egress policy:** deny external model endpoints by default; allow only explicitly selected service destinations. Platform-model backends are local, but their vendors' telemetry is measured and disclosed.
5. **Approval receipts and one-use capabilities:** an approval binds principal, actor, operation, object, exact parameters, expiry, nonce and target provider; it is signed by a key in the platform keystore after biometric/OS authentication; `pr3-vault` exchanges it for a capability with the same binding:

   ```text
   Capability:
     actor:             agent:mail-assistant
     principal:         user:…
     operation:         mail.send
     object:            to:alex@example.com / draft:sha256:4c1e… / attachments:sha256:9b07…
     provider:          mail:sandbox          (or the user's mail provider)
     receipt_hash:      8a3f…
     issued_at/expires: now / +90 s
     max_uses:          1
   ```

   The capability - not a credential - is what the execution adapter receives. Field names are compared with ZeroClaw's tool-receipt format before the schema is frozen ([Sec. 5.1](related-work.md#51-zeroclaw-reuse-candidates-at-the-adapter-layer)).
6. **Idempotency:** unique action keys and reconciliation against provider execution status; do not re-execute actions after retry/disconnection.
7. **Data handling:** local encryption, redacted logs, retention and deletion semantics, import/export, secure device removal; deletion of all physical backup remnants cannot be guaranteed without backup control.
8. **Supply chain:** pinned dependency versions and vendored llama.cpp commit, SBOM, signature verification, reproducible builds where feasible, periodic third-party review.
9. **Memory safety as a measured property, not a slogan:** `#![forbid(unsafe_code)]` on every crate except `pr3-llamacpp`, `pr3-ffi` and the `sys` modules of `pr3-platform-llm`, `pr3-vault` and `pr3-speech`, enforced in CI; cargo-fuzz on every parser (JSON, schema, protocol frames, model templates); Miri on `pr3-core`; panic-site allowlist check on the `no_std` artifact; fallible allocation at untrusted boundaries; ASan and libFuzzer on the audited `unsafe` code, UBSan on the C/C++ it wraps (llama.cpp and whisper.cpp shims) and on `pr3-client-c`; adversarial prompt-injection suites.
10. **Vault discipline:** `pr3-vault` returns results or restricted capabilities, never raw master secrets; signing happens inside platform key storage (Secure Enclave, StrongBox, TPM) wherever the platform offers it.
11. **Secrets never cross a protocol boundary:** `pr3-client-c` carries capabilities and receipts, not credentials; the node holds no approval keys.

Rust is selected for safe ownership and concurrency guarantees in the core, **not as a substitute for a security review**. `unsafe` code in llama.cpp bindings, GPU runtimes, native plugins, FFI and platform APIs remains a critical attack surface, and the C client is C.

### 7.3 Trust tiers - `dev`, `protected`, `secure`

The propose/dispose split ([Sec. 3](overview.md#3-guiding-engineering-principles)) is a design boundary first and a deployment boundary later. Three tiers share one core and one protocol; a tier changes *where the trusted side runs*, never what it does.

| Tier | Trusted side runs in | Keys and signing | Protects against | When |
|---|---|---|---|---|
| **`dev`** (PR3 Runtime) | The app's own process, as separable `no_std` modules | Platform keystore via `pr3-vault` (Secure Enclave, StrongBox, Keychain/Keystore, TPM) | A misbehaving model, plugin or tool; replay; UI spoofing (phone confirmation) | M1 onward - the product on today's OSes. No claim against a compromised host kernel |
| **`protected`** | A platform-isolated environment: Android AVF protected VM, or a dedicated OS-sandboxed process on desktop | Same, plus attestation of the protected environment where available | A compromised app process or guest userland | Post-M5, only if live execution (payments, irreversible actions) needs it; gated by evidence, not scheduled |
| **`secure`** | Protection domains on a formally verified microkernel, with the app and its OS as an untrusted guest | Keys never leave a native microkernel domain | A compromised guest OS kernel | Preserved option only ([ADR-002](adr/002-trust-architecture.md)). Not funded, not scheduled |

Rules:

- The `no_std + alloc`, sans-I/O, panic-free discipline on the trusted-side modules ([Sec. 4.2](architecture.md#42-rust-workspace-host-api-c-abi-and-protocol-client)) is what makes `protected` and `secure` reachable without a rewrite - the same property that preserves the console path ([RFC-001](rfc/001-console-cognition-tiers.md)).
- Security claims are per tier and per configuration. PR3 may say "critical isolation boundaries are enforced by a formally verified microkernel in supported configurations" only for a shipped `secure` build on a verified configuration, and never that the whole device, VMM, firmware or AI stack is verified. The `dev` tier makes no claim against a compromised OS and must not imply one.
- On ARM64, a verified microkernel and Android's pKVM both occupy EL2; `protected` (AVF/pKVM) and `secure` (the microkernel) are alternative backends, not layers. Nested virtualization is never a dependency.
- No tier work beyond `dev` begins before the M5 gate, and `secure` begins only on an RFC with a funded partner (government, key custody or OEM) and a chosen reference board.
