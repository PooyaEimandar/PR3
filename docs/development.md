# Development

Part of the [PR3 documentation](README.md). How to build, run, configure and test PR3 today.

## Prerequisites

- **Rust.** rustup installs the pinned toolchain from `rust-toolchain.toml` on first use.
- **Tauri CLI 2:** `cargo install tauri-cli --version "^2"`.
- **Python 3.11 or newer**, for `scripts/fetch-model.sh`; 3.12 for the AgentDojo harness.
- **iOS and macOS:** Xcode. **Android:** the Android SDK with NDK 27, and JDK 21.

## Models

The model is chosen per device class in [`config/pr3.toml`](../config/pr3.toml): Qwen3.8-27B on computers with 30 GB of memory or more (Qwen3.5-4B below that), Qwen3.5-4B on phones with 7 GB or more and Qwen3.5-2B on smaller phones. Every model is pinned to a repository revision and SHA-256.

```bash
scripts/fetch-model.sh            # the models config/pr3.toml selects, into models/
scripts/fetch-model.sh --list     # every model it knows
scripts/fetch-model.sh qwen3.8-27b-q8
```

To run another model, change the ids under `[model]`. To change anything without rebuilding, put an edited copy of `pr3.toml` in the app's data folder (on macOS `~/Library/Application Support/ai.pooya.pr3/`). `PR3_MODEL`, a path to a `.gguf` file, overrides the choice.

## Build and run

- **macOS:** `cd apps/pr3-app && cargo tauri dev`, or `cargo tauri build --debug --bundles app` for an app bundle.
- **iOS simulator:** `cd apps/pr3-app && cargo tauri ios build --debug --target aarch64-sim`, then `xcrun simctl install booted src-tauri/gen/apple/build/arm64-sim/PR3.app`. Debug builds read models from the repository's `models/` folder.
- **Android:** `scripts/build-android.sh` builds a debug APK for arm64 (it sets the ARMv8.2 dot-product flag llama.cpp needs). Install it with `adb install -r`, then copy the phone model into the app's data folder:

  ```bash
  adb exec-in run-as ai.pooya.pr3 sh -c 'mkdir -p models && cat > models/Qwen3.5-2B-Q4_K_M.gguf' < models/Qwen3.5-2B-Q4_K_M.gguf
  ```

llama.cpp's CMake step does not reconfigure an existing build: after changing a `GGML_*` flag, run `cargo clean -p llama-cpp-sys-2 --target <target>`.

## Configuration

Everything lives in [`config/pr3.toml`](../config/pr3.toml):

