// The webview only renders. All decisions live in the Rust runtime, and the types below
// mirror what its commands send (src-tauri/src).
import { markdown, plain } from "./markdown.js";

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
const stopButton = find("#stop-task", HTMLButtonElement, form);

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
  | { type: "result"; text: string }
  | { type: "token"; text: string }
  | { type: "tool"; name: string; arguments: string; success: boolean; output: string }
  // `sources`: the sites whose pages the model has read and still sees, if any.
  | { type: "approval"; request: ApprovalRequest; sources?: string[] }
  // The message opened a new task; the one the conversation was on is set aside.
  | ({ type: "task_opened" } & Opened)
  // The message seemed to start something new; comes once the turn has ended.
  | { type: "new_task" }
  // Before the answer, PR3 summed up a task's earlier exchanges; the dots stay meanwhile.
  | { type: "summary"; task: number; prompt_tokens: number; completion_tokens: number; ms: number };

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

// One step of a task's chat, as the conversation showed it, and when, in milliseconds
// since the epoch (task.rs's Step).
type Entry = (
  | { type: "user"; text: string }
  | { type: "reply"; text: string }
  | { type: "tool"; name: string; arguments: string; success: boolean; output: string }
  | { type: "failed"; reason: string }
) & { at: number; result?: string };

// A task opened from the list (task.rs's TaskView).
interface TaskView extends TaskCard {
  owned: boolean;
  parent: { id: number; title: string } | null;
  running: boolean;
  /** Its confirmation card waits for the user; after a restart, ask again. */
  waiting: boolean;
  finished: boolean;
  retryable: boolean;
  save_error: string | null;
  chat: Entry[];
}

// The conversation goes back to a task (task.rs's Continued), or the next message
// follows up a finished one (FollowingUp); `previous` is the task set aside.
// A task the conversation goes on in, and the one it set aside (task.rs's Opened).
interface Opened {
  id: number;
  previous: TaskCard | null;
}

interface Continued {
  task: TaskView;
  previous: TaskCard | null;
}

interface FollowingUp {
  parent: TaskCard;
  previous: TaskCard | null;
}

// Words about the task list, answered from it instead of by the model (task.rs's Answer).
type Answer =
  | { type: "open"; cards: TaskCard[] }
  | ({ type: "continued" } & Continued)
  | ({ type: "following_up" } & FollowingUp)
  | { type: "choose"; cards: TaskCard[] }
  | { type: "status"; card: TaskCard }
  | { type: "export"; card: TaskCard }
  | { type: "finished"; card: TaskCard }
  | { type: "choose_finish"; cards: TaskCard[] };

// What a finished turn asks of the host (pr3-core's Action); the UI shows the reply or
// the failure, and the host has handled the rest.
type Action =
  | { type: "reply"; content: string }
  | { type: "failed"; reason: string }
  | { type: "infer" | "call_tool" | "propose" | "trace" };

// Model output is untrusted: always textContent, or markdown(), never innerHTML. A reply
// still coming gets its time once it is in.
function show(kind: string, text: string): HTMLDivElement {
  const el = document.createElement("div");
  el.className = `msg ${kind}`;
  el.textContent = text;
  if (!kind.includes("pending")) timed(el);
  addToLog(el);
  return el;
}

function addToLog(node: Node): void {
  log.append(node);
  markDays(log);
  follow();
}

