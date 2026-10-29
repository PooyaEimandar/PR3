// The webview only renders. All decisions live in the Rust runtime, and the types below
// mirror what its commands send (src-tauri/src).
const { invoke, Channel } = window.__TAURI__.core;

// An element of index.html, of the kind the script expects.
function find<T extends Element>(selector: string, type: { new (): T; prototype: T }, root: ParentNode = document): T {
  const found = root.querySelector(selector);
  if (!(found instanceof type)) throw new Error(`index.html has no ${selector} of the expected kind`);
  return found;
}

const log = find("#log", HTMLElement);
const form = find("#composer", HTMLFormElement);
const input = find("#text", HTMLInputElement);
const button = find("button[type=submit]", HTMLButtonElement, form);

// Sets the conversation's task aside, so the next message opens a new one (docs
// Sec. 4.8). It stays under the conversation while there is one.
const newTopicButton = document.createElement("button");
newTopicButton.type = "button";
newTopicButton.className = "new-topic";
newTopicButton.textContent = "New topic";
newTopicButton.hidden = true;
log.append(newTopicButton);

// A consequential action waiting for the user, built by the trusted side from the
// validated intent (pr3-core's ApprovalRequest).
interface ApprovalRequest {
  request: string;
  operation: string;
  provider: string;
  risk: "low" | "high";
  mail: { to: { name: string; address: string }[]; subject: string; body: string };
  /** Milliseconds since the epoch. */
  expires_at: number;
}

// What streams in while a turn runs (the host's TurnEvent).
type TurnEvent =
  | { type: "token"; text: string }
  | { type: "tool"; name: string; arguments: string; success: boolean; output: string }
  | { type: "approval"; request: ApprovalRequest }
  // The message opened a new task; the one the conversation was on is set aside.
  | { type: "task_opened"; id: number; previous: TaskCard | null };

// Tasks (task.rs): the page a task shows on, and the task as the list shows it.
type PageName = "todo" | "doing" | "done";

interface TaskCard {
  id: number;
  title: string;
  page: PageName;
  /** What it waits on, or how it ended. */
  line: string;
  /** Milliseconds since the epoch. */
  updated_at: number;
}

// One step of a task's chat, as the conversation showed it (task.rs's Entry).
type Entry =
  | { type: "user"; text: string }
  | { type: "reply"; text: string }
  | { type: "tool"; name: string; arguments: string; success: boolean; output: string }
  | { type: "failed"; reason: string };

// A task opened from the list (task.rs's TaskView).
interface TaskView extends TaskCard {
  parent: { id: number; title: string } | null;
  running: boolean;
  chat: Entry[];
}

// What a finished turn asks of the host (pr3-core's Action); the UI shows the reply or
// the failure, and the host has handled the rest.
type Action =
  | { type: "reply"; content: string }
  | { type: "failed"; reason: string }
  | { type: "infer" | "call_tool" | "propose" | "trace" };

// Model output is untrusted: always textContent, never innerHTML.
function show(kind: string, text: string): HTMLDivElement {
  const el = document.createElement("div");
  el.className = `msg ${kind}`;
  el.textContent = text;
  addToLog(el);
  return el;
}

// The conversation grows above the New topic button.
function addToLog(node: Node): void {
  log.insertBefore(node, newTopicButton);
  log.scrollTop = log.scrollHeight;
}

const toolText = (t: { name: string; arguments: string; output: string }) => `⚙ ${t.name} ${t.arguments} → ${t.output}`;

// The confirmation card for a consequential action. What it shows comes from the
// validated intent — the contacts' real addresses and the exact text that will be
// sent — never from the model's reply. Nothing is sent unless the user taps Send.
function showApproval(request: ApprovalRequest, before: Node): void {
  const { mail } = request;
  const card = document.createElement("div");
  card.className = "msg approval";

  const title = document.createElement("h2");
  title.textContent =
    mail.to.length > 1 ? `Send this email to ${mail.to.length} people?` : "Send this email?";

  const fields = document.createElement("dl");
  const field = (label: string, value: string) => {
    const dt = document.createElement("dt");
    dt.textContent = label;
    const dd = document.createElement("dd");
    dd.textContent = value;
    fields.append(dt, dd);
  };
  field("To", mail.to.map((c) => `${c.name} <${c.address}>`).join(", "));
  field("Subject", mail.subject);

  const body = document.createElement("div");
  body.className = "body";
  body.textContent = mail.body;

  const meta = document.createElement("p");
  meta.className = "meta";
  const where = request.provider === "mail:sandbox" ? "sandbox outbox" : request.provider;

  const choices = document.createElement("div");
  choices.className = "choices";
  const decline = document.createElement("button");
  decline.type = "button";
  decline.className = "secondary";
  decline.textContent = "Decline";
  const approve = document.createElement("button");
  approve.type = "button";
  approve.textContent = "Send";
  choices.append(decline, approve);

  let timer: number | undefined;
  const finish = (text: string) => {
    clearInterval(timer);
    choices.remove();
    meta.textContent = text;
    card.classList.add("answered");
  };
  const answer = (yes: boolean) => {
    decline.disabled = approve.disabled = true;
    invoke("answer_approval", { request: request.request, approve: yes })
      .then(() => finish(yes ? "Approved" : "Declined: nothing was sent"))
      .catch((err) => finish(String(err)));
  };
  approve.addEventListener("click", () => answer(true));
  decline.addEventListener("click", () => answer(false));

  const tick = () => {
    const left = Math.max(0, Math.round((request.expires_at - Date.now()) / 1000));
    if (left === 0) {
      finish("Expired: nothing was sent");
      return;
    }
    const clock = `${Math.floor(left / 60)}:${String(left % 60).padStart(2, "0")}`;
    meta.textContent = `${where} · ${request.risk} risk · expires in ${clock}`;
  };
  tick();
  timer = setInterval(tick, 1000);

  card.append(title, fields, body, meta, choices);
  log.insertBefore(card, before);
}