| Section | What it sets |
|---|---|
| `[model]`, `[models.*]` | The model per device class, and the pinned models PR3 knows |
| `[traces]` | OpenJarvis-format traces of every turn; off by default, rotated at `max_mb` |
| `[actions]` | The kill switch, limits and sandbox address book for consequential actions ([Sec. 4.6](architecture.md#46-structured-intent-and-execution-sequence)) |
| `[node]` | Whether a computer lends its model to paired phones, and where it listens ([Sec. 6.3](deployment.md#63-private-hybrid-mobile--desktop)) |

## Sending email (sandbox)

PR3 can send email through a sandbox connector: messages land in `outbox.jsonl` in the app's data folder, never on the internet. Each one waits for your approval on a confirmation card and then for Face ID, Touch ID, a fingerprint or the passcode; the approval key lives in the Secure Enclave or the Android Keystore. On Android, set a screen lock first. Every step is recorded in a hash-chained `audit.jsonl` that is verified at each start.

## Voice notes

Hold the talk button beside the message field, speak and let go; or tap it, speak and tap again. No Send button is needed. In the header, **Send on pause** (on by default) sends a tapped note once you stop talking for two seconds, and **Speak replies** reads replies aloud; both are remembered. Recognition and voices run on the device, and the first voice note asks for the microphone and speech recognition.

- **macOS and iOS:** the Speech framework's on-device recognition. If your language has none, turn on Dictation for it (System Settings, Keyboard on a Mac); its on-device model comes with it. Better voices download in the Spoken Content settings. `src-tauri/Info.plist` gives `cargo tauri dev` the usage strings macOS asks for.
- **Android 12 and later:** the on-device recognizer. On first use Android may offer to download the language's speech model (about 60 MB). In the emulator, turn on **Virtual microphone uses host audio input** in the extended controls.
- **Simulator and emulator:** both hear through the Mac's microphone, so Simulator and the emulator need microphone access on the Mac (System Settings, Privacy & Security, Microphone). Without it the emulator records silence and the simulator's audio stalls until iOS ends the app. From the command line, start the emulator with `-allow-host-audio`. Some simulator runtimes ship a damaged on-device speech model ("Failed to initialize recognizer"); test transcription on a Mac or a phone instead.

## Pairing a phone with a computer

1. On the computer, open **Devices** and choose **Pair a phone**; it shows a six-digit code, valid once for two minutes.
2. On the phone, open **Devices** and enter the computer's address and the code.

The computer listens on `127.0.0.1:7313` by default, which reaches the iOS simulator and, as `10.0.2.2:7313`, the Android emulator. For a phone on your network set `listen = "0.0.0.0:7313"` under `[node]`; macOS then asks whether PR3 may accept incoming connections. A paired phone uses the computer's model and answers with its own when the computer cannot be reached. In debug builds, `PR3_PAIR_ON_START=1` prints a pairing code at startup.

## Tests and checks

```bash
cargo test --workspace                          # unit tests
cargo test --workspace --release -- --ignored   # real-model tests (need the models)
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
scripts/check-trusted-side.sh                   # the trusted side imports nothing from the agent side
cargo check -p pr3-core --no-default-features --target thumbv7em-none-eabihf
```

CI ([`.github/workflows/ci.yml`](../.github/workflows/ci.yml)) runs formatting, lints, tests, the `no_std` builds and dependency advisories. Real-model tests stay manual. The rules behind them are in [quality-gates.md](quality-gates.md).

## Benchmarks

The AgentDojo harness in [`benches/agentdojo`](../benches/agentdojo) compares PR3 with a plain tool-calling loop on the same model. Results so far: [Sec. 8.6](benchmarks.md#86-results-so-far).

```bash
cd benches/agentdojo
python3.12 -m venv .venv && .venv/bin/pip install -r requirements.txt
cargo build --release -p pr3-bench-agentdojo
.venv/bin/python run.py --model ../../models/Qwen3.5-4B-Q4_K_M.gguf --suite banking --modes pr3 plain confirm_all
.venv/bin/python run.py --summary results/banking-Qwen3.5-4B-Q4_K_M.jsonl
.venv/bin/python -m unittest discover tests
```

A run resumes where it stopped. Expect about 10–70 seconds per task, depending on the model.

## Repository layout

```text
crates/pr3-core           sans-I/O, no_std core: the agent loop and the trusted side (intent, policy, receipts, capabilities, audit)
crates/pr3-protocol       versioned wire contract between a phone and its node (no_std types, CBOR)
crates/pr3-hybrid         pairing and encrypted sessions between a phone and the user's computer
crates/pr3-llamacpp       in-process llama.cpp backend with Qwen prompt formats and prompt-cache reuse
crates/pr3-memory         user memory with provenance: no_std types and the store interface
crates/pr3-memory-sqlite  SQLite/FTS5 memory store
crates/pr3-tools          built-in tools (calculator, clock, memory) and the sandbox mail connector
crates/pr3-speech         on-device speech: push-to-talk recognition and spoken replies through the platform's engines
crates/pr3-vault          the device's approval key: Secure Enclave, Android Keystore or a software key
apps/pr3-app              the Tauri 2 app for macOS, iOS and Android
benches/agentdojo         AgentDojo harness (benchmark only, never shipped)
config/pr3.toml           models, traces, actions and node settings
scripts/                  model download, Android build, trusted-side check
```

Crates the roadmap plans but the spike has not needed yet - `pr3-inference`, `pr3-ffi`, `pr3-client-c`, `pr3-node` - are described in [Sec. 4.2](architecture.md#42-rust-workspace-host-api-c-abi-and-protocol-client).
