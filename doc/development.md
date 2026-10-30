# Development

Part of the [PR3 documentation](README.md). How to build, run, configure and test PR3 today.

## Prerequisites

- **Rust.** rustup installs the pinned toolchain from `rust-toolchain.toml` on first use.
- **Tauri CLI 2:** `cargo install tauri-cli --version "^2"`.
- **Node.js 24 or newer**, for the app's TypeScript: run `npm ci` in `app/pr3-app` once. It installs the pinned compiler (TypeScript 7) and the Tauri API's types; nothing from npm ships in the app.
- **Python 3.11 or newer**, for `script/fetch-model.sh`; 3.12 for the AgentDojo harness.
- **iOS and macOS:** Xcode. **Android:** the Android SDK with NDK 27, and JDK 21.

## Models

The model is chosen per device class in [`config/pr3.toml`](../config/pr3.toml): Qwen3.8-27B on computers with 30 GB of memory or more (Qwen3.5-4B below that), Qwen3.5-4B on phones with 7 GB or more and Qwen3.5-2B on smaller phones. Every model is pinned to a repository revision and SHA-256.

**In the app.** No build carries a model. Open Settings (the gear at the bottom left), then **Models**: it lists the models PR3 knows with their sizes, the one recommended for this device and the one in use.

- **Download** fetches a model over HTTPS from its pinned Hugging Face revision and shows its progress. **Cancel**, a lost connection or closing the app keeps what has arrived, and **Resume** carries on from there.
- On a phone, a download waits for Wi-Fi, and one under way pauses when the phone moves to mobile data, then goes on by itself once Wi-Fi is back. **Download over mobile data**, at the top of the tab, lets downloads use mobile data instead; the choice is remembered. Low Data Mode on an iPhone, and a personal hotspot, count as mobile data. Computers download on any network. The file is named, and can be used, only once its SHA-256 matches the pin; a download that does not match is thrown away.
- The first model downloaded while none is running is used at once. **Use** switches to another, and the choice is remembered.
- **Delete** removes a model and **Discard** an unfinished download; both take a second tap.
- A model that would take more than half of a phone's memory, or three quarters of a computer's, is marked too large and cannot be downloaded or used.

Models live in the app's data folder, under `model/`, apart from the app itself: installing a new version keeps them, phone backups leave them out, and on a phone uninstalling PR3 removes them.