let answering = false;

function setBusy(busy: boolean): void {
  answering = busy;
  button.disabled = busy;
  talk.disabled = busy;
  languageSelect.disabled = busy || voice !== null;
  input.placeholder = busy ? "PR3 is answering…" : "Message PR3";
  updateNewTopic();
}

async function send(text: string): Promise<void> {
  // "New topic", typed or said, sets the conversation's task aside instead.
  if (/^\s*new topic[\s.!]*$/i.test(text)) {
    startNewTopic();
    return;
  }
  // The answer shows in the conversation, so a message sent from Tasks goes back to it.
  if (!tasksSection.hidden) showTasks(false);
  const said = show("user", text);
  const bubble = show("assistant pending", "…");
  let streamed = "";

  const onEvent = new Channel<TurnEvent>();
  onEvent.onmessage = (event) => {
    if (event.type === "task_opened") {
      currentTask = event.id;
      foldConversation(event.previous, said);
      // A "Following up" line stays above the follow-up, as the way back to its parent.
      log.querySelector("button.fold.following")?.classList.remove("following");
    } else if (event.type === "token") {
      streamed += event.text;
      bubble.textContent = streamed;
    } else if (event.type === "tool") {
      // Show what PR3 ran on the user's behalf, above the answer it leads to.
      const line = document.createElement("div");
      line.className = `msg tool${event.success ? "" : " failed"}`;
      line.textContent = toolText(event);
      log.insertBefore(line, bubble);
      streamed = "";
      bubble.textContent = "…";
    } else if (event.type === "approval") {
      showApproval(event.request, bubble);
    }
    log.scrollTop = log.scrollHeight;
  };

  setBusy(true);
  try {
    const utcOffsetMinutes = -new Date().getTimezoneOffset();
    const actions = await invoke<Action[]>("send_message", { text, utcOffsetMinutes, onEvent });
    bubble.classList.remove("pending");
    for (const action of actions) {
      // The final reply is canonical; it replaces whatever was streamed.
      if (action.type === "reply") {
        bubble.textContent = action.content;
        if (speakReplies) invoke("speak", { text: action.content }).catch((err) => show("error", String(err)));
      } else if (action.type === "failed") {
        bubble.remove();
        show("error", action.reason);
      }
    }
  } catch (err) {
    bubble.remove();
    show("error", String(err));
  } finally {
    setBusy(false);
    // A paired phone may have answered here or on its computer; show which.
    showPlatform();
    // The turn may have proposed a memory.
    if (showing("memory")) renderMemory();
    if (!tasksSection.hidden && !taskList.hidden) renderTasks();
  }
}

form.addEventListener("submit", (event) => {
  event.preventDefault();
  // Send during a voice note ends it and sends what was heard.
  if (voice) {
    finishVoiceNote();
    return;
  }
  const text = input.value.trim();
  if (!text || button.disabled) return;
  input.value = "";
  send(text);
});

// Voice notes: hold the talk button and speak, then let go; or tap it and speak, and
// stop talking (or tap again). The words are recognized on the device and sent like
// typed text. Replies are spoken too when the user turns that on.
const talk = find("#talk", HTMLButtonElement);
const speakRow = find("#speak-replies", HTMLLabelElement);
const speakToggle = find("input", HTMLInputElement, speakRow);
const voiceUnavailable = find("#voice-unavailable", HTMLParagraphElement);
// A press shorter than this is a tap: listening continues until the next tap.
const HOLD_MS = 300;
// A tapped voice note is sent once the user stops talking this long.
const PAUSE_MS = 2000;
// A voice note that hears nothing new for this long ends by itself, so the
// microphone never stays open unattended.
const SILENCE_MS = 30000;

// What voice notes can do here (speech.rs's SpeechStatus).
interface SpeechStatus {
  /** Voice notes work here; otherwise `why_not` says why. */
  listen: boolean;
  why_not: string | null;
  /** Replies can be spoken. */
  speak: boolean;
  /** The language being recognized, which may be another region of the one picked. */
  locale: string | null;
  /** What the user picked; null follows the system. */
  chosen: string | null;
}

// What a voice note hears (speech.rs's Hearing).
type Hearing = { type: "partial"; text: string } | { type: "failed"; error: string };

interface VoiceNote {
  /** When the press began. */
  since: number;
  /** When words last came. */
  heard: number;
  spoke: boolean;
  held: boolean;
  ending: boolean;
  /** What the field held before, to restore if nothing is sent. */
  typed: string;
  /** Ends the note after a pause, or after long silence. */
  watch: number;
  /** Listening began, or failed to. */
  started: Promise<void>;
}

let speech: SpeechStatus = { listen: false, why_not: null, speak: false, locale: null, chosen: null };
let voice: VoiceNote | null = null;
// A press began with a pointer, so the click that follows is not a key press.
let pointerPress = false;
let speakReplies = recall("pr3.speakReplies") === "on";

