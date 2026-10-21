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
