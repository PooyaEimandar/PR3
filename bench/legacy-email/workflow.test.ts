// Native WKWebView/WebKitGTK/WebView2 -> production Tauri IPC -> Host -> SQLite.
// Model/native-device adapters are fixtures; there is no command mocking here.
import assert from 'node:assert/strict';
import { afterEach, beforeEach, test } from 'node:test';
import { spawn, execFile, type ChildProcess } from 'node:child_process';
import { promisify } from 'node:util';
import { once } from 'node:events';
import { createServer } from 'node:net';
import { mkdtemp, writeFile, readFile, mkdir, open, rm, type FileHandle } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { remote } from 'webdriverio';

const binary = resolve('../../target/debug/pr3-e2e');
const artifacts = resolve('test-results');
let dir: string;
let child: ChildProcess | undefined;
let driver: Awaited<ReturnType<typeof remote>> | undefined;
let logFile: FileHandle | undefined;
function browser(): Awaited<ReturnType<typeof remote>> {
  assert.ok(driver, 'The native WebDriver session must be running');
  return driver;
}
interface TaskCard { id: number; page: 'todo' | 'doing' | 'done' }
interface TaskView extends TaskCard {
  retryable: boolean;
  running: boolean;
  finished: boolean;
  waiting: boolean;
  save_error: string | null;
}
interface ReadResults {
  platform_info: { model: string };
  current_task: TaskView | null;
  list_tasks: TaskCard[];
  list_memories: unknown[];
  list_documents: unknown[];
  contacts_info: { on: boolean; count: number };
}
let scenario = 0;
const pause = (ms: number) => new Promise<void>(resolve => setTimeout(resolve, ms));

async function launch() {
  const socket = createServer();
  socket.listen(0, '127.0.0.1');
  await once(socket, 'listening');
  const address = socket.address();
  assert.ok(address && typeof address !== 'string', 'Expected a TCP listener');
  const port = address.port;
  await new Promise<void>((resolve, reject) => socket.close(error => error ? reject(error) : resolve()));
  logFile = await open(resolve(artifacts, `${scenario}-app.log`), 'a');
  child = spawn(binary, [], {
    env: { ...process.env, PR3_E2E_DIR: dir, TAURI_WEBDRIVER_PORT: String(port) },
    stdio: ['ignore', logFile.fd, logFile.fd],
  });
  let ready = false;
  for (let n = 0; n < 200; n++) {
    if (child.exitCode !== null) throw new Error(`E2E app exited: ${child.exitCode}; see ${artifacts}`);
    try { ready = (await fetch(`http://127.0.0.1:${port}/status`)).ok; } catch {}
    if (ready) break;
    await pause(100);
  }
  assert.ok(ready, 'native WebDriver did not start');
  driver = await remote({ hostname: '127.0.0.1', port, capabilities: { browserName: 'tauri' }, logLevel: 'error', connectionRetryCount: 0 });
  await browser().setTimeout({ implicit: 0, script: 10000 });
  await browser().$('#text').waitForDisplayed({ timeout: 15000 });
  assert.match((await invoke('platform_info')).model, /E2E scripted model/);
}

async function stop() {
  if (driver) { await browser().deleteSession().catch(() => {}); driver = undefined; }
  if (child && child.exitCode === null) {
    const exited = once(child, 'exit');
    child.kill('SIGKILL'); // A real process loss, not just a webview reload.
    await exited;
  }
  child = undefined;
  await logFile?.close(); logFile = undefined;
}

beforeEach(async () => {
  scenario++;
  await mkdir(artifacts, { recursive: true });
  dir = await mkdtemp(resolve(tmpdir(), 'pr3-native-e2e-'));
  await writeFile(resolve(dir, '.pr3-e2e'), 'isolated workflow fixture\n');
  await launch();
});

