# RFC-001 - Console Cognition Tiers (deferred)

Part of the [PR3 documentation](../README.md).

**Status:** Draft, not scheduled. **Purpose:** the design a studio partner would react to, so that a console conversation does not start from zero.

## 1. Context

Console silicon is sufficient: a jailbroken PS5 has reportedly run llama.cpp on the GPU with a heavily quantized 27B model at roughly 20 tokens/s (unsourced - cite in [the references](../related-work.md#references), and omit from any copy shared with platform holders). On retail SDKs, however, there is no GPU inference backend (PS5 AGC, Xbox DX12, Switch 2 NVN have no ggml backend), so a shipping title gets CPU-only inference, and PR3 is a guest in the title's memory and frame budget.

Approximate budgets available to titles (as reported publicly, October 2026): Xbox Series X ≈13.5 GB and Series S ≈8 GB; PS5 ≈12–13 GB; Switch 2 ≈9 GB with 6 CPU cores available to developers. A 1–1.5B Q4 model with a modest KV cache needs ≈1.3 GB; 3B needs ≈2.5 GB.

## 2. Tiers

| Tier | Where | Does | Why |
|---|---|---|---|
| C0 | Game code | State, rules, what an NPC *can* do | Deterministic, cert-safe |
| C1 | On-console 0.5–1.5B, CPU, 2 pinned cores, grammar-constrained | Intent, barks, short reactions, local memory summaries | <300 ms, works offline, no server cost per utterance |
| C2 | Private server (publisher's, or the player's paired PR3 node) 8B–32B | Long dialogue, planning, director decisions, consolidation | Quality; one model serves many players |

C2 is the default brain; C1 is an accelerator for low-stakes cognition. The on-console model never removes the server. (Console tiers are prefixed `C` to keep them distinct from Rust target tiers and the trust tiers of [Sec. 7.3](../security.md#73-trust-tiers---dev-protected-secure).)

## 3. Host budget contract

Declared by the title at init: memory ceiling, pinned cores, per-frame time slice, model load/unload hooks (reclaim for cutscenes, reload after), suspend/resume behavior, output filter and deterministic fallback for every LLM-driven line.

## 4. Integration shape

- **Phase A (available now):** `pr3-client-c` compiled under the console SDK, speaking `pr3-protocol` to a C2 node. No Rust on the console.
- **Phase B (partner-funded):** the `pr3-ffi` core build (`pr3-core` as a `no_std + alloc` staticlib) via custom target spec and `-Zbuild-std=core,alloc` (nightly, partner SDK, private CI), driven by the game's C++ through the C ABI; ggml CPU backend built with the console toolchain and supplied as the inference callback. Order: Xbox (GDK is closest to a stock Win32/Rust target) → PS5 → Switch 2.
- **Never in Phase B:** `std` in core, Tokio, sockets or files inside PR3 on the console; threads beyond the host's pinned cores.

## 5. Non-engineering gates

Devkit/SDK access, TRC/XR/Lotcheck review, platform policy on cross-title user data (relevant to the "game as client of the player's PR3 node" idea), package size on Switch 2 game cards.

## 6. Trigger to schedule this work

Any one of: a studio with SDK access asks and co-funds the port; a next-generation platform ships an on-device ML/LLM API usable by third-party titles; or a devkit and idle time make a ggml-CPU + `no_std` core experiment on Xbox cheap enough to produce a real datapoint. Absent these, the only console work is keeping `pr3-core` `no_std`-clean and `pr3-client-c` building.
