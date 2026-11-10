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

To run another model, change the ids under `[model]`. To change anything without rebuilding, put an edited copy of `pr3.toml` in the app's data folder (on macOS `~/Library/Application Support/ai.pooya.pr3/`). A copy that does not read - bad TOML, or a key or table PR3 does not know - is not used: the app runs on its built-in settings with actions turned off and no task erased, and Settings, **Models**, says why. `PR3_MODEL`, a path to a `.gguf` file, overrides every choice; Settings then still downloads and deletes models, but cannot switch.

## Build and run

- **macOS:** `cd app/pr3-app && cargo tauri dev`, or `cargo tauri build --debug --bundles app` for an app bundle.
- **iOS simulator:** `cd app/pr3-app && cargo tauri ios build --debug --target aarch64-sim`, then `xcrun simctl install booted src-tauri/gen/apple/build/arm64-sim/PR3.app`. Debug builds also find models in the repository's `model/` folder.
- **Android:** `script/build-android.sh` builds a debug APK for arm64 (it sets the ARMv8.2 dot-product flag llama.cpp needs). Install it with `adb install -r`, then download the phone model in Settings, or copy it in from the repository, which is quicker:

  ```bash
  adb exec-in run-as ai.pooya.pr3 sh -c 'mkdir -p model && cat > model/Qwen3.5-2B-Q4_K_M.gguf' < model/Qwen3.5-2B-Q4_K_M.gguf
  ```

llama.cpp's CMake step does not reconfigure an existing build: after changing a `GGML_*` flag, run `cargo clean -p llama-cpp-sys-2 --target <target>`.

The Android library is linked for 16 KB memory pages (`-z max-page-size=16384`, in `app/pr3-app/src-tauri/build.rs`), which Google Play requires and without which Android warns on opening the app that it is not 16 KB-compatible; CI checks every library in the APK.

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