| Platform | Folder |
|---|---|
| macOS | `~/Library/Application Support/ai.pooya.pr3/model/` |
| Windows | `%APPDATA%\ai.pooya.pr3\model\` |
| Linux | `~/.local/share/ai.pooya.pr3/model/` |
| iOS | `Library/Application Support/ai.pooya.pr3/model/` in the app's container |
| Android | `/data/user/0/ai.pooya.pr3/model/` |

**From the command line.** `script/fetch-model.sh` puts models in the repository's `model/` folder, where debug builds on the Mac and in the iOS simulator also look:

```bash
script/fetch-model.sh            # the models config/pr3.toml selects, into model/
script/fetch-model.sh --list     # every model it knows
script/fetch-model.sh qwen3.8-27b-q8
```

To run another model, change the ids under `[model]`. To change anything without rebuilding, put an edited copy of `pr3.toml` in the app's data folder (on macOS `~/Library/Application Support/ai.pooya.pr3/`). `PR3_MODEL`, a path to a `.gguf` file, overrides every choice; Settings then still downloads and deletes models, but cannot switch.

## Build and run

- **macOS:** `cd app/pr3-app && cargo tauri dev`, or `cargo tauri build --debug --bundles app` for an app bundle.
- **iOS simulator:** `cd app/pr3-app && cargo tauri ios build --debug --target aarch64-sim`, then `xcrun simctl install booted src-tauri/gen/apple/build/arm64-sim/PR3.app`. Debug builds also find models in the repository's `model/` folder.
- **Android:** `script/build-android.sh` builds a debug APK for arm64 (it sets the ARMv8.2 dot-product flag llama.cpp needs). Install it with `adb install -r`, then download the phone model in Settings, or copy it in from the repository, which is quicker:

  ```bash
  adb exec-in run-as ai.pooya.pr3 sh -c 'mkdir -p model && cat > model/Qwen3.5-2B-Q4_K_M.gguf' < model/Qwen3.5-2B-Q4_K_M.gguf
  ```

llama.cpp's CMake step does not reconfigure an existing build: after changing a `GGML_*` flag, run `cargo clean -p llama-cpp-sys-2 --target <target>`.

The webview's script is TypeScript, in `app/pr3-app/src`, checked in strict mode. The Tauri CLI compiles it into `ui/main.js` before every build, and `cargo tauri dev` recompiles it as you edit; `npm run check` in `app/pr3-app` only type-checks it. The source keeps to types a compiler can simply erase, so tools that strip types without checking them (esbuild, swc, Node, Deno, Bun) build the same script.

## Dev builds

[`.github/workflows/app.yml`](../.github/workflows/app.yml) builds the app for every platform on every push and pull request:

| File | Platform | To install |
|---|---|---|
| `pr3-<version>-aarch64-ios-simulator.zip` | iOS simulator on an Apple Silicon Mac | unzip it, then `xcrun simctl install booted PR3.app` |
| `pr3-<version>-aarch64.apk` | Android, arm64 | `adb install -r`, or open it on the phone |
| `pr3-<version>-aarch64.dmg` | macOS, Apple Silicon | drag PR3 to Applications. It is signed ad hoc, so macOS blocks the first launch: allow it under System Settings, Privacy & Security, **Open Anyway** |
| `pr3-<version>-x86_64.exe` | Windows x64 | run the installer. It is unsigned, so Windows asks first: **More info**, **Run anyway** |
| `pr3-<version>-x86_64.AppImage`, `pr3-<version>-x86_64.deb` | Linux x64 | `chmod +x` the AppImage and run it, or `sudo apt install ./pr3-<version>-x86_64.deb` |

- **Each push to `main`:** the files stay 7 days among the run's artifacts (Actions, the run, Artifacts; GitHub asks you to sign in to download). The APK is signed with the fixed dev key, so each build installs over the last and keeps its data, and every file is attested: `gh attestation verify <file> --repo PooyaEimandar/PR3` checks that this workflow built it from that commit.
- **A pull request:** the same files for 7 days, but the APK carries the runner's throwaway debug key (`…-debug-key.apk`) and nothing is attested.
- **A release:** Actions, **Release**, **Run workflow** from `main`, with nothing to fill in. [`release.yml`](../.github/workflows/release.yml) tags `main`'s newest commit with that commit's own time, builds every platform from it, and, once all of them have built, publishes them with `SHA256SUMS` as a permanent release on the [Releases](https://github.com/PooyaEimandar/PR3/releases) page. If a build fails, the tag stays without a release: rerun the failed jobs, or run Release again, which reuses the tag. `script/version.py --tag` shows the tag a commit would get.

**Versions.** Every version comes from the time of the commit a build is made from ([`script/version.py`](../script/version.py)), so a later commit always has a larger one. A release is tagged `vYYYY.MM.DD.SSSSS`, the commit's UTC day and its seconds since midnight (00000-86399). For a commit `fd9a287…` made on 27 October 2026 at 08:23:04 UTC, released as `v2026.10.27.30184`:

| Platform | Version | Build number |
|---|---|---|
| Android, dev build | `2026.10.27.30184-dev+fd9a2875dc06` | `25863784` |
| Android, release | `2026.10.27.30184` | `25863784` |
| iOS and macOS | `2026.10.27` | `25863784` |
| Windows and Linux | `2026.300.15092` (Windows shows `2026.300.15092.0`) | - |

- **Dev builds** add `-dev+` and the commit's first 12 characters to the name, so a build names the commit it came from. The app's header shows the name; a local build says *local build*.
- **The build number** is the seconds since 1 January 2026 UTC: Android's version code and Apple's `CFBundleVersion`. It stops at 2,100,000,000, Google Play's ceiling, about 66 years on.
- **iOS and macOS** show at most three numbers, so the version is the day alone, without leading zeros (`2026.1.5`, not `2026.01.05`); the build number orders builds within a day.
- **Windows** takes four numbers of at most 65535 each: the year, the day of the year, half the seconds since midnight, and 0. The Linux packages carry the first three.
- **Newer, by at least two seconds.** Two commits that close together share Windows' half-second, and a commit whose clock ran behind gets a smaller version than the one before it. So Release refuses a commit less than two seconds after the newest release, and a build of `main` less than two seconds after the previous build of `main` (the commit its push replaced) warns that its APK will not install over that build without uninstalling it first; the next commit sets it right. `script/version.py --after <tag or commit>` runs the same check.

All builds are the `dev` channel ([quality-gates.md](quality-gates.md)): no security claims yet. The iOS simulator and Android builds are debug builds; the Android one is debuggable, so the `adb` command above can put a model in its data folder. No build carries a model: download one in Settings, **Models** ([Models](#models)). Until one is there, the app answers with a stand-in that names the file to download.

llama.cpp in the Windows and Linux builds uses AVX2 and FMA, which x86 processors have had since 2013; left alone it would build for plain SSE2, several times slower. The Linux build runs on Ubuntu 22.04 so that it also runs on distributions as old as that.

**Signing setup (once, by the repository owner).** The dev key that signs the APK lives only in the `app-dev` environment's secrets, which only the signing job reads. That job checks out nothing and runs nothing the build made, and takes the APK only if it matches the digest the build job reported. Make the key once and keep the file somewhere safe: losing it means every phone has to uninstall PR3 before the next build will install.

```bash
keytool -genkeypair -keystore pr3-dev.p12 -storetype PKCS12 -alias pr3-dev -keyalg RSA -keysize 4096 -validity 10000 -dname "CN=PR3 dev"
gh api -X PUT repos/PooyaEimandar/PR3/environments/app-dev -F 'deployment_branch_policy[protected_branches]=false' -F 'deployment_branch_policy[custom_branch_policies]=true'
gh api -X POST repos/PooyaEimandar/PR3/environments/app-dev/deployment-branch-policies -f name=main -f type=branch
base64 -i pr3-dev.p12 | gh secret set ANDROID_DEV_KEYSTORE --env app-dev --repo PooyaEimandar/PR3
gh secret set ANDROID_DEV_KEYSTORE_PASSWORD --env app-dev --repo PooyaEimandar/PR3
```

The environment's branch rule lets only `main` use the key; releases run from `main` too. Until the secrets exist, the signing job fails with a message that points here, and the other platforms are still built and attested.

## Configuration

Everything lives in [`config/pr3.toml`](../config/pr3.toml):

| Section | What it sets |
|---|---|
| `[model]`, `[models.*]` | The model per device class, and the pinned models PR3 knows |
| `[traces]` | OpenJarvis-format traces of every turn; off by default, rotated at `max_mb` |
| `[tasks]` | `keep_days`: how long finished tasks are kept, 30 by default ([Tasks](#tasks)) |
| `[actions]` | The kill switch, limits and sandbox address book for consequential actions ([Sec. 4.6](architecture.md#46-structured-intent-and-execution-sequence)) |
| `[node]` | Whether a computer lends its model to paired phones, and where it listens ([Sec. 6.3](deployment.md#63-private-hybrid-mobile--desktop)) |

## Sending email (sandbox)

PR3 can send email through a sandbox connector: messages land in `outbox.jsonl` in the app's data folder, never on the internet. Each one waits for your approval on a confirmation card and then for Face ID, Touch ID, a fingerprint or the passcode; the approval key lives in the Secure Enclave or the Android Keystore. On Android, set a screen lock first. Every step is recorded in a hash-chained `audit.jsonl` that is verified at each start.

## Tasks

PR3 groups the conversation into tasks ([Sec. 4.8](architecture.md#48-tasks-and-conversation-continuity)). They are kept in `memory.sqlite` with their chats, so they survive closing the app.

- A message continues the current task while PR3 is answering it or within 10 minutes of its last answer; after that, the next message opens a new task. **New topic**, under the conversation, starts one at once, and typing or saying "new topic" on its own does the same. The conversation it replaces folds into a line at the top, which opens that task.
- **Tasks**, beside Settings, opens three pages, **To Do**, **Doing** and **Done**: swipe between them or tap a name. To Do holds tasks waiting for your OK and ones that did not finish, Doing the task the conversation is on, and Done the rest. Tap a task to read its chat; **Back** returns to the list, and **Tasks** to the conversation.
- **Continue** brings an unfinished task back into the conversation, and **Follow up** starts a new task linked to a finished one. **Delete** takes a second tap and also erases what the runtime holds for that task. All three wait while PR3 is answering.
- When PR3 opens again, the conversation comes back if its 10 minutes have not run out, and a follow-up picks up where it left off. A confirmation card that was waiting is shown again, to approve afresh; if it ran out while PR3 was closed, nothing was sent and the task is on Done as expired. A turn cut off by closing is on To Do as not finished.
- Say or type, as the whole message: "what is still open?" to list To Do; "carry on with the email to Alex" (or "back to", "resume", "pick up") to go back to that task, or to follow it up if it is finished; "what happened with the trip?" to see where it stands; "export the trip" to export it. PR3 matches the words against each task's title and first message, and offers a choice when several unfinished tasks match. Words that name no task go to the model as usual.
- **Export**, on a task, saves its chat as Markdown and its record and hash-chained events as JSON: through the share sheet on iOS and Android, and a save dialog on a computer.
- A finished task is erased 30 days after it last changed; `keep_days` under `[tasks]` sets how long, and 0 keeps finished tasks only until the app closes. Unfinished ones stay until they finish.

## Voice notes

Hold the microphone button, between the message field and Send, speak and let go; or tap it and speak, and the note is sent once you stop talking for two seconds (or at a second tap). No Send button is needed. Settings, **Voice**, picks the language voice notes and spoken replies use: *System language* follows the system, and the list holds the languages this device recognizes on the device (on Android 13 and later also those it downloads on first use); the choice is remembered. **Speak replies**, on the same tab, reads replies aloud; it is remembered too. Recognition and voices run on the device, and the first voice note asks for the microphone and speech recognition.

- **macOS and iOS:** the Speech framework's on-device recognition. If your language has none, turn on Dictation for it (System Settings, Keyboard on a Mac); its on-device model comes with it. Better voices download in the Spoken Content settings. `src-tauri/Info.plist` gives `cargo tauri dev` the usage strings macOS asks for.
- **Android 12 and later:** the on-device recognizer. On first use Android may offer to download the language's speech model (about 60 MB). In the emulator, turn on **Virtual microphone uses host audio input** in the extended controls.
- **Simulator and emulator:** both hear through the Mac's microphone, so Simulator and the emulator need microphone access on the Mac (System Settings, Privacy & Security, Microphone). Without it the emulator records silence and the simulator's audio stalls until iOS ends the app. From the command line, start the emulator with `-allow-host-audio`. Some simulator runtimes ship a damaged on-device speech model ("Failed to initialize recognizer"); test transcription on a Mac or a phone instead.

## Memory

Settings, **Memory**, shows what PR3 remembers. Memories the model proposed from your words wait under *To review*: confirm them, edit them (the corrected version replaces them) or forget them, which takes a second tap and also erases the words they came from. You can add memories yourself; those are confirmed from the start. Everything is in `memory.sqlite` in the app's data folder and never leaves the device.

## Pairing a phone with a computer

1. On the computer, open Settings, **Devices**, and choose **Pair a phone**; it shows a six-digit code, valid once for two minutes.
2. On the phone, open Settings, **Devices**, and enter the computer's address and the code.

The computer listens on `127.0.0.1:7313` by default, which reaches the iOS simulator and, as `10.0.2.2:7313`, the Android emulator. For a phone on your network set `listen = "0.0.0.0:7313"` under `[node]`; macOS then asks whether PR3 may accept incoming connections. A paired phone uses the computer's model and answers with its own when the computer cannot be reached. In debug builds, `PR3_PAIR_ON_START=1` prints a pairing code at startup.

## Tests and checks

```bash
cargo test --workspace                          # unit tests
cargo test --workspace --release -- --ignored   # real-model tests (need the models)
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
script/check-trusted-side.sh                   # the trusted side imports nothing from the agent side
cargo check -p pr3-core --no-default-features --target thumbv7em-none-eabihf
```

Fuzzing and Miri use a separately pinned nightly (`fuzz/rust-toolchain.toml`), never product builds. Install cargo-fuzz once with `cargo +nightly-2026-08-29 install cargo-fuzz`, then:

```bash
script/fuzz.sh 60                                                  # every fuzz target for a minute
script/fuzz.sh 900 trusted_gate                                    # one target for longer
PROPTEST_CASES=4 MIRIFLAGS=-Zmiri-disable-isolation \
  cargo +nightly-2026-08-29 miri test -p pr3-core                   # undefined behaviour in the core