afterEach(async context => {
  const failed = 'error' in context && Boolean(context.error);
  if (failed && driver) {
    await browser().saveScreenshot(resolve(artifacts, `${scenario}-failure.png`)).catch(() => {});
    await writeFile(resolve(artifacts, `${scenario}-page.html`), await browser().getPageSource()).catch(() => {});
  }
  await stop();
  if (!failed) await rm(dir, { recursive: true, force: true });
  else console.error(`Failed test data retained at ${dir}`);
});

// Read-only assertions through the same IPC used by the UI. All workflow actions
// below (including approval, retry and deletion) are clicks or typed messages.
async function invoke<K extends keyof ReadResults>(command: K): Promise<ReadResults[K]> {
  const result: unknown = await browser().execute(async (name: string) => {
    return window.__TAURI__.core.invoke<unknown>(name);
  }, String(command));
  return result as ReadResults[K];
}
async function currentTask(): Promise<TaskView> {
  const task = await invoke('current_task');
  assert.ok(task, 'Expected an active task');
  return task;
}
async function click(selector: string) {
  const split = selector.indexOf(' button=');
  const element = split < 0 ? await browser().$(selector)
    : await (await browser().$(selector.slice(0, split))).$(selector.slice(split + 1));
  await element.waitForClickable({ timeout: 10000 });
  await element.click();
}
async function textIncludes(selector: string, value: string) {
  await browser().waitUntil(async () => (await browser().$(selector).getText()).includes(value), { timeout: 10000, timeoutMsg: `Expected ${selector} to include ${value}` });
}
async function idle() {
  await browser().waitUntil(async () => (await browser().$('#composer button[type=submit]').isEnabled()), { timeout: 15000, timeoutMsg: 'Composer did not return to idle' });
}
async function fixtureReached(name: string) {
  await browser().waitUntil(async () => {
    try { await readFile(resolve(dir, name)); return true; }
    catch (e) { if (e instanceof Error && 'code' in e && e.code === 'ENOENT') return false; throw e; }
  }, { timeout: 5000, timeoutMsg: `Fixture did not reach ${name}` });
}
async function send(text: string, wait = true) {
  await browser().$('#text').setValue(text);
  await click('#composer button[type=submit]');
  if (wait) await idle();
}
async function settings(tab: string) {
  if (!(await browser().$('#settings').isDisplayed())) await click('#settings-button');
  await click(`[data-tab="${tab}"]`);
}
async function settingsAnimationDone() {
  await browser().execute(async () => {
    const settings = document.querySelector('#settings');
    if (!settings) throw new Error('Settings panel is missing');
    await Promise.all(settings.getAnimations().map(animation => animation.finished.catch(() => {})));
  });
}
async function syncContacts() {
  await settings('email');
  await click('#email input[role=switch]');
  await browser().waitUntil(async () => (await invoke('contacts_info')).on === true);
  await click('#settings-close');
  await browser().waitUntil(async () => !(await browser().$('#settings').isDisplayed()));
}
async function capture() {
  await send('Prepare meeting details for Alex at 14:00.');
  await send('The meeting moved to 15:00.');
}
async function approval() {
  await send('Email Alex the meeting details.', false);
  await browser().$('.approval:not(.answered)').waitForDisplayed({ timeout: 10000 });
  const card = await browser().$('.approval:not(.answered)').getText();
  assert.match(card, /alex@example.com/);
  assert.match(card, /15:00/);
  assert.doesNotMatch(card, /14:00/);
}
interface Handoff { to: { name: string; address: string }[]; subject: string; body: string }
async function handoffs(): Promise<Handoff[]> {
  try { return (await readFile(resolve(dir, 'handoffs.jsonl'), 'utf8')).trim().split('\n').filter(Boolean).map(line => JSON.parse(line) as Handoff); }
  catch (e) { if (e instanceof Error && 'code' in e && e.code === 'ENOENT') return []; throw e; }
}
async function storageFault(sql: string) {
  // Fault injection only in the runner's marked private database, never user data.
  await promisify(execFile)('python3', ['-c', 'import sqlite3,sys; db=sqlite3.connect(sys.argv[1]); db.executescript(sys.argv[2]); db.close()', resolve(dir, 'memory.sqlite'), sql]);
}