// Storage can be unavailable; a choice then lasts until the app closes.
function recall(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function remember(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {}
}

async function setUpSpeech(): Promise<void> {
  renderVoiceStatus();
  try {
    // The language the user picked before, if any, else the system's.
    const picked = recall("pr3.voiceLanguage");
    speech = picked
      ? await invoke<SpeechStatus>("set_speech_language", { language: picked })
      : await invoke<SpeechStatus>("speech_status");
  } catch {
    return;
  }
  setUpLanguages();
  talk.hidden = !speech.listen && !speech.speak;
  speakRow.hidden = !speech.speak;
  speakToggle.checked = speakReplies;
  renderVoiceStatus();
}

// Why voice notes cannot be used here, if they cannot.
function renderVoiceStatus(): void {
  voiceUnavailable.textContent = speech.listen ? "" : speech.why_not || "Voice notes are unavailable here.";
  voiceUnavailable.hidden = speech.listen;
}

speakToggle.addEventListener("change", () => {
  speakReplies = speakToggle.checked;
  remember("pr3.speakReplies", speakReplies ? "on" : "off");
  if (!speakReplies) invoke("stop_speaking").catch(() => {});
});

// The language of voice notes and spoken replies: the system's, unless the user picks
// one of the languages this device recognizes on the device.
const languageField = find("#voice-language", HTMLLabelElement);
const languageSelect = find("select", HTMLSelectElement, languageField);

// The languages voice notes can use here (speech.rs's SpeechLanguages).
interface SpeechLanguages {
  /** What the user picked; null follows the system. */
  chosen: string | null;
  /** Recognized on the device now, as BCP 47 tags. */
  installed: string[];
  /** Recognized on the device after a download on first use (Android). */
  available: string[];
}

const languageName = ((): ((tag: string) => string) => {
  try {
    const names = new Intl.DisplayNames([navigator.language], { type: "language" });
    return (tag) => names.of(tag) || tag;
  } catch {
    return (tag) => tag;
  }
})();

async function setUpLanguages(): Promise<void> {
  let languages: SpeechLanguages;
  try {
    languages = await invoke<SpeechLanguages>("speech_languages");
  } catch {
    return;
  }
  const tags = [...new Set([...languages.installed, ...languages.available])];
  // Nothing to choose from: older Android, or no on-device recognition here.
  if (tags.length === 0) return;
  const options = tags
    .map((tag) => {
      const later = languages.installed.includes(tag) ? "" : " (downloads on first use)";
      return new Option(`${languageName(tag)}${later}`, tag);
    })
    .sort((a, b) => a.text.localeCompare(b.text));
  languageSelect.replaceChildren(new Option("System language", ""), ...options);
  languageSelect.value = languages.chosen ?? "";
  languageField.hidden = false;
}

languageSelect.addEventListener("change", async () => {
  const tag = languageSelect.value || null;
  remember("pr3.voiceLanguage", tag ?? "");
  try {
    speech = await invoke<SpeechStatus>("set_speech_language", { language: tag });
  } catch (err) {
    show("error", String(err));
  }
  renderVoiceStatus();
});

// `held`: started by a press that may become a hold; a tap or a key leaves the note
// to end itself at the next pause.
function startVoiceNote(held: boolean): void {
  if (!speech.listen) {
    show("error", speech.why_not || "Voice notes are unavailable here.");
    return;
  }
  const typed = input.value;
  languageSelect.disabled = true;
  talk.classList.add("listening");
  talk.setAttribute("aria-label", "Listening; release or tap to send");
  input.value = "";
  input.placeholder = "Listening…";
  const onEvent = new Channel<Hearing>();
  onEvent.onmessage = (event) => {
    if (voice !== note || note.ending) return;
    if (event.type === "partial") {
      note.heard = Date.now();
      note.spoke = true;
      input.value = event.text;
    } else if (event.type === "failed") {
      // The recognizer stopped on its own; say so now rather than at release.
      note.ending = true;
      show("error", event.error);
      invoke("cancel_listening")
        .catch(() => {})
        .finally(() => endVoiceNote(note, true));
    }
  };
  const now = Date.now();
  const note: VoiceNote = {
    since: now,
    heard: now,
    spoke: false,
    held,
    ending: false,
    typed,
    watch: setInterval(() => {
      const quiet = Date.now() - note.heard;
      const paused = note.spoke && !note.held && quiet > PAUSE_MS;
      if (paused || quiet > SILENCE_MS) finishVoiceNote();
    }, 250),
    // The first time, the system asks for the microphone and speech recognition.
    started: invoke<void>("start_listening", { onEvent }).catch((err) => {
      endVoiceNote(note, true);
      show("error", String(err));
      throw err;
    }),
  };
  voice = note;
}

async function finishVoiceNote(): Promise<void> {
  const note = voice;
  if (!note || note.ending) return;
  note.ending = true;
  // What the field shows now: the last partial transcript, or words typed meanwhile.
  const shown = input.value.trim();
  try {
    await note.started;
  } catch {
    return;
  }
  input.placeholder = "…";
  let text = "";
  try {
    text = (await invoke<string>("stop_listening")).trim() || shown;
  } catch (err) {
    show("error", String(err));
  }
  endVoiceNote(note, !text);
  if (text) send(text);
}

function cancelVoiceNote(): void {
  const note = voice;
  if (!note || note.ending) return;
  note.ending = true;
  note.started
    .then(() => invoke("cancel_listening"))
    .catch(() => {})
    .finally(() => endVoiceNote(note, true));
}

// `restore`: nothing is sent, so whatever was typed before comes back.
function endVoiceNote(note: VoiceNote, restore: boolean): void {
  clearInterval(note.watch);
  if (voice !== note) return;
  voice = null;
  talk.classList.remove("listening");
  talk.setAttribute("aria-label", "Hold to talk");
  input.value = restore ? note.typed : "";
  setBusy(button.disabled);
}

talk.addEventListener("pointerdown", (event) => {
  if (event.button !== 0 || talk.disabled) return;
  pointerPress = true;
  event.preventDefault();
  talk.setPointerCapture(event.pointerId);
  if (voice) finishVoiceNote();
  else startVoiceNote(true);
});
talk.addEventListener("pointerup", () => {
  if (!voice || !voice.held) return;
  // Held: letting go sends. Tapped: a pause sends, or a second tap.
  if (Date.now() - voice.since >= HOLD_MS) finishVoiceNote();
  else voice.held = false;
});
// A system sheet (a permission prompt, a model download) took the touch away.
talk.addEventListener("pointercancel", () => {
  pointerPress = false;
  cancelVoiceNote();
});
talk.addEventListener("contextmenu", (event) => event.preventDefault());
// Keyboard: Enter or Space on the button starts a voice note and ends it. Android's
// webview reports a tap's click like a key's, so pointer presses are tracked instead.
talk.addEventListener("click", () => {
  if (pointerPress) {
    pointerPress = false;
    return;
  }
  if (voice) finishVoiceNote();
  else startVoiceNote(false);
});
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && voice) cancelVoiceNote();
});

