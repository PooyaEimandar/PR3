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
  | { type: "approval"; request: ApprovalRequest };

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
  log.append(el);
  log.scrollTop = log.scrollHeight;
  return el;
}

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

function setBusy(busy: boolean): void {
  button.disabled = busy;
  talk.disabled = busy;
  languageSelect.disabled = busy || voice !== null;
  input.placeholder = busy ? "PR3 is answering…" : "Message PR3";
}

async function send(text: string): Promise<void> {
  show("user", text);
  const bubble = show("assistant pending", "…");
  let streamed = "";

  const onEvent = new Channel<TurnEvent>();
  onEvent.onmessage = (event) => {
    if (event.type === "token") {
      streamed += event.text;
      bubble.textContent = streamed;
    } else if (event.type === "tool") {
      // Show what PR3 ran on the user's behalf, above the answer it leads to.
      const line = document.createElement("div");
      line.className = `msg tool${event.success ? "" : " failed"}`;
      line.textContent = `⚙ ${event.name} ${event.arguments} → ${event.output}`;
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
    if (!memoryPanel.hidden) renderMemory();
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
const speakButton = find("#speak-button", HTMLButtonElement);
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
  speakButton.hidden = !speech.speak;
  renderSpeakButton();
}

function renderSpeakButton(): void {
  speakButton.setAttribute("aria-pressed", String(speakReplies));
  speakButton.title = speakReplies ? "Replies are spoken; tap to stop" : "Replies are text only; tap to hear them too";
}

speakButton.addEventListener("click", () => {
  speakReplies = !speakReplies;
  remember("pr3.speakReplies", speakReplies ? "on" : "off");
  renderSpeakButton();
  if (!speakReplies) invoke("stop_speaking").catch(() => {});
});

// The language of voice notes and spoken replies: the system's, unless the user picks
// one of the languages this device recognizes on the device.
const languageChip = find("#voice-language", HTMLLabelElement);
const languageSelect = find("select", HTMLSelectElement, languageChip);
const languageLabel = find("span", HTMLSpanElement, languageChip);

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
  renderLanguage();
  languageChip.hidden = false;
}

function renderLanguage(): void {
  const tag = languageSelect.value;
  languageLabel.textContent = tag ? (tag.split(/[-_]/)[0] ?? tag).toUpperCase() : "Auto";
  languageChip.title = `Voice language: ${tag ? languageName(tag) : "the system's"}`;
}

languageSelect.addEventListener("change", async () => {
  const tag = languageSelect.value || null;
  remember("pr3.voiceLanguage", tag ?? "");
  renderLanguage();
  try {
    speech = await invoke<SpeechStatus>("set_speech_language", { language: tag });
    if (!speech.listen && speech.why_not) show("error", speech.why_not);
  } catch (err) {
    show("error", String(err));
  }
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
  version: string;
  model: string;
}

const platformLine = find("#platform", HTMLParagraphElement);

function showPlatform(): void {
  invoke<PlatformInfo>("platform_info")
    .then((p) => {
      document.body.classList.add(p.os);
      platformLine.textContent = `v${p.version} · ${p.os}/${p.arch} · ${p.model}`;
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
  const close = makeButton("Close", () => (panel.hidden = true), "secondary small");
  if (info.role === "node") {
    panel.append(el("h2", `This computer lends its model to paired phones`));
    panel.append(el("p", `${info.name} · listening on ${info.listen}`, "meta"));
    const pairArea = el("div");
    const pair = makeButton("Pair a phone", async () => {
      try {
        const p = await invoke<Pairing>("start_pairing");
        pairArea.replaceChildren(
          el("p", "On the phone, open Devices and enter:"),
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
    });
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
    }, "secondary"));
  } else if (info.role === "phone") {
    panel.append(el("h2", "Pair with your computer"));
    panel.append(el("p", "Open Devices on the computer, tap Pair a phone, then enter its address and code.", "meta"));
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
  panel.append(close);
}

find("#devices-button", HTMLButtonElement).addEventListener("click", () => {
  panel.hidden = !panel.hidden;
  if (!panel.hidden) {
    memoryPanel.hidden = true;
    renderDevices();
  } else clearInterval(pairingTimer);
});

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
    memoryPanel.replaceChildren(
      el("p", String(err), "error-text"),
      makeButton("Close", () => (memoryPanel.hidden = true), "secondary small"),
    );
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
  memoryPanel.append(makeButton("Close", () => (memoryPanel.hidden = true), "secondary small"));
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

find("#memory-button", HTMLButtonElement).addEventListener("click", () => {
  memoryPanel.hidden = !memoryPanel.hidden;
  if (!memoryPanel.hidden) {
    panel.hidden = true;
    clearInterval(pairingTimer);
    renderMemory();
  }
});

// The model loads in the background; refresh the header when it is ready.
window.__TAURI__.event.listen("model-changed", showPlatform);
showPlatform();
setUpSpeech();