```

The fuzz targets in [`fuzz/fuzz_target`](../fuzz/fuzz_target) cover what reads untrusted input: the core's agent loop under arbitrary model replies and tool results, the trusted gate under forged and altered receipts, phone↔computer messages, the model-output parser, and memory search. A failing input is saved in `fuzz/artifact/<target>/`; `cargo +nightly-2026-08-29 fuzz run <target> <file>` replays it.

CI ([`.github/workflows/ci.yml`](../.github/workflows/ci.yml)) builds and tests the workspace on macOS, Linux and Windows, and runs formatting, lints, the `no_std` builds, dependency advisories and a minute of fuzzing per target on every change; Miri on `pr3-core` (about an hour) runs on pushes to `main` and weekly. On Linux, Tauri needs `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev`, and llama.cpp needs CMake and clang. Real-model tests stay manual. The app itself is built for every platform by `app.yml`, and released by `release.yml` ([Dev builds](#dev-builds)). The rules behind them are in [quality-gates.md](quality-gates.md).

## Benchmarks

The AgentDojo harness in [`bench/agentdojo`](../bench/agentdojo) compares PR3 with a plain tool-calling loop on the same model. Results so far: [Sec. 8.6](benchmarks.md#86-results-so-far).

```bash
cd bench/agentdojo
python3.12 -m venv .venv && .venv/bin/pip install -r requirements.txt
cargo build --release -p pr3-bench-agentdojo
.venv/bin/python run.py --model ../../model/Qwen3.5-4B-Q4_K_M.gguf --suite banking --modes pr3 plain confirm_all
.venv/bin/python run.py --summary result/banking-Qwen3.5-4B-Q4_K_M.jsonl
.venv/bin/python report.py                                   # every suite's tables, as in Sec. 8.6
.venv/bin/python -m unittest discover test
```

A run resumes where it stopped. Expect about 10–70 seconds per task, depending on the model.

The task suite in [`bench/task`](../bench/task) measures what a task keeps across app launches, changes of topic and confirmation cards (Sec. 4.8). It runs the app's own host on a model, each launch in its own process, through the made-up cases in `corpus.toml`, and checks PR3's answers and emails for words the task said earlier and for words only another task said or the user took out. Results so far: [Sec. 8.6](benchmarks.md#86-results-so-far).

```bash
cargo build --release -p pr3-bench-task
./target/release/pr3-bench-task run --model model/Qwen3.5-4B-Q4_K_M.gguf     # --case restart/trip for one case
./target/release/pr3-bench-task summary bench/task/result/*.jsonl --update doc/benchmarks.md
```

A run resumes where it stopped, at one sampling seed (`--seed`, 7 by default). Expect a few seconds per message with the 4B model.

## Repository layout

```text
crate/pr3-core           sans-I/O, no_std core: the agent loop and the trusted side (intent, policy, receipts, capabilities, audit)
crate/pr3-protocol       versioned wire contract between a phone and its node (no_std types, CBOR)
crate/pr3-hybrid         pairing and encrypted sessions between a phone and the user's computer
crate/pr3-llamacpp       in-process llama.cpp backend with Qwen prompt formats and prompt-cache reuse
crate/pr3-memory         user memory with provenance: no_std types and the store interface
crate/pr3-memory-sqlite  SQLite/FTS5 memory store
crate/pr3-tool          built-in tools (calculator, clock, memory) and the sandbox mail connector
crate/pr3-speech         on-device speech: push-to-talk recognition and spoken replies through the platform's engines
crate/pr3-vault          the device's approval key: Secure Enclave, Android Keystore or a software key
fuzz                     fuzz targets for the parsers of untrusted input (pinned nightly, cargo-fuzz)
app/pr3-app              the Tauri 2 app for macOS, iOS and Android: Rust in src-tauri, the webview's TypeScript in src, its page in ui
bench/agentdojo         AgentDojo harness (benchmark only, never shipped)
bench/task              tasks that span app launches, on the app's own host (benchmark only, never shipped)
config/pr3.toml           models, traces, actions and node settings
script/                  model download, Android build, trusted-side check
```

Crates the roadmap plans but the spike has not needed yet - `pr3-inference`, `pr3-ffi`, `pr3-client-c`, `pr3-node` - are described in [Sec. 4.2](architecture.md#42-rust-workspace-host-api-c-abi-and-protocol-client).