// This build and the model answering (lib.rs's PlatformInfo).
interface PlatformInfo {
  os: string;
  arch: string;
  /** A CI build's version, such as 2026.10.27.30184-dev+fd9a2875dc06; null for a local build. */
  version: string | null;
  model: string;
}

const platformLine = find("#platform", HTMLParagraphElement);

function showPlatform(): void {
  invoke<PlatformInfo>("platform_info")
    .then((p) => {
      document.body.classList.add(p.os);
      platformLine.textContent = `${p.version ?? "local build"} · ${p.os}/${p.arch} · ${p.model}`;
      platformLine.title = platformLine.textContent;
    })
    .catch((err) => show("error", String(err)));
}

// Devices: on a computer, pair phones and see them; on a phone, pair with a computer.
// Everything shown here comes from the Rust side; pairing codes are typed, never sent
// anywhere else.
const panel = find("#devices", HTMLElement);
let pairingTimer: number | undefined;

// A paired phone, or the computer a phone is paired with (hybrid.rs's PeerInfo).
interface Peer {
  id: string;
  name: string;
  address: string | null;
  /** Milliseconds since the epoch. */
  paired_at: number;
}

// What the devices panel shows (hybrid.rs's DevicesInfo).
type DevicesInfo =
  | { role: "node"; name: string; listen: string; phones: Peer[] }
  | { role: "phone"; node: Peer | null }
  | { role: "off" };

// A fresh pairing code to type on the phone (lib.rs's PairingInfo).
interface Pairing {
  code: string;
  expires_in_s: number;
  listen: string;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, text?: string, className?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  if (className) node.className = className;
  return node;
}

function makeButton(text: string, onClick: () => void, className = ""): HTMLButtonElement {
  const b = el("button", text, className);
  b.type = "button";
  b.addEventListener("click", onClick);
  return b;
}

async function renderDevices(): Promise<void> {
  clearInterval(pairingTimer);
  panel.replaceChildren();
  let info: DevicesInfo;
  try {
    info = await invoke<DevicesInfo>("devices_info");
  } catch (err) {
    panel.append(el("p", String(err), "error-text"));
    return;
  }
  if (info.role === "node") {
    panel.append(el("h2", `This computer lends its model to paired phones`));
    panel.append(el("p", `${info.name} · listening on ${info.listen}`, "meta"));
    const pairArea = el("div");
    const pair = makeButton("Pair a phone", async () => {
      try {
        const p = await invoke<Pairing>("start_pairing");
        pairArea.replaceChildren(
          el("p", "On the phone, open Settings, then Devices, and enter:"),
          el("p", p.code.replace(/(\d{3})(\d{3})/, "$1 $2"), "code"),
          el("p", `Address: ${p.listen} (the Android emulator uses 10.0.2.2:${p.listen.split(":").pop()})`, "meta"),
        );
        const left = el("p", "", "meta");
        pairArea.append(left);
        const until = Date.now() + p.expires_in_s * 1000;
        const tick = () => {
          const s = Math.max(0, Math.round((until - Date.now()) / 1000));
          left.textContent = s ? `Valid for one try, ${s}s` : "Expired";
          if (!s) clearInterval(pairingTimer);
        };
        tick();
        clearInterval(pairingTimer);
        pairingTimer = setInterval(tick, 1000);
      } catch (err) {
        pairArea.replaceChildren(el("p", String(err), "error-text"));
      }
    }, "small");
    panel.append(pair, pairArea, el("h3", "Paired phones"));
    if (info.phones.length === 0) panel.append(el("p", "None yet.", "meta"));
    for (const phone of info.phones) {
      const row = el("div", undefined, "row");
      row.append(
        el("span", `${phone.name} · ${new Date(phone.paired_at).toLocaleString()}`),
        makeButton("Forget", async () => {
          await invoke("forget_phone", { id: phone.id }).catch((e) => show("error", String(e)));
          renderDevices();
        }, "secondary small"),
      );
      panel.append(row);
    }
  } else if (info.role === "phone" && info.node) {
    panel.append(el("h2", `Paired with ${info.node.name}`));
    panel.append(el("p", `${info.node.address} · this phone answers on its own when it cannot reach it`, "meta"));
    panel.append(makeButton("Unpair", async () => {
      await invoke("unpair").catch((e) => show("error", String(e)));
      renderDevices();
    }, "secondary small"));
  } else if (info.role === "phone") {
    panel.append(el("h2", "Pair with your computer"));
    panel.append(el("p", "On the computer, open Settings, then Devices, and tap Pair a phone. Then enter its address and code here.", "meta"));
    const form = el("form");
    const address = el("input");
    address.placeholder = document.body.classList.contains("android") ? "10.0.2.2:7313" : "127.0.0.1:7313";
    address.value = address.placeholder;
    address.setAttribute("aria-label", "Computer address");
    const code = el("input");
    code.inputMode = "numeric";
    code.placeholder = "6-digit code";
    code.setAttribute("aria-label", "Pairing code");
    const status = el("p", "", "meta");
    const submit = el("button", "Pair");
    submit.type = "submit";
    form.append(address, code, submit);
    form.addEventListener("submit", async (event) => {
      event.preventDefault();
      submit.disabled = true;
      status.textContent = "Pairing…";
      try {
        const node = await invoke<Peer>("pair_with_node", { address: address.value.trim(), code: code.value });
        status.textContent = `Paired with ${node.name}.`;
        renderDevices();
      } catch (err) {
        status.textContent = String(err);
        submit.disabled = false;
      }
    });
    panel.append(form, status);
  } else {
    panel.append(el("p", "Pairing is off on this computer ([node] in pr3.toml).", "meta"));
  }
}

