// The webview only renders. All decisions live in the Rust runtime.
const { invoke } = window.__TAURI__.core;

const log = document.querySelector("#log");
const form = document.querySelector("#composer");
const input = document.querySelector("#text");

// Model output is untrusted: always textContent, never innerHTML.
function show(kind, text) {
  const el = document.createElement("div");
  el.className = `msg ${kind}`;
  el.textContent = text;
  log.append(el);
  log.scrollTop = log.scrollHeight;
}

async function send(text) {
  show("user", text);
  try {
    const actions = await invoke("send_message", { text });
    for (const action of actions) {
      if (action.type === "reply") show("assistant", action.content);
      else if (action.type === "failed") show("error", action.reason);
    }
  } catch (err) {
    show("error", String(err));
  }
}

form.addEventListener("submit", (event) => {
  event.preventDefault();
  const text = input.value.trim();
  if (!text) return;
  input.value = "";
  send(text);
});

invoke("platform_info")
  .then((p) => {
    document.body.classList.add(p.os);
    document.querySelector("#platform").textContent = `v${p.version} · ${p.os}/${p.arch} · mock model`;
  })
  .catch((err) => show("error", String(err)));