// A message's time, at its bottom right: "1:57 PM", as the device writes times.
function timed<T extends HTMLElement>(node: T, at = Date.now()): T {
  node.dataset.at = String(at);
  const time = el("time", new Date(at).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" }), "at");
  time.dateTime = new Date(at).toISOString();
  node.append(time);
  return node;
}

const sameDay = (a: number, b: number) => new Date(a).toDateString() === new Date(b).toDateString();

// "Today", "24 October", or "15 November 2024" in another year.
function dayName(at: number): string {
  const day = new Date(at);
  const now = new Date();
  if (sameDay(at, now.getTime())) return "Today";
  const month = day.toLocaleDateString(undefined, { month: "long" });
  const year = day.getFullYear() === now.getFullYear() ? "" : ` ${day.getFullYear()}`;
  return `${day.getDate()} ${month}${year}`;
}

// Above the first message of `chat`, and above each of another day than the one before
// it, a line with its day.
function markDays(chat: HTMLElement): void {
  for (const line of chat.querySelectorAll(":scope > .day")) line.remove();
  let last: number | null = null;
  for (const node of chat.querySelectorAll<HTMLElement>(":scope > [data-at]")) {
    const at = Number(node.dataset.at);
    if (last === null || !sameDay(last, at)) node.before(el("div", dayName(at), "day"));
    last = at;
  }
}

// The log keeps its latest messages in sight - as a reply streams in, or when it gets
// shorter as the keyboard opens - while the user has left it at its end, so they can
// scroll back up while PR3 answers; sending a message goes back to the end. Where they
// left it is where their last scroll ended, not where it is once more has come in.
let atEnd = true;
let held = false;
log.addEventListener("scroll", () => {
  atEnd = log.scrollHeight - log.scrollTop - log.clientHeight < 8;
  markEnd();
});
// A wheel turned up leaves the end before the log has moved.
log.addEventListener(
  "wheel",
  (event) => {
    if (event.deltaY < 0) atEnd = false;
  },
  { passive: true },
);
// A finger on the log holds it still; let go at the end, it catches up. The touch ends on
// the element it began on, even when a streaming reply has since replaced it and the end
// no longer reaches the log.
log.addEventListener(
  "touchstart",
  ({ target }) => {
    held = true;
    const release = () => {
      held = false;
      follow();
    };
    target?.addEventListener("touchend", release, { once: true });
    target?.addEventListener("touchcancel", release, { once: true });
  },
  { passive: true },
);
new ResizeObserver(follow).observe(log);

function follow(): void {
  if (atEnd && !held) log.scrollTop = log.scrollHeight;
  markEnd();
}

function toEnd(): void {
  atEnd = true;
  log.scrollTop = log.scrollHeight;
  markEnd();
}

// Go to the end, above Send: shown while the chat on screen - the conversation, or a task
// opened in Tasks - is not at its end, and a tap jumps there at once.
const toEndButton = find("#to-end", HTMLButtonElement);

function chatOnScreen(): HTMLElement | null {
  if (!log.hidden) return log;
  return document.querySelector<HTMLElement>("#tasks:not([hidden]) #task-detail:not([hidden]) .body");
}

function markEnd(): void {
  const chat = chatOnScreen();
  toEndButton.hidden = !chat || chat.scrollHeight - chat.scrollTop - chat.clientHeight < 8;
}

toEndButton.addEventListener("click", () => {
  const chat = chatOnScreen();
  if (chat === log) toEnd();
  else if (chat) chat.scrollTop = chat.scrollHeight;
  markEnd();
});

const toolText = (t: { name: string; arguments: string; output: string }) => `⚙ ${t.name} ${t.arguments} → ${t.output}`;

// A tool line shows its first three lines, fading out if there is more; tapping it shows
// the rest, and tapping it again folds it back.
const toolLines = new ResizeObserver((lines) => {
  for (const { target } of lines) {
    const line = target as HTMLElement;
    if (!line.classList.contains("folded")) continue;
    const long = line.scrollHeight > line.clientHeight + 1;
    line.classList.toggle("long", long);
    // Only a line with more to show is something to tap.
    if (long) {
      line.tabIndex = 0;
      line.setAttribute("role", "button");
      line.setAttribute("aria-expanded", "false");
    } else {
      line.removeAttribute("tabindex");
      line.removeAttribute("role");
      line.removeAttribute("aria-expanded");
    }
  }
});

const memoryChoices = new Set<HTMLButtonElement>();
function toolNode(t: { name: string; arguments: string; output: string; success: boolean }, taskId: number | null = currentTask): HTMLElement {
  if (t.name === "send_email") {
    const card = el("div", undefined, "msg email-result");
    const opened = t.success && t.output.startsWith("Email draft opened in your email app");
    card.append(el("h3", opened ? "Draft opened" : t.success ? "Email result" : "Email needs your attention"),
      el("p", t.output));
    return card;
  }
  if (t.name === "review_memory" && t.success) {
    try {
      const review: { fact: string; candidates: { id: number; text: string }[]; notice?: string } = JSON.parse(t.arguments);
      if (typeof review.fact !== "string" || !Array.isArray(review.candidates)) throw new Error("Invalid review");
      const card = el("div", undefined, "msg memory-review");
      card.dir = "auto";
      card.append(el("h3", "Update an existing memory?"), el("p", `New fact: ${review.fact}`));
      if (typeof review.notice === "string") card.append(el("p", review.notice, "meta"));
      const choice = (label: string, command: string) => {
        const b = makeButton(label, async () => {
          if (answering) return;
          try {
            if (taskId !== null) {
              const task = await invoke<TaskView>("open_task", { id: taskId });
              if (answering) return;
              if (task.finished || task.page === "done") {
                const next = await invoke<FollowingUp>("follow_up_task", { id: taskId });
                followingUp(taskId, next);
              } else {
                const next = await invoke<Continued>("continue_task", { id: taskId });
                backTo(taskId, next);
              }
            }
            await send(command);
          } catch (err) { card.append(el("p", String(err), "error-text")); }
        }, "secondary small");
        b.disabled = answering;
        memoryChoices.add(b);
        return b;
      };
      for (const old of review.candidates) {
        if (!Number.isSafeInteger(old.id) || old.id <= 0 || typeof old.text !== "string") continue;
        const row = el("div", undefined, "memory-candidate");
        row.append(el("p", `#${old.id}: ${old.text}`), choice(`Replace #${old.id}`, `Correct memory #${old.id}: ${review.fact}`));
        card.append(row);
      }
      card.append(choice("Save separately", `Save separate memory: ${review.fact}`));
      return card;
    } catch { /* Older/malformed records stay readable as plain tool text. */ }
  }
  const line = el("div", toolText(t), `msg tool folded${t.success ? "" : " failed"}`);
  const toggle = () => {
    if (!line.classList.contains("long")) return;
    const folded = line.classList.toggle("folded");
    line.setAttribute("aria-expanded", String(!folded));
  };
  line.addEventListener("click", toggle);
  line.addEventListener("keydown", (event) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    toggle();
  });
  toolLines.observe(line);
  return line;
}