// Memory: what PR3 remembers about the user. The model can only propose memories;
// confirming, correcting, adding and forgetting happen here, by the user. Everything
// shown is text from the store, never markup.
const memoryPanel = find("#memory", HTMLElement);

// A memory as the panel shows it (memory.rs's MemoryView).
interface Memory {
  id: number;
  text: string;
  /** Proposed by the model and not yet reviewed, or confirmed by the user. */
  status: "proposed" | "confirmed";
  /** Whose information it is: `user`, or the person it is about. */
  about: string;
  /** Milliseconds since the epoch. */
  created_at: number;
  /** The user's words it was proposed from, if it came from a conversation. */
  source: string | null;
  /** It replaces an earlier version the user corrected. */
  corrects: boolean;
}

type MemoryCommand = "add_memory" | "confirm_memory" | "correct_memory" | "forget_memory";

const clip = (text: string, max: number) => (text.length > max ? `${text.slice(0, max - 1)}…` : text);

async function renderMemory(problem?: string): Promise<void> {
  let memories: Memory[];
  try {
    memories = await invoke<Memory[]>("list_memories");
  } catch (err) {
    memoryPanel.replaceChildren(el("p", String(err), "error-text"));
    return;
  }
  memoryPanel.replaceChildren(
    el("h2", "What PR3 remembers"),
    el("p", "PR3 suggests memories from what you say; none is trusted until you confirm it. They stay on this device.", "meta"),
  );
  if (problem) memoryPanel.append(el("p", problem, "error-text"));

  const form = el("form");
  const input = el("input");
  input.placeholder = "Something PR3 should remember";
  input.setAttribute("aria-label", "New memory");
  const add = el("button", "Add");
  add.type = "submit";
  form.append(input, add);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    if (input.value.trim()) act("add_memory", { text: input.value });
  });
  memoryPanel.append(form);

  const proposed = memories.filter((m) => m.status === "proposed");
  const confirmed = memories.filter((m) => m.status === "confirmed");
  memoryPanel.append(el("h3", `To review (${proposed.length})`));
  if (proposed.length === 0) memoryPanel.append(el("p", "Nothing to review.", "meta"));
  for (const m of proposed) memoryPanel.append(memoryItem(m));
  memoryPanel.append(el("h3", `Confirmed (${confirmed.length})`));
  if (confirmed.length === 0) memoryPanel.append(el("p", "Nothing confirmed yet.", "meta"));
  for (const m of confirmed) memoryPanel.append(memoryItem(m));
}

function memoryItem(m: Memory): HTMLDivElement {
  const item = el("div", undefined, `item ${m.status}`);
  const when = new Date(m.created_at).toLocaleDateString();
  const origin = m.source
    ? `From what you said on ${when}: “${clip(m.source, 140)}”`
    : m.corrects
      ? `Corrected by you on ${when}`
      : `Added by you on ${when}`;
  const about = m.about === "user" ? "" : ` · about ${m.about}`;

  const choices = el("div", undefined, "choices");
  if (m.status === "proposed") {
    choices.append(makeButton("Confirm", () => act("confirm_memory", { id: m.id }), "small"));
  }
  choices.append(
    makeButton("Edit", () => {
      const field = el("input");
      field.value = m.text;
      field.setAttribute("aria-label", "Memory");
      const row = el("div", undefined, "choices");
      row.append(
        makeButton("Save", () => act("correct_memory", { id: m.id, text: field.value }), "small"),
        makeButton("Cancel", () => renderMemory(), "secondary small"),
      );
      item.replaceChildren(field, row);
      field.focus();
    }, "secondary small"),
  );
  // Forgetting cannot be undone, so it takes a second tap.
  const forget = makeButton("Forget", () => {
    if (forget.dataset.sure) act("forget_memory", { id: m.id });
    else {
      forget.dataset.sure = "yes";
      forget.textContent = "Forget for good?";
    }
  }, "secondary small");
  choices.append(forget);

  item.append(el("p", m.text), el("p", origin + about, "meta"), choices);
  return item;
}

async function act(command: MemoryCommand, args: Record<string, unknown>): Promise<void> {
  let problem: string | undefined;
  try {
    await invoke(command, args);
  } catch (err) {
    problem = String(err);
  }
  renderMemory(problem);
}

// Models: which of the pinned models are on this device, and which one answers. They
// are kept apart from the app, so an update keeps them, and a download is used only if
// it matches the SHA-256 the config pins (models.rs).
const modelsPane = find("#models", HTMLElement);

// A model as the panel shows it (models.rs's ModelView).
interface ModelView {
  id: string;
  file: string;
  size: number;
  /** The one the config picks for this device. */
  recommended: boolean;
  /** Small enough for this device's memory. */
  fits: boolean;
  /** In the app's model folder. */
  downloaded: boolean;
  /** Only in the repository's model folder (debug builds): usable, not deletable. */
  in_repository: boolean;
  /** Bytes downloaded so far, if a download is unfinished. */
  partial: number;
  downloading: boolean;
  /** Downloading, but waiting for Wi-Fi. */
  waiting: boolean;
  /** Why the last download stopped. */
  error: string | null;
  /** Running, or loading to run. */
  active: boolean;
}

// What the panel shows (models.rs's ModelsInfo).
interface ModelsInfo {
  folder: string | null;
  /** PR3_MODEL chooses the model, so the panel cannot. */
  forced: string | null;
  problem: string | null;
  /** Whether downloads may use mobile data; null on a computer, which never asks. */
  mobile_data: boolean | null;
  models: ModelView[];
}

// A download's progress (models.rs's Progress).
interface DownloadProgress {
  id: string;
  bytes: number;
  size: number;
  done: boolean;
  /** Waiting for Wi-Fi. */
  waiting: boolean;
  error: string | null;
}

