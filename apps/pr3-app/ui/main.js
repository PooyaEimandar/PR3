// The webview only renders. All decisions live in the Rust runtime.
const { invoke, Channel } = window.__TAURI__.core;

const log = document.querySelector("#log");
const form = document.querySelector("#composer");
const input = document.querySelector("#text");
const button = form.querySelector("button");

// Model output is untrusted: always textContent, never innerHTML.
function show(kind, text) {
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
function showApproval(request, before) {
  const { mail } = request;
  const card = document.createElement("div");
  card.className = "msg approval";

  const title = document.createElement("h2");
  title.textContent =
    mail.to.length > 1 ? `Send this email to ${mail.to.length} people?` : "Send this email?";

  const fields = document.createElement("dl");
  const field = (label, value) => {
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

  let timer;
  const finish = (text) => {
    clearInterval(timer);
    choices.remove();
    meta.textContent = text;
    card.classList.add("answered");
  };
  const answer = (yes) => {
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

function setBusy(busy) {
  button.disabled = busy;
  input.placeholder = busy ? "PR3 is answering…" : "Message PR3";
}

async function send(text) {
  show("user", text);
  const bubble = show("assistant pending", "…");
  let streamed = "";

  const onEvent = new Channel();
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
    const actions = await invoke("send_message", { text, utcOffsetMinutes, onEvent });
    bubble.classList.remove("pending");
    for (const action of actions) {
      // The final reply is canonical; it replaces whatever was streamed.
      if (action.type === "reply") bubble.textContent = action.content;
      else if (action.type === "failed") {
        bubble.remove();
        show("error", action.reason);
      }
    }
  } catch (err) {
    bubble.remove();
    show("error", String(err));
  } finally {
    setBusy(false);
  }
}

form.addEventListener("submit", (event) => {
  event.preventDefault();
  const text = input.value.trim();
  if (!text || button.disabled) return;
  input.value = "";
  send(text);
});

function showPlatform() {
  invoke("platform_info")
    .then((p) => {
      document.body.classList.add(p.os);
      document.querySelector("#platform").textContent = `v${p.version} · ${p.os}/${p.arch} · ${p.model}`;
    })
    .catch((err) => show("error", String(err)));
}

// The model loads in the background; refresh the header when it is ready.
window.__TAURI__.event.listen("model-changed", showPlatform);
showPlatform();
