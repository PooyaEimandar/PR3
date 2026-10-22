# Deployment modes

Part of the [PR3 documentation](README.md). Section numbers are stable; code comments cite them as “docs Sec. N”.

## 6. Supported Deployment Modes

### 6.1 Fully offline mobile

Local model (platform or bundled), local database, local skills/tools and policy enforcement. No cloud LLM. Must support airplane-mode tests for document Q&A, local reminders, summaries of cached data and other permitted tasks. Email sync, web content and external APIs cannot update without connectivity.

Mobile-specific rules: the app is useful before any model download completes; the bundled download is Wi-Fi-only and resumable; the deterministic engine runs before the LLM so battery is spent only on explanations worth giving; proactive/background behavior is **not** promised on the phone (see [Sec. 6.3](#63-private-hybrid-mobile--desktop)).

### 6.2 Fully offline desktop

Same Rust core with a larger local model, longer contexts and optional heavier retrieval/learning features. Local model loading, agent actions and memory must not require an always-running HTTP server. The desktop may additionally run `pr3-node` for other devices.

### 6.3 Private hybrid: mobile + desktop

Mobile performs routine inference locally, delegates selected difficult tasks after a data-transfer policy check, and resumes locally when the node is unavailable. The node executes longer-running workflows and returns provenance-tagged results.

**The hybrid matters more on mobile than anywhere else:** iOS and Android do not allow sustained background inference, so the proactive part of PR3 - watching, summarizing, deciding whether to alert - runs on the always-on node, and the phone wakes on push to render and act. Without a node, proactive features are limited to what OS-scheduled background windows allow, and the product must say so.

**Push needs a publisher-operated relay.** APNs and FCM accept only messages authenticated with the app publisher's credentials, so a self-hosted node cannot wake the phone directly without holding the publisher's key. The hybrid therefore uses a **push relay** run by the app publisher (the pattern Matrix uses with its Sygnal push gateway) or, on Android, UnifiedPush to a distributor the user chooses. The relay carries only an opaque wake-up - a device token and a payload sealed to the phone - never task content. It is a hosted dependency of proactive features only; offline and node-less modes do not use it. It appears in the threat model ([Sec. 7.1](security.md#71-threat-model)).

Use authenticated pairing, certificate/key pinning or mutually authenticated TLS, replay protection, short-lived authorizations and explicit remote cancellation. Zero-knowledge E2EE cannot coexist with the remote node reading plaintext task data; the node must be trusted by the user and authorized to process that subset.

**Identity.** Every device, user, agent and protocol host has an identity with registered public keys, session binding and a revocation state. Grants, approvals and capabilities bind to these identities, so revoking a device, a plugin or a game host invalidates everything issued to it without touching the rest.

**The phone is the trusted confirmation device.** Actions prepared on the node - an outbound message, a calendar change, a payment - are confirmed on the phone: PR3's own UI renders the exact recipients, attachments, amounts and expiry from the *validated intent*, not from anything the node sent for display; the user approves with OS biometrics; the approval receipt is signed with a key `pr3-vault` keeps in the platform keystore. The node never holds an approval key, so a compromised node can prepare but not approve.

That guarantee holds only if the node also cannot *execute* without the phone. Third-party providers - mail, calendar, payment services - check their own OAuth tokens or API keys, not PR3 receipts; a node holding such a token could act with no approval at all. For live execution, the provider credential therefore stays in the phone's platform keystore and the execution adapter runs on the phone, released by the one-use capability - unless a provider itself verifies PR3 receipts. Sandbox connectors are unaffected. Which arrangement applies to each live provider is recorded at the M5 gate together with the `protected`-tier decision ([Sec. 7.3](security.md#73-trust-tiers---dev-protected-secure)).

Phone-side confirmation is the earliest practical answer to the trusted-path problem - a compromised display showing one action while requesting another - and it costs nothing beyond what the hybrid already builds.

### 6.4 OEM / Embedded

Two deliverables, chosen by what the target can run:

- **Rust runtime** for embedded Linux and other Tier 2 Rust targets (Ferrocene where a qualified toolchain is required; QNX where its Rust support applies). Host supplies storage, scheduling and inference; compile-time features remove unnecessary dependencies.
- **`pr3-client-c`** for MCU-class devices, secure elements and TEEs. Nothing LLM-shaped runs there; the TEE's job is key custody and approval-receipt verification, which PR3 deliberately keeps outside itself.

This is **not** a claim that a full LLM can run on every device. Automotive safety-critical controls remain outside LLM authority.

### 6.5 What ships where

| Target | Ships | Inference | Important constraint |
|---|---|---|---|
| Android | Rust runtime via Tauri 2 (Cargo) or Kotlin host (UniFFI / C ABI) | Gemini Nano where available; bundled GGUF CPU-first, Vulkan measured; node | Background execution/power limits; device zoo; Prompt API availability |
| iOS / iPadOS | Rust runtime via Tauri 2 or SwiftUI host (UniFFI / C ABI) | Apple Foundation Models on Apple Intelligence devices; bundled GGUF Metal; node | Sandbox/background restrictions; model-download and signing rules |
| macOS | Rust runtime + optional `pr3-node` | Foundation Models; GGUF Metal/MLX | Apple codesigning / distribution |
| Windows / Linux | Rust runtime + `pr3-node` | GGUF CPU/Vulkan/CUDA | Driver availability; OS sandboxing |
| Embedded Linux / OEM | Rust runtime (Tier 2 targets; Ferrocene if qualified) | Small GGUF CPU, or node | Memory budget; vendor BSPs → host callbacks |
| MCU / secure element / TEE | `pr3-client-c` only | Node | Keys live here; PR3 does not |
| Native games - desktop/mobile | Rust runtime as staticlib via the `pr3-ffi` full build, in-process | GGUF or node | Frame budget; host-declared memory/thread budget |
| Consoles | `pr3-client-c` only (initial). `pr3-ffi` core build via custom target when a partner funds it | Node (console tier C2). In-process deferred | Closed SDKs, platform certification, no public GPU backend; see [Sec. 6.7](#67-game-consoles---deferred-path-preserved), [RFC-001](rfc/001-console-cognition-tiers.md) |
| Web / Unreal HTML5 | `pr3-web` as a separate Wasm module + JS bridge, or `pr3-client-c` compiled by the host's own emsdk | Sibling WebGPU engine (desktop browsers) or node | Single Rust/Emscripten binary with Unreal is out of scope; see [Sec. 6.6](#66-browser-wasm-and-unreal-html5-integration) |

### 6.6 Browser Wasm and Unreal HTML5 integration

**Browser-first scope:** compile agent state, policy, schemas, trace processing and memory *interfaces* to `wasm32-unknown-unknown` (`no_std` keeps the module small). Use JavaScript/browser host bindings for network (`fetch`/WebSocket), persistence (IndexedDB/OPFS through a compatible adapter), timing and task scheduling. Native Tokio/SQLite/llama.cpp adapters are never dependencies of `pr3-web`.

**In-browser inference is a separate proof-of-concept:** the Rust/Wasm agent core delegates to an independently packaged WebGPU inference engine in the page, or to an approved private node. Model memory, WebGPU availability and frame impact must be measured; mobile browsers are node-only.

**Unreal HTML5 - supported path:** `pr3-client-c` compiled by the Unreal fork's *own* pinned emsdk and linked into the game's binary, speaking the protocol over WebSocket to a node. CI carries a job on that exact emsdk version. A separate Rust `.wasm` module with a JS message bridge is the alternative when the client's footprint matters less than keeping Rust in the loop.

**Out of scope:** a single Rust-plus-Unreal Emscripten binary via `wasm32-unknown-emscripten`. LLVM object-format mismatches and pthreads/Asyncify flag conflicts make it fragile regardless of language unless everything is compiled by the same emsdk - which is exactly what the C client achieves.

**WASI:** treat `wasm32-wasip2` as a distinct target with host-supplied capabilities; browser WebGPU and native filesystem behavior do not carry over automatically.

**Optional digital face, later.** A stylized face - falling glyphs that form a head, not a photoreal human - rendered with WebGPU in the Tauri webview, with a WebGL2 fallback where a webview lacks WebGPU. A hidden low-poly head driven by mouth-shape weights sets the glyphs' brightness, so the face speaks in sync with the audio; listening, thinking and speaking map to rain speed and glow. Mouth shapes come from the node's audio-driven model when available and from text-to-speech phoneme timing offline, carried as presence events in `pr3-protocol`. The face only displays: approvals stay on the phone ([Sec. 6.3](#63-private-hybrid-mobile--desktop)).

### 6.7 Game consoles - deferred, path preserved

In-process console inference is technically feasible (the hardware exceeds a flagship phone) but is **not** a 2026 priority: retail SDKs expose no GPU inference backend, so a CPU-only model caps at ~1–1.5B and the server stays mandatory anyway; the dominant costs are SDK access, platform certification and platform relations rather than code; the generation is late-cycle; and in-game AI is publisher-owned by construction, which is off PR3's user-owned thesis.

What PR3 does now, at a cost of roughly a page and one CI job: keep `pr3-core` `no_std`-clean, ship `pr3-client-c`, and maintain [RFC-001](rfc/001-console-cognition-tiers.md) as the design a studio partner would react to. Order of attack when a partner appears: Xbox (GDK is closest to a stock Rust/Win32 target) → PS5 → Switch 2 (ARM, tightest budget).