// Where each download in the panel shows its progress, by model id.
const progressBars = new Map<string, { bar: HTMLProgressElement; text: HTMLElement; waiting: boolean }>();

const amount = (n: number) => (n >= 1e9 ? `${(n / 1e9).toFixed(1)} GB` : `${Math.round(n / 1e6)} MB`);
const progressText = (bytes: number, size: number) =>
  `${amount(bytes)} of ${amount(size)} (${Math.floor((100 * bytes) / size)}%)`;

async function renderModels(problem?: string): Promise<void> {
  let info: ModelsInfo;
  try {
    info = await invoke<ModelsInfo>("list_models");
  } catch (err) {
    modelsPane.replaceChildren(el("p", String(err), "error-text"));
    return;
  }
  progressBars.clear();
  modelsPane.replaceChildren(
    el("h2", "Models on this device"),
    el(
      "p",
      "Kept apart from the app, so updating PR3 keeps them. Each comes from Hugging Face and is used only if it matches the SHA-256 that PR3 pins.",
      "meta",
    ),
  );
  if (problem) modelsPane.append(el("p", problem, "error-text"));
  if (info.problem) modelsPane.append(el("p", info.problem, "error-text"));
  if (info.forced) modelsPane.append(el("p", `PR3_MODEL chooses the model: ${info.forced}`, "meta"));
  if (info.mobile_data !== null) modelsPane.append(mobileDataToggle(info.mobile_data));
  for (const m of info.models) modelsPane.append(modelItem(m, info.forced !== null));
  if (info.folder) modelsPane.append(el("p", `Stored in ${info.folder}`, "meta"));
}

// On a phone, downloads wait for Wi-Fi unless the user allows mobile data; a download
// that is waiting goes on as soon as they do.
function mobileDataToggle(allowed: boolean): HTMLLabelElement {
  const toggle = el("label", undefined, "toggle");
  const box = el("input");
  box.type = "checkbox";
  box.checked = allowed;
  box.addEventListener("change", () => {
    invoke("set_mobile_data", { allowed: box.checked })
      .then(() => renderModels())
      .catch((err) => renderModels(String(err)));
  });
  toggle.append(box, el("span", "Download over mobile data"));
  return toggle;
}

function modelItem(m: ModelView, forced: boolean): HTMLDivElement {
  const item = el("div", undefined, `item${m.active ? " active" : ""}`);
  const tags = [m.active ? "in use" : "", m.recommended ? "recommended here" : ""].filter(Boolean);
  item.append(el("p", m.file, "name"));
  if (tags.length) item.append(el("p", tags.join(" · "), "tags"));

  const usable = m.downloaded || m.in_repository;
  let state = amount(m.size);
  if (m.downloaded) state += " · downloaded";
  else if (m.in_repository) state += " · in the repository's model folder";
  else if (!m.fits) state += " · too large for this device's memory";
  else if (m.partial > 0 && !m.downloading) state += ` · ${Math.floor((100 * m.partial) / m.size)}% downloaded`;
  item.append(el("p", state, "meta"));

  if (m.downloading) {
    const bar = el("progress");
    bar.max = m.size;
    bar.value = m.partial;
    const text = el("p", progressText(m.partial, m.size), "meta");
    progressBars.set(m.id, { bar, text, waiting: m.waiting });
    item.append(bar, text);
    if (m.waiting) {
      text.textContent = `Waiting for Wi-Fi · ${amount(m.partial)} of ${amount(m.size)}`;
      item.append(el("p", "To go on now, check Download over mobile data above.", "meta"));
    }
  }
  if (m.error) item.append(el("p", `Stopped: ${m.error}`, "error-text"));

  const choices = el("div", undefined, "choices");
  const act = (command: string) => () => {
    invoke(command, { id: m.id })
      .then(() => renderModels())
      .catch((err) => renderModels(String(err)));
  };
  if (m.downloading) {
    choices.append(makeButton("Cancel", act("cancel_download"), "secondary small"));
  } else if (!usable && m.fits) {
    choices.append(makeButton(m.partial > 0 ? "Resume" : "Download", act("download_model"), "small"));
  }
  if (usable && !m.active && !forced && m.fits) {
    choices.append(makeButton("Use", act("use_model"), "small"));
  }
  if (!m.downloading && (m.downloaded || m.partial > 0)) {
    // Deleting cannot be undone, so it takes a second tap.
    const label = m.downloaded ? "Delete" : "Discard";
    const remove = makeButton(label, () => {
      if (remove.dataset.sure) act("delete_model")();
      else {
        remove.dataset.sure = "yes";
        remove.textContent = `${label} ${amount(m.downloaded ? m.size : m.partial)}?`;
      }
    }, "secondary small");
    choices.append(remove);
  }
  if (choices.childElementCount) item.append(choices);
  return item;
}

window.__TAURI__.event.listen<DownloadProgress>("model-download", ({ payload: p }) => {
  if (!showing("models")) return;
  const shown = progressBars.get(p.id);
  if (p.done || !shown || p.waiting !== shown.waiting) {
    renderModels();
    return;
  }
  shown.bar.value = p.bytes;
  shown.text.textContent = progressText(p.bytes, p.size);
});

// Settings: models, voice, memory and pairing, one tab at a time, opened from the
// button beside the composer.
const settings = find("#settings", HTMLElement);
const settingsButton = find("#settings-button", HTMLButtonElement);
const tabs = [...settings.querySelectorAll<HTMLButtonElement>("[role=tab]")];
const TABS = ["models", "voice", "memory", "devices"] as const;
type Tab = (typeof TABS)[number];
const isTab = (name: string | null | undefined): name is Tab => TABS.some((t) => t === name);
const panes: Record<Tab, HTMLElement> = {
  models: modelsPane,
  voice: find("#voice", HTMLElement),
  memory: memoryPanel,
  devices: panel,
};
let tab: Tab = ((): Tab => {
  const last = recall("pr3.settingsTab");
  return isTab(last) ? last : "models";
})();