test('encrypted storage locks on key loss and recovers without resetting memories', { timeout: 90000 }, async () => {
  await settings('memory');
  await browser().$('[aria-label="New memory"]').setValue('private encrypted memory 8392');
  await click('#memory button=Add');
  await textIncludes('#memory', 'private encrypted memory 8392');
  await stop();
  await writeFile(resolve(dir, '.encrypted-storage'), 'synthetic encryption fixture\n');
  await launch();
  await settings('storage');
  await textIncludes('#storage', 'encrypted on this device');
  await settingsAnimationDone();
  await browser().saveScreenshot(resolve(artifacts, 'storage-encrypted.png'));
  await click('#storage button=Show recovery code');
  const code = await browser().$('#recovery-code').getValue();
  assert.match(code, /^PR3(?:-[0-9a-f]{4}){16}$/);
  await click('#recovery-code-dialog button=Done');
  await browser().waitUntil(async () => (await browser().$('#recovery-code').getValue()) === '');
  await stop();
  const before = await readFile(resolve(dir, 'memory.sqlite'));
  assert.equal(before.includes(Buffer.from('private encrypted memory 8392')), false);
  await rm(resolve(dir, 'fixture-storage.key'));
  await launch();
  await browser().$('#storage-warning').waitForDisplayed();
  assert.equal(await browser().$('#text').isEnabled(), false);
  const rejection = await browser().execute(async () => {
    try { await window.__TAURI__.core.invoke('send_message', {
      text: 'hello', utcOffsetMinutes: 0, onEvent: new window.__TAURI__.core.Channel(),
    }); return 'unexpectedly allowed'; } catch (e) { return String(e); }
  });
  assert.match(rejection, /Storage is locked/);
  await click('#storage-open');
  await textIncludes('#storage', 'Your saved data is locked');
  await settingsAnimationDone();
  await browser().saveScreenshot(resolve(artifacts, 'storage-locked.png'));
  await browser().$('#storage [aria-label="Recovery code"]').setValue('PR3-' + '0000-'.repeat(15) + '0000');
  await click('#storage button=Recover access');
  await textIncludes('#storage [role=status]', 'locked or damaged');
  assert.deepEqual(await readFile(resolve(dir, 'memory.sqlite')), before);
  await browser().$('#storage [aria-label="Recovery code"]').setValue(code);
  await click('#storage button=Recover access');
  await textIncludes('#storage [role=status]', 'Close and reopen PR3');
  await stop(); await launch();
  await idle();
  await settings('memory');
  await textIncludes('#memory', 'private encrypted memory 8392');
  assert.deepEqual(await handoffs(), []);
});
async function taskDetail() {
  await click('#tasks-button');
  const cards = await invoke('list_tasks');
  assert.equal(cards.length, 1);
  const card = cards[0];
  assert.ok(card);
  await click(`button[data-page="${card.page}"]`);
  await click(`.page[data-page="${card.page}"] .task-card`);
}

