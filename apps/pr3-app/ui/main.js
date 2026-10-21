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
    // A paired phone may have answered here or on its computer; show which.
    showPlatform();
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

// Devices: on a computer, pair phones and see them; on a phone, pair with a computer.
// Everything shown here comes from the Rust side; pairing codes are typed, never sent
// anywhere else.
const panel = document.querySelector("#devices");
let pairingTimer;

function el(tag, text, className) {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  if (className) node.className = className;
  return node;
}

function makeButton(text, onClick, className = "") {
  const b = el("button", text, className);
  b.type = "button";
  b.addEventListener("click", onClick);
  return b;
}

async function renderDevices() {
  clearInterval(pairingTimer);
  panel.replaceChildren();
  let info;
  try {
    info = await invoke("devices_info");
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
        const p = await invoke("start_pairing");
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
        const node = await invoke("pair_with_node", { address: address.value.trim(), code: code.value });
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

document.querySelector("#devices-button").addEventListener("click", () => {
  panel.hidden = !panel.hidden;
  if (!panel.hidden) renderDevices();
  else clearInterval(pairingTimer);
});

// The model loads in the background; refresh the header when it is ready.
window.__TAURI__.event.listen("model-changed", showPlatform);
showPlatform();