function showing(name: Tab): boolean {
  return !settings.hidden && tab === name;
}

function openTab(name: Tab): void {
  tab = name;
  remember("pr3.settingsTab", name);
  for (const button of tabs) button.setAttribute("aria-selected", String(button.dataset.tab === name));
  for (const t of TABS) panes[t].hidden = t !== name;
  clearInterval(pairingTimer);
  if (name === "models") renderModels();
  else if (name === "memory") renderMemory();
  else if (name === "devices") renderDevices();
}

function showSettings(open: boolean): void {
  if (open) showTasks(false);
  settings.hidden = !open;
  settingsButton.setAttribute("aria-expanded", String(open));
  if (open) openTab(tab);
  else clearInterval(pairingTimer);
}

settingsButton.addEventListener("click", () => showSettings(settings.hidden !== false));
find("#settings-close", HTMLButtonElement).addEventListener("click", () => showSettings(false));
for (const button of tabs) {
  button.addEventListener("click", () => {
    if (isTab(button.dataset.tab)) openTab(button.dataset.tab);
  });
}

// Tasks (docs Sec. 4.8): To Do, Doing and Done, three pages swiped left and right, in
// place of the conversation. A task opens to its chat; Back returns to the list, and the
// Tasks button to the conversation. The Rust side decides which task a message belongs
// to and which page a task shows on; this only renders it, in casual words.
const tasksSection = find("#tasks", HTMLElement);
const tasksButton = find("#tasks-button", HTMLButtonElement);
const taskList = find("#task-list", HTMLElement);
const taskDetail = find("#task-detail", HTMLElement);
const pagesBox = find(".pages", HTMLElement, tasksSection);
const PAGES: readonly PageName[] = ["todo", "doing", "done"];
const PAGE_NAMES: Record<PageName, string> = { todo: "To Do", doing: "Doing", done: "Done" };
const EMPTY: Record<PageName, string> = {
  todo: "Nothing is waiting on you.",
  doing: "Nothing going on. Say something to start.",
  done: "Nothing finished yet.",
};
const isPage = (name: string | undefined): name is PageName => PAGES.some((p) => p === name);
const pageBoxes = Object.fromEntries(
  PAGES.map((p) => [p, find(`.page[data-page=${p}]`, HTMLElement, pagesBox)]),
) as Record<PageName, HTMLElement>;
const pageButtons = [...tasksSection.querySelectorAll<HTMLButtonElement>(".page-names button")];
// Lines for tasks set aside stay at the top of the conversation, the latest few.
const MAX_FOLDS = 3;
// Back, in a task, is an arrow (Material's arrow_back).
const BACK_ARROW = "M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2z";
let page: PageName = "doing";
// The task the conversation on screen belongs to.
let currentTask: number | null = null;