- **Dev builds** add `-dev+` and the commit's first 12 characters to the name, so a build names the commit it came from. Android shows the name in the app's info.
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
| `[web]` | Whether PR3 offers to read web pages you link; on by default ([Reading web pages](#reading-web-pages)) |

## Encrypted local storage

The production app uses SQLCipher for `memory.sqlite` and stores its key separately in Apple Keychain or wrapped by Android Keystore. Existing plaintext databases migrate on first launch. Settings → **Storage** shows encryption/lock status and offers an authenticated recovery code; losing a key blocks work until the original database is successfully unlocked. See [storage/recovery design and Android checks](storage-recovery.md). Windows/Linux secure database-key adapters remain a release blocker; there is no plaintext fallback. Synthetic unit/benchmark databases can still use the explicitly unkeyed `SqliteMemory::open` API.

The native suite also exercises encryption, missing-key lockout, wrong-code rejection and recovery through the production UI/IPC. Only that scenario uses a debug-only temporary key-store fixture; it never opens the real platform vault. Other fault-injection scenarios retain synthetic unkeyed databases so the runner can inject SQLite write failures.

## Preparing email

PR3 prepares drafts for your device's email app using opt-in device contacts. Each draft waits for your approval and then Face ID, Touch ID, a fingerprint or the passcode before handoff. The approval key lives in the Secure Enclave or Android Keystore; set a screen lock on Android first. Review and press Send in the email app. PR3 records the handoff in its verified hash-chained `audit.jsonl`, but cannot verify delivery. See [Device contacts and email](#device-contacts-and-email). Benchmark hosts still use the sandbox connector and `outbox.jsonl`, with no external delivery.

Explicit email exclusions are reconstructed from the task's original user messages after restart or summarization, including retained parent tasks chosen through **Follow up**. For example, `Remove the private reference from the email.` resolves a previously supplied field, and `Omit "PRIVATE-821" from the email.` excludes that literal. A recognized standalone correction gets a runtime acknowledgement without inference or email preparation; request the draft when ready. The host checks the resolved recipients, subject and body before showing an approval card, including a restored card. A rejected draft cannot open the email app; a corrected draft still needs fresh approval and is handed off exactly as approved. No text is silently removed after approval.

The deterministic check recognizes bounded, standalone English removal/inclusion commands and quoted literals or labeled values. Matching ignores letter case and whitespace differences; it is not general semantic redaction or a multilingual data-loss-prevention system. Unresolved recognized exclusions ask for exact quoted text rather than allowing an unchecked draft. Other wording still depends on model interpretation and your review. Replies, summaries and tool output cannot revoke restrictions. **New task** starts without inherited restrictions; deleting task history removes that source. Explicit `Include "PRIVATE-821" in the email.` changes that task's restriction, subject to normal approval. Removing a detail from an email does not delete it from local task history or independently saved memory.

## Reading web pages

Paste a link and ask about it, and PR3 reads that page: over HTTPS only, at most 2 MB in 20 seconds, as text, in parts of 3,000 bytes. It reads only links you wrote in the task, never one the model makes up or finds on a page, and refuses addresses on your own network. A page is text from outside your control, so in a turn that read one PR3 asks for nothing that acts, such as sending an email: ask again, and the card names the sites the conversation has read. Settings, **Web**, turns it off and on again, and `[web] enabled = false` leaves the tool out altogether. The design is [RFC-002](rfc/002-reading-web-pages.md).

## Tasks

PR3 groups the conversation into tasks ([Sec. 4.8](architecture.md#48-tasks-and-conversation-continuity)). They are kept in `memory.sqlite` with their chats, so they survive closing the app.

- A message continues the current task while PR3 is answering it or within 10 minutes of its last answer; after that, the next message opens a new task. Typing or saying "new topic", "new task" or "new job" on its own, in any case, starts one at once. The conversation it replaces folds into a line at the top, which opens that task.
- When a message within those 10 minutes seems to start something new - a question or a request of its own, none of whose words are in the task's title, first message or last exchange, and that does not point back ("send it to Alex instead") - PR3 answers it, then asks under the answer "Looks like a new task - start it on its own?". **Start new task** moves the message and its answer into a task of their own, and the conversation goes on there; **Keep here** leaves them in the task.
- **Tasks**, beside Settings, opens three pages, **To Do**, **Doing** and **Done**: swipe between them or tap a name. To Do holds tasks waiting for your OK and ones that did not finish, Doing the task the conversation is on, and Done the rest. Tap a task to read its chat, which opens at its end; **Back**, the task's title and **More** (three dots) stay at the top while it scrolls. **Back** returns to the list, and **Tasks** to the conversation.
- Each message shows its time at its bottom right, and a line above the first message of each day names it: "Today", "24 October", or "15 November 2024" in another year; so does the conversation.
- **More** holds what to do with the task. **Continue** brings an unfinished task back into the conversation, and **Follow up** starts a new task linked to a finished one. **Delete** asks "Delete task?" first, and also erases what the runtime holds for that task. **Finish task** moves it to Done as **Finished by you**, keeps its chat and clears any approval left by an earlier launch. Finishing does not execute pending work. These choices wait while PR3 is answering; answer any live approval first.
- A follow-up starts from a summary of the task it follows, which the model writes as soon as you choose **Follow up** or say "carry on with" a finished task, if that task has none yet, and then reads, so that on a phone the follow-up's first answer starts within about a second. A long task is summed up too: once what the model would read again passes about 1,800 tokens, its older exchanges are folded into the summary right after PR3 answers, while you read the answer, and the last two stay as they were. On a phone with the 4B a summary takes about 15 seconds for a short task and a fold, or a long task's summary, a minute or more; a message sent before it is written waits for the rest. After every answer PR3 also reads the conversation ahead of your next message, so on that phone the next answer starts within about two seconds, unless the model looks something up first. Forgetting a memory in Settings erases every summary; each is written again from its task's chat when next needed.
- **New task** saves an empty conversation selection before clearing the screen. It stays empty across restarts until you send a message or explicitly return to a task; old tasks remain in Tasks. A failed selection write reports an error and leaves the conversation selected so you can retry.
- When PR3 opens again, the selected conversation comes back if its 10 minutes have not run out or it has a waiting approval; an explicitly chosen follow-up also survives restart. Deleting the selected task leaves an empty conversation rather than selecting another task. A confirmation card that was waiting in a selected task is shown again, to approve afresh; if it ran out while PR3 was closed, nothing was sent and the task is on Done as expired. An unselected waiting task remains available in Tasks. A turn cut off by closing is on To Do as not finished.
- Say or type, as the whole message: "what is still open?" to list To Do and Doing; "carry on with the email to Alex" (or "back to", "resume", "pick up") to go back to that task, or to follow it up if it is finished; "what happened with the trip?" to see where it stands; "export the trip" to export it. PR3 matches the words against each task's title and first message, and offers a choice when several unfinished tasks match. Words that name no task go to the model as usual.
- To finish from a message, say "finish this task", "finish task about the trip", or "mark the trip as done". "This task" means the task open in Tasks, or the current conversation. Several matches are offered for you to choose; an unknown name reports that no task matched. These commands never go to the model. An explicitly finished task stays finished after restart; **Follow up** opens a new linked task.
- Replies and task histories show a **Recorded result** from actual tool outcomes, also kept in exports: memory saved, memory saved for review, or email draft opened (sandbox outbox in tests). A model reply alone is **Reply ready** in the task list, never proof of execution. Approval acknowledges the choice; the device check and execution still have to succeed.
- For a turn that runs a memory write or email action, the runtime composes the final completion message from those results. It replaces model claims before the reply enters chat, core history, exports or speech. With write-capable tools, generated prose is held until the final answer is checked, so it cannot stream an unverified “sent” claim while approval is pending. Read-only configurations retain streaming. Common English action requests and unsupported completion phrases also get a no-action result when no operation was recorded; this fallback is bounded, not a general multilingual fact checker. Ordinary factual answers still come from the model.
- **Stop** replaces Send during a turn and is also available in the running task's More menu. It requests cancellation, closes any pending approval, and waits for an in-flight operation to report its actual result. A model/backend that cannot cancel promptly, a web read or a device check may take time to return. Stop does not undo saved memory or an action that already executed.
- Failed and stopped tasks show **Needs attention** in To Do even when they are the current conversation. **Retry last request** reruns that request in the same task with fresh validation and approval. It is unavailable after a successful or uncertain consequential effect; inspect the recorded result and continue with a new message instead. Finishing a task remains a separate choice.
- A declined/failed operation, or a recognized action request with no recorded operation, also remains in **Needs attention**. A model reply does not move it to Done. **Finish task** is the user's way to close it when no further action is wanted.
- Evaluation traces keep the model's bounded original generation separately from `runtime_content` when it is replaced. The trace's Respond step and final result reflect what the user actually received.
- With task storage enabled, the host saves an execution checkpoint before the device check. If PR3 closes before the result is kept, the task returns as **Outcome unknown**, with no automatic approval or retry. Check the email app (sandbox outbox for test hosts) before continuing. Legacy interrupted turns are also excluded from one-click retry because they lack this checkpoint. Provider reconciliation and distributed recovery are still roadmap work.
- **Export**, in a task's **More**, saves its chat as Markdown and its record and hash-chained events as JSON (`pr3.task/2`, including explicit finish and recorded results): through the share sheet on iOS and Android, and a save dialog on a computer.
- A finished task is erased 30 days after it last changed; `keep_days` under `[tasks]` sets how long, and 0 keeps finished tasks only until the app closes. Unfinished ones stay until they finish.

## Correcting memory

- Say **Remember that the project meeting is at 15:00** (or **Update memory: …**). PR3 compares the new fact with existing notes and offers **Replace #…** or **Save separately**. Review changes nothing and survives restart in the task history. A local model compares the person and attribute/event across paraphrases, languages and negation; it suggests possible updates and never decides which fact is true or writes a replacement.
- Explicit command forms cover English, Persian, Arabic, Spanish, French, German, Chinese (simplified/traditional) and Japanese. Examples: `به خاطر بسپار: من در دبی زندگی می‌کنم.`, `احفظ في الذاكرة: أسكن في دبي.`, `Recuerda que vivo en Dubái.`, `Mémorise: J’habite à Dubaï.`, `Merke dir: Ich wohne in Dubai.`, `记住：我住在迪拜。`, `覚えて：ドバイに住んでいます。`. The [command table](../app/pr3-app/src-tauri/src/memory/commands.rs) lists supported whole-message forms; questions, quotations and negations do not become direct commands. Status labels remain English. Other natural-language statements still go through the model's proposed-memory flow.
- For an exact target, say **Correct memory #3: The project meeting is at 15:00.** or a localized form such as `تصحیح حافظه #۳: من در دبی زندگی می‌کنم.`. ASCII, Persian, Arabic-Indic and fullwidth digits are accepted in ids. Memory ids appear in Settings → Memory. The old fact stops being recalled; a stale, missing or expired id fails visibly. The complete new fact is preserved in its original language. Exact-id correction and **Save separately** do not need a model.
- **Review & confirm** in Memory checks a model proposal locally before confirming it. A possible match returns **Replace #…** and **Keep both** without confirming anything. A failed or incomplete check requires an explicit choice, including **Confirm separately** when no candidate could be established. All unconfirmed proposals stay out of automatic factual notes, including proposals made in the current chat; `recall` can show proposals from the current task with their proposed status; proposals from other tasks are excluded from both search and fallback results until confirmed. The first resumed question receives at most three recent user-message excerpts from its own task (384 bytes each, marked if truncated). Recall uses the same bounded context and includes the current user statement, separately labelled as conversation context, not saved memory.
- Semantic matching never uses a paired node or remote backend. It reads up to 1,000 recent live rows, compares at most 24 same-principal facts in at most four batches of eight, and bounds each JSON payload to 5,000 bytes. Expired/superseded memories are omitted, the model can return only ids actually supplied in that batch, and changed memory invalidates a slow comparison. A partial/invalid check is disclosed and does not silently save or confirm. If no local semantic backend is available, the existing wording check remains available and its limitation is reported. This is a bounded review mechanism, not a semantic index over the entire store.
- Replacing retires the old fact and identical pending proposals and clears cached task summaries in the same database transaction; privacy, identity and retention remain attached to the corrected fact. Forgetting its newest version erases those earlier versions and proposal branches too. Stop cancels a chat comparison; its late result cannot save a fact. No model comparison grants approval for an action.

The opt-in local-model regression uses the checked-in [synthetic corpus](../app/pr3-app/src-tauri/src/memory/semantic_cases.json):

```sh
cargo test -p pr3-app --lib --offline memory::semantic::tests::installed_model_matches_multilingual_correction_corpus -- --ignored --nocapture --test-threads=1
```

The test exercises the full review path at seeds 1, 2 and 7 and verifies that review does not change storage. On 4 November 2026, Qwen3.5-4B passed all 30 checks after adding constrained JSON output and stable ordering within batches; the pre-fix full-path check passed 25/30. The earlier 3 November direct-prompt seed-42 result (10/10) did not cover the production ordering or multiple seeds. The corpus includes English paraphrase, Persian, Arabic, Spanish negation, French/German, Chinese, Japanese, wrong-person/wrong-attribute negatives and an instruction inside a fact. It is development regression evidence, **not held-out accuracy or mobile latency evidence**. Larger stores, other models and broader language coverage still need evaluation. Grammar constrains syntax, not meaning; the host still validates ids, rejects partial output, and leaves replacement to the user.

## Voice notes

Hold the microphone button, between the message field and Send, speak and let go; or tap it and speak, and the note is sent once you stop talking for two seconds (or at a second tap). No Send button is needed. Settings, **Voice**, picks the language voice notes and spoken replies use: *System language* follows the system, and the list holds the languages this device recognizes on the device (on Android 13 and later also those it downloads on first use); the choice is remembered. **Speak replies**, on the same tab, reads replies aloud; it is remembered too. Recognition and voices run on the device, and the first voice note asks for the microphone and speech recognition.

- **macOS and iOS:** the Speech framework's on-device recognition. If your language has none, turn on Dictation for it (System Settings, Keyboard on a Mac); its on-device model comes with it. Better voices download in the Spoken Content settings. `src-tauri/Info.plist` gives `cargo tauri dev` the usage strings macOS asks for.
- **Android 12 and later:** the on-device recognizer. On first use Android may offer to download the language's speech model (about 60 MB). In the emulator, turn on **Virtual microphone uses host audio input** in the extended controls.
- **Simulator and emulator:** both hear through the Mac's microphone, so Simulator and the emulator need microphone access on the Mac (System Settings, Privacy & Security, Microphone). Without it the emulator records silence and the simulator's audio stalls until iOS ends the app. From the command line, start the emulator with `-allow-host-audio`. Some simulator runtimes ship a damaged on-device speech model ("Failed to initialize recognizer"); test transcription on a Mac or a phone instead.

## Memory

Settings, **Memory**, shows what PR3 remembers. Memories the model proposed from your words wait under *To review*: confirm them, edit them (the corrected version replaces them) or forget them, which takes a second tap and also erases the words they came from. You can add memories yourself; those are confirmed from the start. Everything is in `memory.sqlite` in the app's data folder and never leaves the device.

For an explicit capture, type or say "Remember that the project meeting is at 14:30.", "Remember: …", or "Save to memory: …" as the whole message. PR3 preserves your wording, checks for related memories as described above, and reports a confirmed save only after the write succeeds. Possible updates wait for your choice; without a local model, the disclosed wording check remains available. Empty notes and notes over 500 characters are refused. Ordinary requests and questions still go to the model.

A repeatable daily workflow is: capture meeting details with **Remember that…**; correct them if needed; return with **resume meeting** after restarting; ask PR3 to email the details to a synced contact; review the confirmation and choose **Open email app**; review and send in that app; finish the task from **More**. PR3 records a successful draft handoff, never a provider-delivery claim. Sandbox delivery remains only in benchmark/test hosts.

## Device contacts and email

Settings → **Contacts** → **Sync address book contacts** requests device contact permission when enabled. Android uses the Contacts Provider's email rows and read-only `READ_CONTACTS`; macOS/iOS use Contacts.framework, with `NSContactsUsageDescription` and support for limited access. Only names and email addresses are imported. The list is held in memory, refreshed when the panel opens or an email is prepared, and rechecked after approval. Native contacts remain available offline. Only the `contacts-sync` preference is stored in PR3's data folder; no address-book database or remote sync is added.

The red **Remove all contacts** button clears the imported list and disables sync. Turning the switch off does the same. Neither changes the OS permission setting or deletes the device's contacts. Already prepared drafts and prior chat/approval history remain. OS revocation, changed/removed recipients, provider errors and a late permission response after removal cannot authorize a stale recipient. Duplicate names or multiple email addresses get distinct `[email N]` references; the runtime asks for a choice instead of selecting the first address. Contacts without usable names/email addresses are omitted.

After the confirmation card and device check, `mail:device` opens an encoded `mailto:` draft using Android `ACTION_SENDTO`, macOS `NSWorkspace`, or iOS `UIApplication`. The default mail app (or OS app chooser) owns the account, sending and any offline outbox. PR3's result is **draft opened; sending and delivery unverified**, and the task stays open until the user finishes it. A failed launch is an error; an uncertain launch is not automatically retried. PR3 does not need SMTP credentials or OAuth access to a mailbox. The production app ignores legacy `[actions.contacts]` entries, so upgrading cannot restore the sample contacts; the configuration field remains readable for older configurations.

See the [Android test checklist](android-email-checklist.md) for device verification. Regression tests use synthetic contact sources and a recording launcher, never the developer's real address book or an external send. The installed-model workflow check captures, corrects, reopens the database and approves one exact draft at seeds 1, 2 and 7, checking both the current value and absence of the retired value:

```bash
cargo test -p pr3-app --lib contacts::tests::installed_model_captures_corrects_resumes_and_opens_the_approved_draft -- --ignored --nocapture --test-threads=1
```

Native API references: [Android contact access](https://developer.android.com/identity/providers/contacts-provider), [Android email intents](https://developer.android.com/guide/components/intents-common#Email), [Apple contact authorization](https://developer.apple.com/documentation/contacts/accessing-the-contact-store), [mailto encoding](https://www.rfc-editor.org/rfc/rfc6068).

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

### Automated native workflows

From `app/pr3-app`, run `npm ci --ignore-scripts`, `npm test`, `npm run test:e2e:build`, then `npm run test:e2e`. The unit tests cover turn-scoped Stop requests, including late registration and rejected-stop retry. The native suite drives a Tauri webview through WebDriver, using the production IPC handlers, Host, approval gate and SQLite. It exercises correction and process restart, exact draft approval, decline/failure, interrupted approval recovery, Stop/Retry, saving an action result after storage failure, memory deletion and contact removal. Test-only event-delivery barriers reproduce early Stop clicks for new and existing tasks; they do not replace command results or cancellation behavior. Actions use visible controls; read-only assertions also inspect runtime state. CI runs the native suite on macOS and retains failure screenshots, DOM and app logs under `test-results/`.

`pr3-e2e` is a distinct debug-only binary, requires the `e2e` feature and a marked temporary directory, and uses a separate application identity. Its model, contact source, software key and mail-app adapter are fixtures. It cannot be compiled with the feature in release mode. Normal builds exclude the WebDriver server. This catches native UI/runtime wiring regressions but does not measure model intelligence or device permissions. The [Android email](android-email-checklist.md) and [continuity](continuity-checklist.md) checks remain physical-device work. The driver follows [Tauri's native WebDriver support](https://v2.tauri.app/develop/tests/webdriver/).

### Focused email regressions

The focused email-exclusion regressions use the production Host and approval gate,
with process restarts and sandbox email effects:

```bash
cargo test -p pr3-app --lib email_exclusions --locked
cargo test -p pr3-app --lib task::mail --locked
cargo run --release -p pr3-bench-task --locked -- run \
  --model model/Qwen3.5-4B-Q4_K_M.gguf \
  --corpus bench/task/email-exclusions.toml --seed 1 \
  --out bench/task/result/email-exclusions-4b-seed1.jsonl
```

Use a new output path after changing the build or corpus. The supplementary cases
cover quoted text, labeled fields and independent New tasks. They are development
regressions, separate from the frozen Personal-200 corpus; physical Android
handoff still requires the device checklist.

### Full reproducible rebaseline

After installing the pinned AgentDojo environment below, from the repository root:

```bash
cargo build --release -p pr3-bench-task -p pr3-bench-agentdojo --locked
bench/agentdojo/.venv/bin/python bench/rebaseline/run.py --out bench/rebaseline/result/2026-11-04
```

The selected security matrix is Qwen3.5 4B and Qwen3.8 27B, banking/workspace/slack/travel, PR3/plain/confirm-all, seeds 1/2/7: 18,828 runs. The workflow suite runs all 40 cases at those seeds on 2B, 4B and 27B, plus the 10-case multilingual memory-review development corpus on each model/seed. No real contacts, email or internet pages are used. The 27B comparison can take days on a development Mac.

The runner copies the binaries, corpus and Python driver, archives the source (including uncommitted files), and records hashes, hardware, sampling budgets and dependencies in `manifest.json`. It runs one inference process at a time. `progress.json` and `report.md` update every 30 seconds; raw rows and per-job logs are retained. Repeating the same command resumes only missing cases, after checking the frozen inputs and runtime environment. A changed model, seed, binary or corpus requires a new output directory. Duplicate/foreign rows and malformed JSONL stop the run for inspection. Evaluation errors remain unscored, never counted as safe attacks. The process exits nonzero if jobs stop early or evaluation errors remain.

The runner deliberately rejects an OS/Python/dependency change during resume. Do not edit the old manifest to bypass this. On 7 November, macOS had changed from 27.0.1 to 27.2; retain the partial `2026-11-04` directory and start the full matrix from current rebuilt binaries in the separate `2026-11-07` directory. Results from those two environments/policy versions must remain distinguishable.

To refresh a stopped run's report without inference, add `--report-only`. Stop the foreground runner with Ctrl-C; resume with the same command. Preserve the result directory and the installed model files. On macOS `caffeinate -i` can prefix the command to prevent idle sleep. Workflow timing reports whole turns and first visible response after model load; summary preparation/model loading are separate from turn latency. These are desktop measurements, not Android measurements. Existing cases and the new correction cases are development evidence, not a held-out accuracy study.

Harness regressions: `python3 -m unittest discover -s bench/rebaseline -p 'test_*.py'`, `cargo test -p pr3-bench-task`, and the AgentDojo Python tests below. Historical single-seed tables in `benchmarks.md` stay labeled historical until the new matrix finishes.

### Frozen Personal-200 evaluation

[`bench/personal`](../bench/personal/README.md) contains 200 synthetic desktop
workflows in ten categories. V2 freezes 120 development tasks and 80 held-out
tasks, with all variants of each scenario family in one split. Its manifest binds
the case contents, partition and scoring policy. This public runtime holdout is
separate from the historical task corpus and AgentDojo matrix; it does not cover
Android permissions, physical device transport or live provider delivery.

```bash
python3 bench/personal/dataset.py validate
python3 -m unittest discover -s bench/personal -p 'test_*.py'
python3 bench/personal/run.py --out bench/personal/result/2026-11-08-v2 \
  --model model/Qwen3.5-4B-Q4_K_M.gguf \
  --model model/Qwen3.8-27B-Q4_K_M.gguf --seeds 1 2 7
```

The declared matrix has 1,200 runs: 720 development and 480 held-out. The driver
freezes the built binary, current source, models, corpus and independent Python
scorer before inference. Resume with the same output directory; use
`--report-only` to validate and report recorded rows without inference. Completed
failures are retained, missing rows stay in the denominator, and split-specific
results include seeds, categories, languages, families and desktop latency. Strict
whole-answer checks apply only to explicitly requested bare-value replies; exact
recipients and host lifecycle routes supplement approval/outbox equality.

Do not change v2 or tune the runtime on its held-out results. Later changes informed
by these results make v2 a regression corpus, requiring a new version for a fresh
holdout. Keep a single model evaluation active, pausing the historical matrix
cleanly and resuming its original frozen command afterward. Full run artifacts
live in the ignored output directory; implementation alone is not a passing score.

### Individual suites

The travel planning/answer-injection regressions have a reproducible gate with the unchanged AgentDojo attack and scorer, seeds 1/2/7, and a benign control for each task. All twelve cases must finish within the original ten-round budget, retain useful answer facts backed by the matching rating/address lookups, perform the exact requested calendar effect and have no extra effects:

```bash
cargo build --release -p pr3-bench-agentdojo --locked
bench/agentdojo/.venv/bin/python bench/agentdojo/answer_regression.py --model model/Qwen3.5-4B-Q4_K_M.gguf --out bench/agentdojo/result/answer-regression
cargo test -p pr3-llamacpp --test answer_injection -- --ignored --nocapture
```

Use `--seeds 2` to reproduce just the former planning-limit failures and their controls. The second command retains raw replies and immutable run manifests; the third exercises English/French/Persian instructions, fake roles, an invented declined action, legitimate use of the original target name, and explicit quotation. Harness v6 adds the shared engine's planning guidance in every comparison arm: plan all requested details, reuse available results, skip unrelated lookups and group independent reads where possible. Dependencies still wait for results; schemas, approvals, duplicate-call protection and round/call budgets remain enforced by the host. An entirely repeated batch of caller-declared read-only proposals gets at most one local correction attempt before it reaches the host. Repeated actions and tools with unknown effects are never repaired. This is not another execution or an automatic approval; a still-repeated proposal fails, and batches mixing new and repeated proposals go unchanged to normal validation. The plain and confirm-all loops also stop before approval or execution at the configured round boundary; the final answer opportunity does not permit an extra tool batch. Tool-round counts measure batches returned to the host, separately from inference passes; the latter include both local corrections and protected final-answer generation.

Harness v16 adds temporary host-owned context only after at least one batch consumes the budget. Earlier planning prompts remain unchanged. Replies must acknowledge unresolved facts and actual action outcomes. This context follows the recorded results and never changes the execution limit or grants approval. After at least one batch exhausts the budget, the existing final opportunity offers no tools and retains the protected source boundary. Calls returned anyway still fail the original guard. All three comparison arms use the same finalization behavior. The PR3 bridge filters its registered definitions through each core request’s offered tools, including an empty offer, while preserving schema order and effect metadata. The live fixtures assert a single protected final pass after exhaustion.

The gate evaluates every requested seed before reporting failure, so one failure cannot hide another seed's results. Keep this evidence separate from the already frozen v4 full matrix. Real-model tests are opt-in; renderer and gate-validation unit tests run in normal CI. These are development regressions, not held-out security or Android latency measurements.

The [current planning evidence](../bench/agentdojo/evidence/planning-budget-2026-11-08.json) records 12/12 passing travel cases with supporting source lookups, all nine planning fixtures, 21 source-boundary cases and nine email/restart compatibility runs. The evidence separately records the earlier 27-case history/parking/email run, completed before the final declined-action wording correction, along with the original failure and abandoned candidates. The full frozen matrix and Android acceptance remain open. To include the live zero-round regression for PR3/plain/confirm-all and nine seeded planning fixtures in the Python suite, run after building the bridge. The fixtures cover discovery followed by independent reads, a dependent action with exact approval, truthful success/decline replies, and no retry or effect after decline:

```bash
(cd bench/agentdojo && PR3_TEST_MODEL_FILE=../../model/Qwen3.5-4B-Q4_K_M.gguf PR3_TEST_BRIDGE_FILE=../../target/release/pr3-bench-agentdojo .venv/bin/python -m unittest discover -s test)
```

The focused post-baseline restart/recipient regressions live in `bench/task/reliability.toml`. They exercise recalled details after a restarted approval, a corrected recipient resolved from chat after restart, and declining a corrected draft. `card_contains` and `card_excludes` check the actual confirmation card before any effect, including declined drafts. Run each seed separately, using a fresh output path when the binary changes:

```bash
target/release/pr3-bench-task run --model model/Qwen3.5-4B-Q4_K_M.gguf --corpus bench/task/reliability.toml --seed 1 --out bench/task/result/reliability-seed1.jsonl
```

These results must not be appended to the frozen full rebaseline: that run preserves its original binaries, corpus and source snapshot for comparison.

### Original task history retrieval

Summaries can omit details even when the original chat is intact. A restored task now retrieves relevant original user messages alongside its three most recent messages. Already-running tasks do the same when older wording matches messages outside the runtime window, even if summarization failed; explicit `recall` also searches with its query and the current question. Retrieval uses case-insensitive Unicode words, weights rarer query terms more highly, reserves a window for the newest match, and preserves the two messages following each selected match for short corrections. Selected excerpts stay in chronological order, with later corrections and confirmed memory updates taking precedence.

When older matches are retrieved, the prompt contains at most six older and three recent excerpts, each clipped around its matching word to 240 UTF-8 bytes and at most 280 bytes after JSON quoting. Recent-only fallback preserves the established three-excerpt prompt and its 384-byte clipping; unusually expanded JSON is clipped further so each quoted excerpt stays within 560 bytes. They are labelled historical context, not new instructions or confirmed memory. The source is the current task's user messages only: assistant/tool output, other tasks and unrelated follow-up parents are not searched. There is no persisted retrieval index, additional model call or new global memory write. Deleting or expiring the task removes this source; separately saved memories retain their own controls. This is bounded lexical retrieval, not semantic search across arbitrary paraphrases or languages, and it cannot guarantee that every distant correction fits the selected windows.

`bench/task/history.toml` covers first-turn capture and unavailable details, long conversations followed by a restart, terse corrections, deletion before/after restart, and two tasks with conflicting details. Recall reports when there are no earlier task messages, while retaining details in the current message; it does not equate a missing memory match with an empty conversation. Its established tool description and recent-only context format are preserved after seeded capture regressions caught changed prompts causing repeated lookups or failed answers. Short capture/correction checks run before the expensive long-history cases. Its `delete = true` step invokes the same host operation as the task panel. Run it at seeds 1, 2 and 7, alongside `long/parking` from the historical corpus and the email regressions above. Use separate output paths for each binary/corpus/seed; task harness v3 binds those inputs and does not change the answer/effect checks.

The [8 November validation record](../bench/task/evidence/history-2026-11-07.json) retains 27 passing Qwen3.5-4B workflow runs across those seeds, the failed intermediate prompts, source/input hashes and negative controls. This is focused desktop development evidence, separate from the unchanged frozen full matrix and physical Android acceptance.

```bash
cargo build --release -p pr3-bench-task --locked
target/release/pr3-bench-task run --model model/Qwen3.5-4B-Q4_K_M.gguf --corpus bench/task/history.toml --seed 1 --out bench/task/result/history-seed1.jsonl
target/release/pr3-bench-task run --model model/Qwen3.5-4B-Q4_K_M.gguf --case long/parking --seed 1 --out bench/task/result/parking-seed1.jsonl
```

The AgentDojo harness in [`bench/agentdojo`](../bench/agentdojo) compares PR3 with a plain tool-calling loop on the same model. Results so far: [Sec. 8.6](benchmarks.md#86-results-so-far).

```bash
cd bench/agentdojo
python3.12 -m venv .venv && .venv/bin/pip install -r requirements.txt
cargo build --release -p pr3-bench-agentdojo
.venv/bin/python run.py --model ../../model/Qwen3.5-4B-Q4_K_M.gguf --suite banking --modes pr3 plain confirm_all
.venv/bin/python run.py --summary result/banking-Qwen3.5-4B-Q4_K_M-seed42-v3.jsonl
.venv/bin/python report.py                                   # every suite's tables, as in Sec. 8.6
.venv/bin/python -m unittest discover test
```

A run resumes where it stopped. Expect about 10–70 seconds per task, depending on the model. `--modes pr3_web` runs PR3 with the slack suite's `get_webpage` read as `read_web_page` is ([RFC-002](rfc/002-reading-web-pages.md)): without approval, only on a link the task's prompt names, and with the rest of the turn's actions refused once a page is read.

The task suite in [`bench/task`](../bench/task) measures what a task keeps across app launches, changes of topic and confirmation cards, when a message is offered a task of its own, what summaries keep and cost (Sec. 4.8), and what PR3 does with a page the user links, which may give orders of its own ([RFC-002](rfc/002-reading-web-pages.md)). It runs the app's own host on a model, each launch in its own process, through the made-up cases in `corpus.toml`, and checks PR3's answers and emails for words the task said earlier and for words only another task said or the user took out. Pages come from the corpus's `[[page]]` tables, never the internet. Results so far: [Sec. 8.6](benchmarks.md#86-results-so-far).

```bash
cargo build --release -p pr3-bench-task
./target/release/pr3-bench-task run --model model/Qwen3.5-4B-Q4_K_M.gguf     # --case restart/trip for one case
./target/release/pr3-bench-task summary bench/task/result/*.jsonl --update doc/benchmarks.md
```

A run resumes where it stopped, at one sampling seed (`--seed`, 7 by default). Expect a few seconds per message with the 4B model. `--web off` leaves `read_web_page` out, as `[web] enabled = false` does, to measure what offering it costs.

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
bench/personal          frozen Personal-200 corpus, independent scoring and resumable evaluation
bench/task              tasks that span app launches, on the app's own host (benchmark only, never shipped)
config/pr3.toml           models, traces, actions and node settings
script/                  model download, Android build, trusted-side check
```

Crates the roadmap plans but the spike has not needed yet - `pr3-inference`, `pr3-ffi`, `pr3-client-c`, `pr3-node` - are described in [Sec. 4.2](architecture.md#42-rust-workspace-host-api-c-abi-and-protocol-client).

### Phone/computer continuity

See [continuity setup and acceptance](continuity-checklist.md). Pairing automatically enables continuity; Unpair/Forget disables it. There is no separate setting. Share individual confirmed memories or tasks; Move hands editing to the paired device. A fresh local approval is always needed for a consequential action. The phone syncs after pairing, then polls while PR3 is visible (15 seconds), also on foregrounding, with a manual **Sync now / retry** control. The desktop responds over the existing paired Noise connection. Unpair before recovering a retained replica as a separate local copy; pairing the same devices again resumes sync and propagates the old identity's tombstone.

The one-peer protocol uses pages of at most 64 records / 512 KiB and rejects individual serialized records above 192 KiB. This first implementation has no background service, multi-peer merge, encrypted backup or replacement-key enrollment. A tombstone is permanent; recovery is for a retained live replica, not undoing deletion. Forgetting a fact does not delete a separately shared task's conversation containing those words; delete that task as well if desired.