test('capture, correct, restart, approve exact draft, explicitly finish and delete', { timeout: 90000 }, async () => {
  await syncContacts();
  await capture();
  const id = (await currentTask()).id;
  await stop(); await launch();
  await approval();
  assert.deepEqual(await handoffs(), []);
  await click('.approval button=Open email app');
  await idle();
  const mails = await handoffs();
  assert.equal(mails.length, 1);
  const mail = mails[0];
  assert.ok(mail);
  assert.deepEqual(mail.to, [{ name: 'Alex Rivera', address: 'alex@example.com' }]);
  assert.equal(mail.subject, 'Meeting');
  assert.equal(mail.body, 'The meeting is at 15:00.');
  const active = await currentTask();
  assert.equal(active.id, id);
  assert.equal(active.finished, false);
  await textIncludes('#log', 'email app');
  assert.doesNotMatch(await browser().$('#log').getText(), /I've sent your email|Everything is completed/);
  await taskDetail();
  await click('#task-detail [aria-label=More]');
  await click('#task-detail button=Finish task');
  await browser().waitUntil(async () => (await invoke('list_tasks'))[0]?.page === 'done');
  await stop(); await launch();
  await taskDetail();
  await click('#task-detail [aria-label=More]');
  await click('#task-detail button=Delete');
  await click('#delete-dialog button[value=cancel]');
  assert.equal((await invoke('list_tasks')).length, 1);
  await click('#task-detail [aria-label=More]');
  await click('#task-detail button=Delete');
  assert.match(await browser().$('#delete-dialog button[value=delete]').getAttribute('class') ?? '', /danger/);
  await click('#delete-dialog button[value=delete]');
  await browser().waitUntil(async () => (await invoke('list_tasks')).length === 0);
  await stop(); await launch();
  assert.deepEqual(await invoke('list_tasks'), []);
  assert.equal((await handoffs()).length, 1, 'restart/finish/delete must never resend');
});

test('declining and failed mail handoff never claim completion', { timeout: 60000 }, async () => {
  await syncContacts(); await capture(); await approval();
  await click('.approval button=Decline'); await idle();
  assert.deepEqual(await handoffs(), []);
  await textIncludes('#log', 'Declined');
  await writeFile(resolve(dir, 'mail.fail'), 'fail');
  await approval(); await click('.approval:not(.answered) button=Open email app'); await idle();
  assert.deepEqual(await handoffs(), []);
  const task = await currentTask();
  assert.equal(task.finished, false);
  assert.doesNotMatch(await browser().$('#log').getText(), /I've sent your email|Everything is completed/);
  await textIncludes('#log', 'Fixture mail app refused');
});

test('interrupted approval requires a fresh approval after restart and never replays a handoff', { timeout: 60000 }, async () => {
  await syncContacts(); await capture(); await approval();
  const before = await currentTask();
  assert.equal(before.waiting, true);
  await stop(); await launch();
  await browser().$('.approval:not(.answered)').waitForDisplayed({ timeout: 10000 });
  assert.deepEqual(await handoffs(), []);
  assert.equal((await currentTask()).id, before.id);
  await textIncludes('.approval:not(.answered)', '15:00');
  await click('.approval:not(.answered) button=Open email app'); await idle();
  assert.equal((await handoffs()).length, 1);
  await stop(); await launch();
  assert.equal((await handoffs()).length, 1);
  assert.equal((await currentTask()).waiting, false);
  assert.equal((await currentTask()).finished, false);
});

test('Stop leaves a recoverable task and only explicit Retry resumes it', { timeout: 45000 }, async () => {
  await send('Slow request', false);
  await fixtureReached('slow-request-started');
  await click('#stop-task'); await idle();
  const before = await currentTask();
  assert.equal(before.retryable, true);
  assert.equal(before.page, 'todo');
  await taskDetail();
  await click('#task-detail .body button=Retry last request');
  await idle();
  const after = await currentTask();
  assert.equal(after.id, before.id);
  assert.equal(after.retryable, false);
  await textIncludes('#log', 'Recovered after explicit retry.');
  assert.deepEqual(await handoffs(), []);
});

for (const existing of [false, true]) {
  test(`Stop before turn registration is delivered (${existing ? 'existing task' : 'new task'})`, { timeout: 45000 }, async () => {
    if (existing) await send('Start a task.');
    const previous = await invoke('current_task');
    await writeFile(resolve(dir, 'hold-turn-events'), 'delay native event delivery');
    try {
      await send('Slow request', false);
      await fixtureReached('turn-event-held');
      await click('#stop-task');
      await textIncludes('#log', 'Stop requested. Waiting for the task to start.');
      assert.equal(await browser().$('#stop-task').isEnabled(), false);
    } finally {
      await rm(resolve(dir, 'hold-turn-events'), { force: true });
    }
    await idle();
    await textIncludes('#log', 'Stopped by you');
    const stopped = await currentTask();
    assert.equal(stopped.retryable, true);
    assert.equal(stopped.running, false);
    if (existing) { assert.ok(previous); assert.equal(stopped.id, previous.id); }
    await send('Continue after stopping.');
    await textIncludes('#log', 'We can continue with: Continue after stopping.');
    assert.equal((await currentTask()).id, stopped.id);
    assert.deepEqual(await handoffs(), []);
  });
}

test('Retry saving commits the known mail result without executing the action twice', { timeout: 60000 }, async () => {
  await syncContacts(); await capture(); await approval();
  await storageFault("CREATE TRIGGER fail_result BEFORE INSERT ON messages WHEN new.kind='tool' BEGIN SELECT RAISE(FAIL, 'storage full'); END;");
  await click('.approval:not(.answered) button=Open email app'); await idle();
  assert.equal((await handoffs()).length, 1);
  assert.ok((await currentTask()).save_error);
  await taskDetail();
  await textIncludes('#task-detail', 'Needs attention');
  await storageFault('DROP TRIGGER fail_result;');
  await click('#task-detail .body button=Retry saving');
  await browser().waitUntil(async () => !(await currentTask()).save_error);
  assert.equal((await handoffs()).length, 1);
  await stop(); await launch();
  const task = await currentTask();
  assert.equal(task.save_error, null);
  assert.equal(task.retryable, false);
  assert.equal(task.finished, false);
  assert.equal((await handoffs()).length, 1);
  await textIncludes('#log', 'email app');
});

test('corrected memory survives restart; Forget requires confirmation and stays forgotten', { timeout: 60000 }, async () => {
  await settings('memory');
  await browser().$('[aria-label="New memory"]').setValue('I live in Oslo.');
  await click('#memory button=Add');
  await textIncludes('#memory', 'I live in Oslo.');
  await click('#memory .item button=Edit');
  await browser().$('[aria-label=Memory]').setValue('I live in Lisbon.');
  await click('#memory .item button=Save');
  await textIncludes('#memory', 'I live in Lisbon.');
  await stop(); await launch();
  await settings('memory');
  await textIncludes('#memory', 'I live in Lisbon.');
  assert.doesNotMatch(await browser().$('#memory').getText(), /I live in Oslo/);
  await click('#memory .item button=Forget');
  await click('#remove-item-dialog button[value=cancel]');
  assert.equal((await invoke('list_memories')).length, 1);
  await click('#memory .item button=Forget');
  assert.equal(await browser().$('#remove-item-action').getText(), 'Forget');
  assert.match(await browser().$('#remove-item-action').getAttribute('class') ?? '', /danger/);
  await click('#remove-item-action');
  await browser().waitUntil(async () => (await invoke('list_memories')).length === 0);
  await stop(); await launch();
  assert.deepEqual(await invoke('list_memories'), []);
});

test('contact removal confirms in red and persists sync off', { timeout: 45000 }, async () => {
  await syncContacts(); await settings('email');
  await click('#email button=Remove all contacts');
  await click('#remove-contacts-dialog button[value=cancel]');
  assert.equal((await invoke('contacts_info')).on, true);
  await click('#email button=Remove all contacts');
  assert.match(await browser().$('#remove-contacts-dialog button[value=remove]').getAttribute('class') ?? '', /danger/);
  await click('#remove-contacts-dialog button[value=remove]');
  await browser().waitUntil(async () => (await invoke('contacts_info')).on === false);
  await stop(); await launch();
  assert.equal((await invoke('contacts_info')).on, false);
  assert.equal((await invoke('contacts_info')).count, 0);
});