function ago(ms: number): string {
  const minutes = Math.round((Date.now() - ms) / 60000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  return hours < 24 ? `${hours} h ago` : new Date(ms).toLocaleDateString();
}

// A button that shows only an icon; its label is for screen readers and a long press.
function iconButton(label: string, path: string, onClick: () => void): HTMLButtonElement {
  const b = makeButton("", onClick, "icon");
  b.setAttribute("aria-label", label);
  b.title = label;
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  const shape = document.createElementNS("http://www.w3.org/2000/svg", "path");
  shape.setAttribute("d", path);
  svg.append(shape);
  b.append(svg);
  return b;
}

function showTasks(open: boolean): void {
  if (open) showSettings(false);
  tasksSection.hidden = !open;
  log.hidden = open;
  tasksButton.setAttribute("aria-expanded", String(open));
  if (open) {
    showList();
  } else {
    log.scrollTop = log.scrollHeight;
  }
}

function showList(): void {
  taskDetail.hidden = true;
  taskList.hidden = false;
  renderTasks();
  scrollToPage(page, false);
}

tasksButton.addEventListener("click", () => showTasks(tasksButton.getAttribute("aria-expanded") !== "true"));

function scrollToPage(name: PageName, smooth = true): void {
  page = name;
  pagesBox.scrollTo({ left: PAGES.indexOf(name) * pagesBox.clientWidth, behavior: smooth ? "smooth" : "auto" });
  markPage();
}

function markPage(): void {
  for (const b of pageButtons) b.setAttribute("aria-selected", String(b.dataset.page === page));
}

pagesBox.addEventListener("scroll", () => {
  const i = Math.round(pagesBox.scrollLeft / Math.max(1, pagesBox.clientWidth));
  const name = PAGES[Math.min(PAGES.length - 1, Math.max(0, i))];
  if (name && name !== page) {
    page = name;
    markPage();
  }
});

for (const b of pageButtons) {
  b.addEventListener("click", () => {
    if (isPage(b.dataset.page)) scrollToPage(b.dataset.page);
  });
}

async function renderTasks(): Promise<void> {
  let cards: TaskCard[];
  try {
    cards = await invoke<TaskCard[]>("list_tasks");
  } catch (err) {
    pageBoxes.doing.replaceChildren(el("p", String(err), "empty"));
    return;
  }
  for (const p of PAGES) {
    const mine = cards.filter((c) => c.page === p);
    const name = pageButtons.find((b) => b.dataset.page === p);
    if (name) name.textContent = mine.length ? `${PAGE_NAMES[p]} ${mine.length}` : PAGE_NAMES[p];
    pageBoxes[p].replaceChildren(...(mine.length ? mine.map(taskCard) : [el("p", EMPTY[p], "empty")]));
  }
}

function taskCard(card: TaskCard): HTMLButtonElement {
  const b = makeButton("", () => openTask(card.id), "task-card");
  b.append(el("span", card.title, "title"), el("span", `${card.line} · ${ago(card.updated_at)}`, "line"));
  return b;
}

function entryNode(entry: Entry): HTMLElement {
  switch (entry.type) {
    case "user":
      return el("div", entry.text, "msg user");
    case "reply":
      return el("div", entry.text, "msg assistant");
    case "tool":
      return el("div", toolText(entry), `msg tool${entry.success ? "" : " failed"}`);
    case "failed":
      return el("div", entry.reason, "msg error");
  }
}

// Opens Tasks at one task, as a folded line does.
function openTasks(id: number): void {
  showTasks(true);
  openTask(id);
}

async function openTask(id: number): Promise<void> {
  let task: TaskView;
  try {
    task = await invoke<TaskView>("open_task", { id });
  } catch {
    // Deleted meanwhile: show the list as it is now.
    showList();
    return;
  }
  taskList.hidden = true;
  taskDetail.hidden = false;
  const top = el("div", undefined, "top");
  top.append(iconButton("Back", BACK_ARROW, showList), el("h2", task.title));
  const status = el("p", `${PAGE_NAMES[task.page]} · ${task.line} · ${ago(task.updated_at)}`, "meta");
  const parts: Node[] = [top, status];
  const parent = task.parent;
  if (parent) parts.push(makeButton(`Follows up: ${parent.title}`, () => openTask(parent.id), "secondary small"));
  const chat = el("div", undefined, "chat");
  chat.append(...task.chat.map(entryNode));
  parts.push(chat);

  const choices = el("div", undefined, "choices");
  const waitForAnswer = (b: HTMLButtonElement) => {
    b.disabled = answering || task.running;
    return b;
  };
  // The conversation on screen is this task's, approval cards and all, and the next
  // message goes on with it.
  if (task.id === currentTask && (task.running || task.page === "doing")) {
    choices.append(makeButton("Back to the conversation", () => showTasks(false), "small"));
  } else if (task.page === "done") {
    choices.append(waitForAnswer(makeButton("Follow up", () => followUp(task.id, status), "small")));
  } else {
    choices.append(waitForAnswer(makeButton("Continue", () => continueTask(task.id, status), "small")));
  }
  // Deleting cannot be undone, so it takes a second tap.
  const remove = makeButton("Delete", async () => {
    if (!remove.dataset.sure) {
      remove.dataset.sure = "yes";
      remove.textContent = "Delete for good?";
      return;
    }
    try {
      await invoke("delete_task", { id: task.id });
    } catch (err) {
      status.textContent = String(err);
      return;
    }
    log.querySelector(`button.fold[data-task="${task.id}"]`)?.remove();
    if (currentTask === task.id) {
      currentTask = null;
      foldConversation(null);
    }
    showList();
  }, "secondary small");
  choices.append(waitForAnswer(remove));
  parts.push(choices);
  taskDetail.replaceChildren(...parts);
  taskDetail.scrollTop = 0;
}

async function continueTask(id: number, status: HTMLElement): Promise<void> {
  let next: { task: TaskView; previous: TaskCard | null };
  try {
    next = await invoke("continue_task", { id });
  } catch (err) {
    status.textContent = String(err);
    return;
  }
  log.querySelector(`button.fold[data-task="${id}"]`)?.remove();
  log.querySelector("button.fold.following")?.remove();
  foldConversation(next.previous);
  for (const entry of next.task.chat) addToLog(entryNode(entry));
  currentTask = id;
  showTasks(false);
  updateNewTopic();
}

async function followUp(id: number, status: HTMLElement): Promise<void> {
  let next: { parent: TaskCard; previous: TaskCard | null };
  try {
    next = await invoke("follow_up_task", { id });
  } catch (err) {
    status.textContent = String(err);
    return;
  }
  foldConversation(next.previous);
  currentTask = null;
  log.querySelector("button.fold.following")?.remove();
  const line = addFold(id, `Following up: ${next.parent.title}`);
  line.classList.add("following");
  line.dataset.follows = "yes";
  updateNewTopic();
  showTasks(false);
  input.focus();
}

async function startNewTopic(): Promise<void> {
  try {
    const previous = await invoke<TaskCard | null>("new_topic");
    currentTask = null;
    // New topic also drops a follow-up that was about to start.
    log.querySelector("button.fold.following")?.remove();
    foldConversation(previous);
  } catch (err) {
    show("error", String(err));
  }
}

newTopicButton.addEventListener("click", startNewTopic);

// Takes the conversation off the home page up to `from` (the message that opened the
// next task), and folds the task it belonged to into one line that opens it.
function foldConversation(previous: TaskCard | null, from?: Element): void {
  for (const node of [...log.children]) {
    if (node === from || node === newTopicButton) break;
    if (!node.classList.contains("fold")) node.remove();
  }
  // A follow-up's "Following up" line goes with its conversation; the task keeps the link.
  for (const line of log.querySelectorAll("button.fold[data-follows]:not(.following)")) line.remove();
  if (previous) addFold(previous.id, `${previous.title} - in ${PAGE_NAMES[previous.page]}`);
  updateNewTopic();
}

function addFold(id: number, text: string): HTMLButtonElement {
  log.querySelector(`button.fold[data-task="${id}"]`)?.remove();
  const line = makeButton(text, () => openTasks(id), "fold");
  line.dataset.task = String(id);
  const folds = [...log.querySelectorAll("button.fold")];
  const last = folds[folds.length - 1];
  log.insertBefore(line, last ? last.nextSibling : log.firstChild);
  folds.push(line);
  for (const old of folds.slice(0, -MAX_FOLDS)) old.remove();
  return line;
}

function updateNewTopic(): void {
  newTopicButton.hidden = answering || !log.querySelector(".msg, button.following");
}

// The model loads in the background; refresh the header, and the Models tab, when it
// changes.
window.__TAURI__.event.listen("model-changed", () => {
  showPlatform();
  if (showing("models")) renderModels();
});
showPlatform();
setUpSpeech();