// The confirmation card for a consequential action. What it shows comes from the
// validated intent — the contacts' real addresses and the exact text that will be
// sent — never from the model's reply. Nothing is sent unless the user taps Send. When
// the model has read web pages, the card names their sites: a page may have asked for
// this.
const pendingApprovals = new Set<(text: string) => void>();
function showApproval(request: ApprovalRequest, before: Node, sources: string[] = []): void {
  const { mail } = request;
  const card = document.createElement("div");
  card.className = "msg approval";

  const title = document.createElement("h2");
  title.textContent = request.provider === "mail:sandbox"
    ? "Add this email to the sandbox outbox?"
    : request.provider === "mail:device" ? "Open this draft in your email app?"
    : mail.to.length > 1 ? `Send this email to ${mail.to.length} people?` : "Send this email?";

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
  const where = request.provider === "mail:sandbox" ? "sandbox outbox" : request.provider === "mail:device" ? "Your email app" : request.provider;

  const choices = document.createElement("div");
  choices.className = "choices";
  const decline = document.createElement("button");
  decline.type = "button";
  decline.className = "secondary";
  decline.textContent = "Decline";
  const approve = document.createElement("button");
  approve.type = "button";
  approve.textContent = request.provider === "mail:device" ? "Open email app" : "Approve";
  choices.append(decline, approve);

  let timer: number | undefined;
  const finish = (text: string, keepPending = false) => {
    if (!keepPending) pendingApprovals.delete(finish);
    clearInterval(timer);
    choices.remove();
    meta.textContent = text;
    card.classList.add("answered");
  };
  pendingApprovals.add(finish);
  const answer = (yes: boolean) => {
    decline.disabled = approve.disabled = true;
    invoke("answer_approval", { request: request.request, approve: yes })
      .then(() => finish(yes
        ? answering ? "Approval received. Waiting for the device check and action result." : "Approval received. Review the recorded result below."
        : "Declined: nothing was sent", yes && answering))
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
  card.append(title);
  if (request.provider === "mail:sandbox") card.append(el("p", "This saves a local test email. Nothing is sent externally.", "meta"));
  if (request.provider === "mail:device") card.append(el("p", "Review the draft and press Send in your email app. PR3 cannot verify sending or delivery. You can prepare it offline; delivery needs an internet connection.", "meta"));
  if (sources.length) {
    const from = sources.join(", ");
    card.append(el("p", `This conversation includes text from ${from}. Check that this is what you asked for.`, "sources"));
  }
  card.append(fields, body, meta, choices);
  log.insertBefore(card, before);
  tick();
  if (choices.isConnected) timer = setInterval(tick, 1000);
}

let answering = false;

function setBusy(busy: boolean): void {
  answering = busy;
  for (const b of memoryChoices) {
    if (!b.isConnected) memoryChoices.delete(b);
    else b.disabled = busy;
  }
  button.disabled = busy;
  button.hidden = busy;
  stopButton.hidden = !busy;
  stopButton.disabled = false;
  stopButton.textContent = "Stop";
  if (!busy) {
    for (const close of pendingApprovals) close("Request closed. Review the recorded result below.");
  }
  talk.disabled = busy;
  languageSelect.disabled = busy || voice !== null;
  input.placeholder = busy ? "PR3 is answering…" : "Message PR3";
}

async function stopTask(id: number): Promise<void> {
  stopButton.disabled = true;
  stopButton.textContent = "Stopping…";
  const status = show("assistant", "Requesting stop…");
  try {
    await invoke("stop_task", { id });
    say(status, answering
      ? "Stop requested. Waiting for any in-flight operation to report its result."
      : "Stop request processed. Review the recorded result.");
  } catch (err) {
    stopButton.disabled = false;
    stopButton.textContent = "Stop";
    say(status, String(err));
  }
}
stopButton.addEventListener("click", () => { if (currentTask !== null) stopTask(currentTask); });

async function retryTask(id: number, status: HTMLElement): Promise<void> {
  if (answering) return;
  try {
    const task = await invoke<TaskView>("open_task", { id });
    if (answering) return;
    if (!task.retryable) throw new Error("Review the recorded results and continue with a new message.");
    const last = [...task.chat].reverse().find((entry) => entry.type === "user");
    if (last?.type === "user") await send(last.text, id);
  } catch (err) { say(status, String(err)); }
}

async function send(text: string, retryId?: number): Promise<void> {
  if (answering) return;
  // An offer to start a new task is for the message before this one.
  for (const offer of log.querySelectorAll(".msg.offer")) offer.remove();
  // "New topic", "new task" or "new job", typed or said, sets the conversation's task
  // aside instead.
  if (/^\s*new (topic|task|job)[\s.!]*$/i.test(text)) {
    startNewTopic();
    return;
  }
  if (retryId === undefined && await answerTaskWords(text)) return;
  if (answering) return;
  // The answer shows in the conversation, so a message sent from Tasks goes back to it.
  if (!tasksSection.hidden) showTasks(false);
  toEnd();
  const said = show("user", text);
  const bubble = show("assistant pending", "");
  let streamed = "";
  // Whether to offer a new task, and whether the answer is in to put the offer under.
  let offer = false;
  let answered = false;
  let resultText: string | undefined;
  let resultShown = false;
  let replyToSpeak: string | undefined;
  let failed = false;
  const showResult = () => {
    // IPC events may arrive after the command's promise resolves.
    if (!answered || !resultText || resultShown) return;
    resultShown = true;
    if (bubble.isConnected) bubble.append(resultNode(resultText));
    else if (failed) addToLog(resultNode(resultText));
    if (replyToSpeak) speak(`${replyToSpeak}\nRecorded result: ${resultText}.`);
    follow();
  };

  const onEvent = new Channel<TurnEvent>();
  onEvent.onmessage = (event) => {
    if (event.type === "result") {
      resultText = event.text;
      showResult();
    } else if (event.type === "task_opened") {
      currentTask = event.id;
      foldConversation(event.previous, said);
      // A "Following up" line stays above the follow-up, as the way back to its parent.
      log.querySelector("button.fold.following")?.classList.remove("following");
    } else if (event.type === "token" && !answered) {
      streamed += event.text;
      bubble.replaceChildren(markdown(streamed));
    } else if (event.type === "tool") {
      // Show what PR3 ran on the user's behalf, above the answer it leads to.
      if (bubble.isConnected) log.insertBefore(toolNode(event), bubble);
      else addToLog(toolNode(event));
      if (!answered) {
        streamed = "";
        bubble.textContent = "";
      }
    } else if (event.type === "approval" && !answered) {
      showApproval(event.request, bubble, event.sources);
    } else if (event.type === "new_task") {
      if (answered) offerNewTask(said);
      else offer = true;
    }
    follow();
  };

  setBusy(true);
  try {
    const utcOffsetMinutes = -new Date().getTimezoneOffset();
    const actions = retryId === undefined
      ? await invoke<Action[]>("send_message", { text, utcOffsetMinutes, onEvent })
      : await invoke<Action[]>("retry_task", { id: retryId, utcOffsetMinutes, onEvent });
    bubble.classList.remove("pending");
    for (const action of actions) {
      // The final reply is canonical; it replaces whatever was streamed.
      if (action.type === "reply") {
        bubble.replaceChildren(markdown(action.content));
        timed(bubble);
        follow();
        replyToSpeak = plain(action.content);
      } else if (action.type === "failed") {
        failed = true;
        bubble.remove();
        show("error", action.reason);
      }
    }
  } catch (err) {
    failed = true;
    bubble.remove();
    show("error", String(err));
  } finally {
    setBusy(false);
    answered = true;
    showResult();
    if (offer) offerNewTask(said);
    // The turn may have proposed a memory.
    if (showing("memory")) renderMemory();
    refreshTaskAfterTurn();
  }
}

// The message `said` seemed to start something new (docs Sec. 4.8). It was answered in
// the conversation's task; Start new task moves it and its answer into a task of their
// own, so that task keeps only its own words, and the conversation goes on in the new one.
function offerNewTask(said: Element): void {
  const card = el("div", undefined, "msg assistant offer");
  const choices = el("div", undefined, "choices");
  const start = makeButton(
    "Start new task",
    async () => {
      start.disabled = keep.disabled = true;
      try {
        const opened = await invoke<Opened>("start_new_task");
        currentTask = opened.id;
        card.remove();
        foldConversation(opened.previous, said);
      } catch (err) {
        card.replaceChildren(el("span", String(err)));
      }
    },
    "small",
  );
  const keep = makeButton("Keep here", () => card.remove(), "secondary small");
  choices.append(start, keep);
  card.append(el("span", "Looks like a new task - start it on its own?"), choices);
  addToLog(card);
}

// "What is still open?", "carry on with the email to Alex": words about the task list
// are answered from it. Whether they are is the host's call; when they name no task
// they go to the model like any other message.
async function answerTaskWords(text: string): Promise<boolean> {
  let answer: Answer | null;
  try {
    const context = !tasksSection.hidden && !taskDetail.hidden ? Number(taskDetail.dataset.task) : null;
    answer = await invoke<Answer | null>("task_words", { text, context });
  } catch (err) {
    show("user", text);
    show("error", String(err));
    return true;
  }
  if (!answer) return false;
  if (!tasksSection.hidden) showTasks(false);
  const said = timed(el("div", text, "msg user"));
  const titles = (cards: TaskCard[]) => cards.map((c) => c.title).join("; ");
  switch (answer.type) {
    case "finished": {
      finishedTask(answer.card);
      const line = `Finished “${answer.card.title}”. Closing the task did not send anything.`;
      addToLog(said);
      addToLog(taskAnswer(line, [answer.card], openTasks));
      speak(line);
      break;
    }
    case "choose_finish": {
      addToLog(said);
      const reply = taskAnswer("Which task should be finished?", answer.cards, (id) => finishTask(id, reply));
      addToLog(reply);
      speak(`Which task should be finished: ${titles(answer.cards)}?`);
      break;
    }
    case "open": {
      const n = answer.cards.length;
      const line = n ? `${n === 1 ? "One task is" : `${n} tasks are`} still open:` : "Nothing is still open.";
      addToLog(said);
      addToLog(taskAnswer(line, answer.cards, openTasks));
      speak(n ? `${line} ${titles(answer.cards)}.` : line);
      break;
    }
    case "status": {
      const card = answer.card;
      addToLog(said);
      addToLog(taskAnswer(`It is in ${PAGE_NAMES[card.page]}:`, [card], openTasks));
      speak(`${card.title}: ${card.line}.`);
      break;
    }
    case "choose": {
      addToLog(said);
      const reply = taskAnswer("Which one?", answer.cards, (id) => continueTask(id, reply));
      addToLog(reply);
      speak(`Which one: ${titles(answer.cards)}?`);
      break;
    }
    case "export": {
      const line = `Exporting “${answer.card.title}”.`;
      const reply = timed(el("div", line, "msg assistant"));
      addToLog(said);
      addToLog(reply);
      speak(line);
      exportTask(answer.card.id, reply);
      break;
    }
    case "continued": {
      const line = `Back to “${answer.task.title}”.`;
      backTo(answer.task.id, answer, [said, timed(el("div", line, "msg assistant"))]);
      speak(line);
      break;
    }
    case "following_up": {
      const line = `Your next message follows up “${answer.parent.title}”.`;
      followingUp(answer.parent.id, answer, [said, timed(el("div", line, "msg assistant"))]);
      speak(line);
      break;
    }
  }
  return true;
}

// A reply listing tasks: each card is a button that `pick`s it.
function taskAnswer(line: string, cards: TaskCard[], pick: (id: number) => void): HTMLDivElement {
  const reply = el("div", undefined, "msg assistant task-answer");
  reply.append(el("span", line));
  for (const card of cards) {
    const b = makeButton("", () => pick(card.id), "task-card");
    b.append(el("span", card.title, "title"), el("span", `${card.line} · ${ago(card.updated_at)}`, "line"));
    reply.append(b);
  }
  return timed(reply);
}

function speak(text: string): void {
  if (speakReplies) invoke("speak", { text }).catch((err) => show("error", String(err)));
}

// A confirmation card that was waiting when the app closed is shown again: the trusted
// side takes it as a new request, and the task ends with what came of it.
async function askAgain(id: number): Promise<void> {
  const bubble = show("assistant pending", "");
  let resultText: string | undefined;
  let answerShown = false;
  let resultShown = false;
  const showResult = () => {
    if (answerShown && resultText && !resultShown) {
      if (bubble.isConnected) bubble.append(resultNode(resultText));
      else addToLog(resultNode(resultText));
      resultShown = true;
    }
  };
  const onEvent = new Channel<TurnEvent>();
  onEvent.onmessage = (event) => {
    if (event.type === "approval" && !answerShown) showApproval(event.request, bubble, event.sources);
    else if (event.type === "tool") {
      if (bubble.isConnected) log.insertBefore(toolNode(event), bubble);
      else addToLog(toolNode(event));
    }
    else if (event.type === "result") {
      resultText = event.text;
      showResult();
    }
    follow();
  };
  setBusy(true);
  try {
    const reply = await invoke<string>("ask_again", { id, onEvent });
    bubble.classList.remove("pending");
    bubble.textContent = reply;
    answerShown = true;
    showResult();
    timed(bubble);
    speak(reply);
  } catch (err) {
    bubble.remove();
    show("error", String(err));
  } finally {
    setBusy(false);
    answerShown = true;
    showResult();
    refreshTaskAfterTurn();
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
  // Let the open confirmation handle Escape without closing the settings beneath it.
  if (event.key === "Escape" && document.querySelector("dialog[open]")) return;
  if (event.key === "Escape" && voice) cancelVoiceNote();
  else if (event.key === "Escape" && closeMenu) closeMenu();
  else if (event.key === "Escape" && !settings.hidden) showSettings(false);
});

// This build and the model answering (lib.rs's PlatformInfo).
interface PlatformInfo {
  os: string;
  arch: string;
  /** A CI build's version, such as 2026.10.27.30184-dev+fd9a2875dc06; null for a local build. */
  version: string | null;
  model: string;
}

// Marks the page with its OS, for the insets and defaults that differ.
function markPlatform(): void {
  invoke<PlatformInfo>("platform_info")
    .then((p) => document.body.classList.add(p.os))
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

const removeItemDialog = find("#remove-item-dialog", HTMLDialogElement);
removeItemDialog.addEventListener("click", ({ target }) => {
  if (target === removeItemDialog) removeItemDialog.close();
});

async function confirmItemRemoval(title: string, description: string, action: string): Promise<boolean> {
  if (removeItemDialog.open) return false;
  find("#remove-item-title", HTMLElement).textContent = title;
  find("#remove-item-description", HTMLElement).textContent = description;
  find("#remove-item-action", HTMLButtonElement).textContent = action;
  removeItemDialog.returnValue = "";
  removeItemDialog.showModal();
  await new Promise((closed) => removeItemDialog.addEventListener("close", closed, { once: true }));
  return removeItemDialog.returnValue === "remove";
}

const forgetDeviceDialog = find("#forget-device-dialog", HTMLDialogElement);
forgetDeviceDialog.addEventListener("click", ({ target }) => {
  if (target === forgetDeviceDialog) forgetDeviceDialog.close();
});

async function forgetDevice(peer: Peer, command: "forget_phone" | "unpair"): Promise<void> {
  if (forgetDeviceDialog.open) return;
  find("#forget-device-description", HTMLElement).textContent = `This will unpair ${peer.name} and stop syncing with it. You can pair it again later.`;
  forgetDeviceDialog.returnValue = "";
  forgetDeviceDialog.showModal();
  await new Promise((closed) => forgetDeviceDialog.addEventListener("close", closed, { once: true }));
  if (forgetDeviceDialog.returnValue !== "forget") return;
  try {
    await invoke(command, command === "forget_phone" ? { id: peer.id } : {});
  } catch (err) {
    show("error", String(err));
  }
  await renderDevices();
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
        makeButton("Forget", () => { void forgetDevice(phone, "forget_phone"); }, "secondary small"),
      );
      panel.append(row);
    }
  } else if (info.role === "phone" && info.node) {
    const node = info.node;
    panel.append(el("h2", `Paired with ${info.node.name}`));
    panel.append(el("p", `${info.node.address} · this phone answers on its own when it cannot reach it`, "meta"));
    panel.append(makeButton("Unpair", () => { void forgetDevice(node, "unpair"); }, "secondary small"));
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
        void syncContinuity();
      } catch (err) {
        status.textContent = String(err);
        submit.disabled = false;
      }
    });
    panel.append(form, status);
  } else {
    panel.append(el("p", "Pairing is off on this computer ([node] in pr3.toml).", "meta"));
  }
  await continuitySettings(info);
}

interface ContinuityInfo {
  enabled: boolean;
  peer: string | null;
  last_sync: number | null;
  pending: number;
  generation: number;
  problem: string | null;
  items: {kind: string; id: number; owned: boolean; deleted: boolean; problem?: string | null}[];
}
let continuity: ContinuityInfo | null = null;
let syncRunning = false;
async function refreshContinuity(): Promise<void> {
  continuity = await invoke<ContinuityInfo>("continuity_info").catch(() => null);
}
async function continuitySettings(info: DevicesInfo): Promise<void> {
  await refreshContinuity();
  if (!continuity) return;
  const section = el("section", undefined, "continuity-settings");
  for (const item of continuity.items) {
    if (item.problem) section.append(el("p", `${item.kind === "task" ? "Task" : "Memory"} #${item.id}: ${item.problem}`, "error-text"));
  }
  if (continuity.problem || continuity.last_sync || continuity.pending) {
    const status = continuity.problem ? `Needs attention: ${continuity.problem}` : continuity.last_sync ? `Last exchange: ${new Date(continuity.last_sync).toLocaleString()}` : "No sync completed yet.";
    section.append(el("p", `${status}${continuity.pending ? ` · ${continuity.pending} changes awaiting acknowledgement` : ""}`, "meta"));
  }
  if (continuity.enabled && info.role === "phone") section.append(makeButton("Sync now / retry", async () => { await syncContinuity(true); await renderDevices(); }, "secondary small"));
  if (section.childElementCount) panel.append(section);
}
async function syncContinuity(manual = false): Promise<void> {
  if (syncRunning || answering || document.hidden) return;
  await refreshContinuity();
  if (!continuity?.enabled) return;
  const devices = await invoke<DevicesInfo>("devices_info").catch(() => null);
  if (devices?.role !== "phone") return;
  syncRunning = true;
  try { continuity = await invoke<ContinuityInfo>("sync_continuity"); }
  catch (err) { if (manual) show("error", String(err)); }
  finally { syncRunning = false; }
}
function continuityChoices(kind: "memory" | "task", id: number, after: () => void): HTMLButtonElement[] {
  if (!continuity) return [];
  const item = continuity.items.find(i => i.kind === kind && i.id === id && !i.deleted);
  const owned = item?.owned !== false;
  const perform = async (command: string, args: Record<string, unknown>) => {
    try { await invoke(command, args); await refreshContinuity(); after(); void syncContinuity(); }
    catch (err) { show("error", String(err)); }
  };
  if (continuity.enabled && owned) return [
    ...(!item ? [makeButton("Share with paired device", () => {
      void perform("share_item", { kind, id, moveOwnership: false });
    }, "secondary small")] : []),
    makeButton("Move to paired device", () => {
      void perform("share_item", { kind, id, moveOwnership: true });
    }, "secondary small"),
  ];
  if (!continuity.enabled && !owned) {
    const recover = makeButton("Recover a separate copy", () => {
      if (recover.dataset.sure) void perform("recover_item", { kind, id });
      else { recover.dataset.sure = "yes"; recover.textContent = "Recover copy and retire shared item?"; }
    }, "secondary small");
    return [recover];
  }
  return [];
}
setInterval(() => { void syncContinuity(); }, 15000);
document.addEventListener("visibilitychange", () => { if (!document.hidden) void syncContinuity(); });
window.__TAURI__.event.listen("continuity-changed", async () => {
  const previous = continuity?.generation;
  await refreshContinuity();
  if (answering) return;
  if (currentTask !== null) {
    const item = continuity?.items.find(i => i.kind === "task" && i.id === currentTask);
    if (item && (!item.owned || item.deleted)) { currentTask = null; foldConversation(null); }
  }
  if (previous === continuity?.generation) return;
  if (!tasksSection.hidden) refreshTaskAfterTurn();
  // Refresh only visible panels, preserving an in-progress memory edit.
  if (!memoryPanel.hidden && !memoryPanel.contains(document.activeElement)) void renderMemory();
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
  conflicts: { id: number; text: string }[];
  review_note?: string;
  needs_review?: boolean;
}

interface MemoryConfirmation {
  id: number;
  confirmed: boolean;
  conflicts: { id: number; text: string }[];
  notice: string | null;
}

type MemoryCommand = "add_memory" | "confirm_memory" | "keep_memory_separately" | "correct_memory" | "forget_memory";

const clip = (text: string, max: number) => (text.length > max ? `${text.slice(0, max - 1)}…` : text);

async function renderMemory(problem?: string, review?: MemoryConfirmation): Promise<void> {
  await refreshContinuity();
  let memories: Memory[];
  try {
    memories = await invoke<Memory[]>("list_memories");
  } catch (err) {
    memoryPanel.replaceChildren(el("p", String(err), "error-text"));
    return;
  }
  memoryPanel.replaceChildren(
    el("h2", "What PR3 remembers"),
    el("p", "PR3 suggests memories from what you say; none is trusted until you confirm it. They stay local unless you explicitly share them.", "meta"),
  );
  if (problem) memoryPanel.append(el("p", problem, "error-text"));
  if (review?.confirmed && review.notice) memoryPanel.append(el("p", review.notice, "meta"));
  if (review && !review.confirmed) {
    const target = memories.find(m => m.id === review.id);
    if (target) {
      target.conflicts = review.conflicts;
      target.needs_review = true;
      if (review.notice) target.review_note = review.notice;
    }
  }

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
    if (m.review_note) item.append(el("p", m.review_note, "meta"));
    if (m.conflicts.length > 0 || m.needs_review) {
      item.append(el("p", "This may update an existing memory. Choose what to keep:", "meta"));
      for (const old of m.conflicts) {
        item.append(el("p", `#${old.id}: ${old.text}`, "meta"));
        choices.append(makeButton(`Replace #${old.id}`, () => act("correct_memory", { id: old.id, text: m.text }), "small"));
      }
      choices.append(makeButton(m.conflicts.length > 0 ? "Keep both" : "Confirm separately", () => act("keep_memory_separately", { id: m.id }), "secondary small"));
    }
    choices.append(makeButton("Review & confirm", () => act("confirm_memory", { id: m.id }), "small"));
  }
  const shared = continuity?.items.find(i => i.kind === "memory" && i.id === m.id && !i.deleted);
  if (shared?.owned === true) item.append(el("p", "Selected for sync. Editing is on this device.", "meta"));
  if (shared?.owned === false) item.append(el("p", "Editing belongs to your paired device.", "meta"));
  if (shared?.owned !== false) choices.append(
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
  const forget = makeButton("Forget", async () => {
    const description = `This will permanently forget memory #${m.id}: “${clip(m.text, 160)}”. This can't be undone.`
      + (shared ? " This deletion will also sync to your paired device." : "");
    if (await confirmItemRemoval("Forget memory?", description, "Forget")) await act("forget_memory", { id: m.id });
  }, "secondary small");
  choices.append(forget);
  if (m.status === "confirmed") choices.append(...continuityChoices("memory", m.id, () => { void renderMemory(); }));

  item.prepend(el("p", `#${m.id}: ${m.text}`), el("p", origin + about, "meta"));
  item.append(choices);
  return item;
}

async function act(command: MemoryCommand, args: Record<string, unknown>): Promise<void> {
  let problem: string | undefined;
  let review: MemoryConfirmation | undefined;
  const buttons = Array.from(memoryPanel.querySelectorAll("button"));
  for (const b of buttons) b.disabled = true;
  if (command === "confirm_memory") memoryPanel.append(el("p", "Checking related memories…", "meta"));
  try {
    if (command === "confirm_memory") {
      review = await invoke<MemoryConfirmation>(command, args);
    } else {
      await invoke(command, args);
    }
  } catch (err) {
    problem = String(err);
  }
  await renderMemory(problem, review);
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
  modelsPane.replaceChildren(el("h2", "Models on this device"));
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
    const label = m.downloaded ? "Delete" : "Discard";
    const remove = makeButton(label, async () => {
      const title = m.downloaded ? "Delete model?" : "Discard download?";
      const description = `This will ${label.toLowerCase()} the ${m.downloaded ? "downloaded copy" : "unfinished download"} of ${m.file} (${amount(m.downloaded ? m.size : m.partial)}). You can download it again later.`;
      if (await confirmItemRemoval(title, description, label)) act("delete_model")();
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

// Reading the web pages the user links (web.rs's WebInfo).
interface WebInfo {
  /** False when the config leaves reading out. */
  available: boolean;
  on: boolean;
}

const webPane = find("#web", HTMLElement);

// Whether PR3 reads the pages the user links. Off, the model is told so when it tries.
async function renderWeb(problem?: string): Promise<void> {
  let info: WebInfo;
  try {
    info = await invoke<WebInfo>("web_reading");
  } catch (err) {
    webPane.replaceChildren(el("p", String(err), "error-text"));
    return;
  }
  webPane.replaceChildren(el("h2", "Web"));
  if (problem) webPane.append(el("p", problem, "error-text"));
  if (!info.available) {
    webPane.append(el("p", "Reading web pages is left out by pr3.toml.", "meta"));
    return;
  }
  const toggle = el("label", undefined, "toggle");
  const box = el("input");
  box.type = "checkbox";
  box.checked = info.on;
  box.addEventListener("change", () => {
    invoke("set_web_reading", { on: box.checked })
      .then(() => renderWeb())
      .catch((err) => renderWeb(String(err)));
  });
  toggle.append(box, el("span", "Read web pages you link"));
  webPane.append(
    toggle,
    el(
      "p",
      "PR3 reads only links you write, over HTTPS, and never addresses on your own network. " +
        "A page is information, not instructions: in the turn PR3 reads one, it sends nothing.",
      "meta",
    ),
  );
}

const emailPane = find("#email", HTMLElement);
const removeContactsDialog = find("#remove-contacts-dialog", HTMLDialogElement);
removeContactsDialog.addEventListener("click", ({ target }) => {
  if (target === removeContactsDialog) removeContactsDialog.close();
});
interface ContactsInfo {
  on: boolean;
  permission: "not_determined" | "granted" | "limited" | "denied" | "restricted" | "unavailable";
  count: number;
  problem: string | null;
}
let contactsBusy = false;

async function renderEmail(problem?: string, supplied?: ContactsInfo): Promise<void> {
  let info: ContactsInfo;
  try { info = supplied ?? await invoke<ContactsInfo>("contacts_info"); }
  catch (err) { emailPane.replaceChildren(el("h2", "Contacts"), el("p", String(err), "error-text")); return; }
  emailPane.replaceChildren(el("h2", "Contacts"));
  const toggle = el("label", undefined, "toggle contacts-toggle");
  const box = el("input");
  box.type = "checkbox";
  box.checked = info.on;
  box.disabled = contactsBusy || info.permission === "unavailable";
  box.setAttribute("role", "switch");
  toggle.append(el("span", "Sync address book contacts"), box);
  const status = el("p", undefined, "meta");
  status.setAttribute("role", "status");
  status.hidden = true;
  const perform = async (command: string, args?: Record<string, unknown>) => {
    if (contactsBusy) return;
    contactsBusy = true;
    emailPane.querySelectorAll<HTMLButtonElement | HTMLInputElement>("button, input").forEach((c) => { c.disabled = true; });
    status.hidden = false;
    status.textContent = command === "set_contact_sync" && args?.on ? "Waiting for contact access and syncing…" : "Removing contacts from PR3…";
    try { const next = await invoke<ContactsInfo>(command, args); contactsBusy = false; await renderEmail(undefined, next); }
    catch (err) { contactsBusy = false; await renderEmail(String(err)); }
  };
  box.addEventListener("change", () => { void perform("set_contact_sync", { on: box.checked }); });
  const remove = makeButton("Remove all contacts", async () => {
    if (contactsBusy || removeContactsDialog.open) return;
    removeContactsDialog.returnValue = "";
    removeContactsDialog.showModal();
    await new Promise((closed) => removeContactsDialog.addEventListener("close", closed, { once: true }));
    if (removeContactsDialog.returnValue === "remove") await perform("remove_all_contacts");
  });
  remove.className = "secondary danger";
  remove.disabled = contactsBusy || (!info.on && info.count === 0 && !info.problem);
  remove.title = "Clear contacts from PR3 and turn sync off. Your device contacts stay unchanged.";
  emailPane.append(toggle, remove, status);
  if (info.permission === "denied" || info.permission === "restricted") emailPane.append(el("p", "Contact access is blocked. Allow Contacts for PR3 in your device’s Settings, then enable sync here.", "error-text"));
  if (info.permission === "unavailable") emailPane.append(el("p", "Device contact sync is available on Android, iOS and macOS.", "meta"));
  if (problem || info.problem) emailPane.append(el("p", problem ?? info.problem ?? "", "error-text"));
  if (info.on && info.count === 0 && !info.problem) emailPane.append(el("p", "No email contacts found. Add an email address in your device’s Contacts app.", "meta"));
}

// Settings: models, voice, memory, pairing, email and the web, opened from
// the button beside the composer.
const settings = find("#settings", HTMLElement);
const settingsButton = find("#settings-button", HTMLButtonElement);
const tabs = [...settings.querySelectorAll<HTMLButtonElement>("[role=tab]")];
const TABS = ["models", "voice", "memory", "devices", "email", "web"] as const;
type Tab = (typeof TABS)[number];
const isTab = (name: string | null | undefined): name is Tab => TABS.some((t) => t === name);
const panes: Record<Tab, HTMLElement> = {
  models: modelsPane,
  voice: find("#voice", HTMLElement),
  memory: memoryPanel,
  devices: panel,
  web: webPane,
  email: emailPane,
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
  else if (name === "web") renderWeb();
  else if (name === "email") renderEmail();
}

document.addEventListener("visibilitychange", () => {
  if (!document.hidden && showing("email") && !contactsBusy) void renderEmail();
});

// Settings slides in from the left over the whole screen and back out when closed.
function showSettings(open: boolean): void {
  if (open) showTasks(false);
  settingsButton.setAttribute("aria-expanded", String(open));
  if (open) {
    settings.classList.remove("closing");
    settings.hidden = false;
    openTab(tab);
    return;
  }
  clearInterval(pairingTimer);
  if (settings.hidden) return;
  if (matchMedia("(prefers-reduced-motion: reduce)").matches) settings.hidden = true;
  else settings.classList.add("closing");
}

settings.addEventListener("animationend", () => {
  if (!settings.classList.contains("closing")) return;
  settings.classList.remove("closing");
  settings.hidden = true;
});
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
// Back, in a task, is an arrow (Material's arrow_back); More is three dots (more_vert).
const BACK_ARROW = "M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2z";
const MORE_DOTS =
  "M12 8c1.1 0 2-.9 2-2s-.9-2-2-2-2 .9-2 2 .9 2 2 2zm0 2c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2zm0 6c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2z";
const deleteDialog = find("#delete-dialog", HTMLDialogElement);
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
    toEnd();
  }
}

function showList(): void {
  taskDetail.hidden = true;
  taskList.hidden = false;
  renderTasks();
  scrollToPage(page, false);
  markEnd();
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

function entryNode(entry: Entry, taskId: number | null = currentTask): HTMLElement {
  switch (entry.type) {
    case "user":
      return timed(el("div", entry.text, "msg user"), entry.at);
    case "reply": {
      const reply = el("div", undefined, "msg assistant");
      reply.append(markdown(entry.text));
      if (entry.result) reply.append(resultNode(entry.result));
      return timed(reply, entry.at);
    }
    case "tool":
      return toolNode(entry, taskId);
    case "failed": {
      const failed = timed(el("div", entry.reason, "msg error"), entry.at);
      if (entry.result) failed.append(resultNode(entry.result));
      return failed;
    }
  }
}

function resultNode(text: string): HTMLElement {
  const result = el("p", `Recorded result: ${text}`, "runtime-result");
  result.setAttribute("aria-label", "Recorded result");
  return result;
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
  await refreshContinuity();
  taskList.hidden = true;
  taskDetail.hidden = false;
  taskDetail.dataset.task = String(id);
  // What to do with the task is in More, at the top with Back and the title, so a long
  // chat need not be scrolled to its end first; the chat opens at its end anyway. What
  // came of a choice shows under the chat.
  const status = el("p", undefined, "meta note");
  const waitForAnswer = (b: HTMLButtonElement) => {
    b.disabled = answering || task.running;
    return b;
  };
  const choices: HTMLButtonElement[] = [];
  // The conversation on screen is this task's, approval cards and all, and the next
  // message goes on with it.
  if (!task.owned) {
    status.textContent = "This task is owned by your paired device. Move it back from there to continue here.";
  } else if (task.id === currentTask && (task.running || task.page === "doing")) {
    choices.push(makeButton("Back to the conversation", () => showTasks(false)));
  } else if (task.page === "done") {
    choices.push(waitForAnswer(makeButton("Follow up", () => followUp(task.id, status))));
  } else {
    choices.push(waitForAnswer(makeButton("Continue", () => continueTask(task.id, status))));
  }
  if (task.owned && !task.finished) choices.push(waitForAnswer(makeButton("Finish task", () => finishTask(task.id, status))));
  if (task.running) choices.push(makeButton("Stop", () => stopTask(task.id)));
  if (task.owned && task.retryable) choices.push(waitForAnswer(makeButton("Retry last request", () => retryTask(task.id, status))));
  if (!task.running) choices.push(...continuityChoices("task", id, () => { void openTask(id); }));
  choices.push(makeButton("Export", () => exportTask(task.id, status)));
  choices.push(waitForAnswer(makeButton("Delete", () => deleteTask(task.id, status), "danger")));
  const top = el("div", undefined, "top");
  top.append(iconButton("Back", BACK_ARROW, showList), el("h2", task.title), ...moreMenu(choices));

  const body = el("div", undefined, "body");
  body.append(el("p", task.line, "meta task-state"));
  if (task.save_error) {
    body.append(el("p", task.save_error, "error-text"));
    body.append(waitForAnswer(makeButton("Retry saving", async () => {
      try { await invoke("retry_task_save"); await openTask(task.id); }
      catch (err) { status.textContent = String(err); }
    }, "secondary")));
  }
  if (task.owned && task.retryable) {
    body.append(waitForAnswer(makeButton("Retry last request", () => retryTask(task.id, status), "secondary")));
  }
  const parent = task.parent;
  if (parent) body.append(makeButton(`Follows up: ${parent.title}`, () => openTask(parent.id), "secondary small"));
  const chat = el("div", undefined, "chat");
  chat.append(...task.chat.map(entry => entryNode(entry, task.id)));
  markDays(chat);
  body.append(chat, status);
  taskDetail.replaceChildren(top, body);
  // At its end as it opens, the chat stays there while it settles - as text reflows once
  // a font arrives - until the user scrolls it.
  let atChatEnd = true;
  body.addEventListener("scroll", () => {
    atChatEnd = body.scrollHeight - body.scrollTop - body.clientHeight < 8;
    markEnd();
  });
  const settle = new ResizeObserver(() => {
    if (atChatEnd) body.scrollTop = body.scrollHeight;
    markEnd();
  });
  settle.observe(chat);
  settle.observe(body);
  body.scrollTop = body.scrollHeight;
  markEnd();
}

function refreshTaskAfterTurn(): void {
  if (tasksSection.hidden) return;
  if (!taskList.hidden) renderTasks();
  else if (taskDetail.dataset.task) openTask(Number(taskDetail.dataset.task));
}

// What came of a choice in a task's More - where its export went, or why a choice
// failed - under the task's chat.
function say(status: HTMLElement, text: string): void {
  // A reply in the conversation keeps its time.
  const time = status.querySelector(":scope > time");
  status.textContent = text;
  if (time) status.append(time);
  const body = status.parentElement;
  if (body) body.scrollTop = body.scrollHeight;
}

// Closes the menu that is open, if one is.
let closeMenu: (() => void) | null = null;

// More (⋮) and the menu it opens under it. Choosing, tapping anywhere else or Escape
// closes the menu.
function moreMenu(items: HTMLButtonElement[]): HTMLElement[] {
  const menu = el("div", undefined, "menu");
  menu.setAttribute("role", "menu");
  menu.hidden = true;
  const more = iconButton("More", MORE_DOTS, () => {
    if (menu.hidden) open();
    else closeMenu?.();
  });
  more.setAttribute("aria-haspopup", "menu");
  more.setAttribute("aria-expanded", "false");
  const open = () => {
    closeMenu?.();
    menu.hidden = false;
    more.setAttribute("aria-expanded", "true");
    closeMenu = () => {
      menu.hidden = true;
      more.setAttribute("aria-expanded", "false");
      closeMenu = null;
    };
  };
  for (const item of items) {
    item.setAttribute("role", "menuitem");
    item.addEventListener("click", () => closeMenu?.());
  }
  menu.append(...items);
  return [more, menu];
}

document.addEventListener(
  "pointerdown",
  ({ target }) => {
    const inMenu = target instanceof Element && target.closest(".menu, [aria-haspopup=menu]");
    if (!inMenu) closeMenu?.();
  },
  true,
);

// Finishing preserves the chat and records the user's closure, without executing work.
async function finishTask(id: number, status: HTMLElement): Promise<void> {
  try {
    const card = await invoke<TaskCard>("finish_task", { id });
    finishedTask(card);
    if (!tasksSection.hidden && !taskDetail.hidden && taskDetail.dataset.task === String(id)) {
      await openTask(id);
    } else {
      const line = `Finished “${card.title}”. Closing the task did not send anything.`;
      if (status.isConnected) say(status, line);
      else addToLog(taskAnswer(line, [card], openTasks));
    }
  } catch (err) {
    say(status, String(err));
  }
}

function finishedTask(card: TaskCard): void {
  if (currentTask === card.id) {
    currentTask = null;
    foldConversation(card);
  }
  const fold = log.querySelector<HTMLElement>(`button.fold[data-task="${card.id}"]`);
  if (fold) fold.textContent = `${card.title} - in Done`;
  page = "done";
  renderTasks();
}

// Deleting cannot be undone, so the user is asked first.
async function deleteTask(id: number, status: HTMLElement): Promise<void> {
  deleteDialog.returnValue = "";
  deleteDialog.showModal();
  await new Promise((closed) => deleteDialog.addEventListener("close", closed, { once: true }));
  if (deleteDialog.returnValue !== "delete") return;
  try {
    await invoke("delete_task", { id });
  } catch (err) {
    say(status, String(err));
    return;
  }
  log.querySelector(`button.fold[data-task="${id}"]`)?.remove();
  if (currentTask === id) {
    currentTask = null;
    foldConversation(null);
  }
  showList();
}

// A tap on the dim around the dialog cancels it, as Escape does.
deleteDialog.addEventListener("click", ({ target }) => {
  if (target === deleteDialog) deleteDialog.close();
});

// The task's chat as Markdown and its record and events as JSON, handed to the share
// sheet; on a computer, saved where the user picks. `status` says where they went.
async function exportTask(id: number, status: HTMLElement): Promise<void> {
  const utcOffsetMinutes = -new Date().getTimezoneOffset();
  try {
    const saved = await invoke<string | null>("export_task", { id, utcOffsetMinutes });
    if (saved) say(status, saved);
  } catch (err) {
    say(status, String(err));
  }
}

async function continueTask(id: number, status: HTMLElement): Promise<void> {
  let next: Continued;
  try {
    next = await invoke<Continued>("continue_task", { id });
  } catch (err) {
    say(status, String(err));
    return;
  }
  backTo(id, next);
}

// The conversation goes back to a task: what was on screen folds away and its chat
// comes back, then `after` (the words that asked for it), then its confirmation card if
// it was waiting on one.
function backTo(id: number, next: Continued, after: Node[] = []): void {
  log.querySelector(`button.fold[data-task="${id}"]`)?.remove();
  log.querySelector("button.fold.following")?.remove();
  foldConversation(next.previous);
  for (const entry of next.task.chat) addToLog(entryNode(entry));
  for (const node of after) addToLog(node);
  currentTask = id;
  showTasks(false);
  if (next.task.waiting) askAgain(id);
}

async function followUp(id: number, status: HTMLElement): Promise<void> {
  let next: FollowingUp;
  try {
    next = await invoke<FollowingUp>("follow_up_task", { id });
  } catch (err) {
    say(status, String(err));
    return;
  }
  followingUp(id, next);
}

// The next message follows up a finished task: what was on screen folds away under a
// "Following up" line that opens it, then `after` shows.
function followingUp(id: number, next: FollowingUp, after: Node[] = []): void {
  foldConversation(next.previous);
  currentTask = null;
  log.querySelector("button.fold.following")?.remove();
  const line = addFold(id, `Following up: ${next.parent.title}`);
  line.classList.add("following");
  line.dataset.follows = "yes";
  for (const node of after) addToLog(node);
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

// Takes the conversation off the home page up to `from` (the message that opened the
// next task), and folds the task it belonged to into one line that opens it.
function foldConversation(previous: TaskCard | null, from?: Element): void {
  for (const node of [...log.children]) {
    if (node === from) break;
    if (!node.classList.contains("fold")) node.remove();
  }
  // A follow-up's "Following up" line goes with its conversation; the task keeps the link.
  for (const line of log.querySelectorAll("button.fold[data-follows]:not(.following)")) line.remove();
  if (previous) addFold(previous.id, `${previous.title} - in ${PAGE_NAMES[previous.page]}`);
  markDays(log);
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

// The conversation the app closed on comes back while it is still going, and a
// confirmation card it was waiting on is asked again.
async function restoreConversation(): Promise<void> {
  const task = await invoke<TaskView | null>("current_task").catch(() => null);
  if (!task) return;
  currentTask = task.id;
  for (const entry of task.chat) addToLog(entryNode(entry));
  if (task.waiting) askAgain(task.id);
}

// The model loads in the background; refresh the Models tab when it changes.
window.__TAURI__.event.listen("model-changed", () => {
  if (showing("models")) renderModels();
});
markPlatform();
setUpSpeech();
restoreConversation();
