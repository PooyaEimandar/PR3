// Native WKWebView/WebKitGTK/WebView2 -> production Tauri IPC -> Host -> SQLite.
// Model/native-device adapters are fixtures; there is no command mocking here.
import assert from 'node:assert/strict';
import { afterEach, beforeEach, test } from 'node:test';
import { spawn, type ChildProcess } from 'node:child_process';
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
  research_settings: { provider: 'tavily' | 'brave'; configured: boolean; web_enabled: boolean };
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
    const layout = await browser().execute(() => ({
      visibility: document.visibilityState,
      settings: getComputedStyle(document.querySelector('#settings') ?? document.documentElement).transform,
      animations: document.getAnimations().map(a => ({
        name: a instanceof CSSAnimation ? a.animationName : undefined, state: a.playState, time: a.currentTime,
      })),
    })).catch(() => null);
    await writeFile(resolve(artifacts, `${scenario}-layout.json`), JSON.stringify(layout, null, 2)).catch(() => {});
  }
  await stop();
  if (!failed) await rm(dir, { recursive: true, force: true });
  else console.error(`Failed test data retained at ${dir}`);
});

// Read-only assertions through the same IPC used by the UI. All workflow actions
// below (including approval, retry and deletion) are clicks or typed messages;
// file and native select inputs use DOM events with the production handlers.
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
  await settingsAnimationDone();
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
    const animations = settings.getAnimations();
    // WKWebView freezes CSS animations at time zero when its native window is
    // occluded. Finish only this visual transition so UI clicks remain testable
    // on unattended runners; commands, events and workflow state stay real.
    if (document.hidden) for (const animation of animations) animation.finish();
    await Promise.all(animations.map(animation => animation.finished.catch(() => {})));
  });
}


test('encrypted storage locks on key loss and recovers without resetting memories', { timeout: 90000 }, async context => {
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
  if (await browser().execute(() => document.hidden)) {
    const showCode = await (await browser().$('#storage')).$('button=Show recovery code');
    await browser().waitUntil(async () => showCode.isEnabled());
    assert.equal(await browser().$('#recovery-code').getValue(), '');
    assert.equal(await browser().$('#recovery-code-dialog').isDisplayed(), false);
    context.skip('Recovery code correctly withheld: this workflow needs a visible, unlocked desktop session');
    return;
  }
  await browser().$('#recovery-code-dialog').waitForDisplayed();
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
    });
}

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

// File selection is a fixture; change -> FileReader -> real import IPC -> encrypted
// library is the production path. OS picker UI is covered by device testing.
async function addDocument(name: string, text: string) {
  await settings('documents');
  await browser().execute((name, text) => {
    const selected = new DataTransfer(); selected.items.add(new File([text], name, { type: 'text/plain' }));
    const input = document.querySelector<HTMLInputElement>('#documents input[type=file]');
    if (!input) throw new Error('Document picker is missing');
    input.files = selected.files;
    input.dispatchEvent(new Event('change', { bubbles: true }));
  }, name, text);
  await textIncludes('#documents', name);
}
test('documents search, verified answer, restart, source inspection and deletion', { timeout: 90000 }, async () => {
  assert.equal(await browser().$('[data-tab=email]').isExisting(), false);
  await addDocument('permit.md', 'The permit code is JN-371.');
  await browser().$('[aria-label="Search documents"]').setValue('permit code');
  await click('#documents button=Search');
  await textIncludes('#documents .document-passage', 'JN-371');
  await browser().saveScreenshot(resolve(artifacts, 'documents.png'));
  await click('#documents button=Ask PR3'); await idle();
  await textIncludes('#log', 'Research findings');
  assert.doesNotMatch(await browser().$('#log').getText(), /MADE-UP|sent everything/);
  await stop(); await launch();
  await textIncludes('#log', 'JN-371');
  await click('#log button=View source 1');
  await textIncludes('#document-source-body', 'The permit code is JN-371.');
  await click('#document-source-dialog button=Close');
  await settings('documents'); await click('#documents .document-row button=Remove');
  await click('#remove-item-dialog button[value=cancel]');
  assert.equal((await invoke('list_documents')).length, 1);
  await click('#documents .document-row button=Remove');
  assert.equal(await browser().$('#remove-item-action').getText(), 'Remove');
  assert.match(await browser().$('#remove-item-action').getAttribute('class') ?? '', /danger/);
  await click('#remove-item-action');
  await browser().waitUntil(async () => (await invoke('list_documents')).length === 0);
  assert.deepEqual(await invoke('list_tasks'), []);
  await stop(); await launch();
  assert.deepEqual(await invoke('list_documents'), []);
  assert.doesNotMatch(await browser().$('#log').getText(), /JN-371/);
});

test('research compares sources, restores the report and removes dependent tasks', { timeout: 90000 }, async () => {
  await addDocument('alpha.md', 'Alpha price is 25.');
  await addDocument('beta.md', 'Beta price is 40.');
  await click('#settings-close');
  assert.equal(await browser().$$('#research-button').length, 0);
  assert.equal(await browser().$$('#research-dialog').length, 0);
  assert.equal(await browser().$('#research-online').isSelected(), false);
  await send('Hello');
  const originalTask = (await currentTask()).id;
  await send('Compare these documents');
  assert.equal((await currentTask()).id, originalTask);
  await textIncludes('#log', 'Research findings');
  await textIncludes('#log', 'Alpha price is 25.');
  await textIncludes('#log', 'Beta price is 40.');
  assert.doesNotMatch(await browser().$('#log').getText(), /INVENTED|sent your files/);
  assert.equal((await invoke('list_tasks')).length, 1);
  await browser().saveScreenshot(resolve(artifacts, 'research.png'));
  await send('Hello again');
  await textIncludes('#log', 'We can continue with: Hello again');
  assert.equal((await currentTask()).id, originalTask);
  await stop(); await launch();
  await send('Continue with the saved sources');
  assert.equal((await currentTask()).id, originalTask);
  await textIncludes('#log', 'Research findings');
  const sourceButtons = await browser().$('#log').$$('button=View source 2');
  await sourceButtons[sourceButtons.length - 1]!.click();
  await textIncludes('#document-source-body', 'Beta price is 40.');
  await click('#document-source-dialog button=Close');
  await settings('documents');
  await click('#documents .document-row:last-child button=Remove');
  await click('#remove-item-action');
  await browser().waitUntil(async () => (await invoke('list_documents')).length === 1);
  assert.deepEqual(await invoke('list_tasks'), []);
  await stop(); await launch();
  assert.doesNotMatch(await browser().$('#log').getText(), /Alpha price|Beta price/);
});

test('inline source controls reject invalid domains without opening a task', { timeout: 30000 }, async () => {
  await send('Hello');
  const task = (await currentTask()).id;
  await click('#research-online');
  await click('#chat-sources summary');
  await browser().$('#research-websites').setValue('https://example.org');
  await send('Research prices');
  await textIncludes('#log', 'website domains');
  assert.equal((await currentTask()).id, task);
  assert.equal((await invoke('list_tasks')).length, 1);
  await browser().$('#research-websites').setValue('sqlite.org');
  await browser().saveScreenshot(resolve(artifacts, 'chat-sources.png'));
  await click('#research-online');
  await send('Hello again');
  await textIncludes('#log', 'We can continue with: Hello again');
  await send('New task');
  assert.equal(await browser().$('#research-online').isSelected(), false);
  assert.equal(await browser().$('#research-websites').getValue(), '');
});

test('search provider selection and separate keys survive restart and confirmed removal', { timeout: 90000 }, async () => {
  const chooseProvider = async (provider: 'tavily' | 'brave') => {
    await browser().$('#research-provider').waitForClickable();
    // The native driver's option.click() does not reliably change WKWebView's
    // select value. Supply the selection event; UI handlers and IPC stay real.
    await browser().execute((value) => {
      const select = document.querySelector<HTMLSelectElement>('#research-provider');
      if (!select || select.disabled) throw new Error('Search provider is not ready');
      select.value = value;
      select.dispatchEvent(new Event('change', { bubbles: true }));
    }, provider);
    await browser().waitUntil(async () => (await invoke('research_settings')).provider === provider);
    assert.equal(await browser().$$('#research-provider').length, 1, 'only the latest settings refresh is rendered');
  };
  await settings('web');
  assert.deepEqual(await invoke('research_settings'), { provider: 'tavily', configured: false, web_enabled: true });
  await textIncludes('#web', 'free Tavily account');
  await browser().$('[aria-label="Tavily API key"]').setValue('synthetic-tavily-fixture-key');
  await click('#web button=Save key');
  await textIncludes('#web', 'encrypted storage');
  assert.equal((await invoke('research_settings')).configured, false);
  await stop();
  await writeFile(resolve(dir, '.encrypted-storage'), 'synthetic encryption fixture\n');
  await launch(); await settings('web');
  await browser().$('[aria-label="Tavily API key"]').setValue('synthetic-tavily-fixture-key');
  await click('#web button=Save key');
  await textIncludes('#web', 'Tavily key saved on this device.');
  assert.equal(await browser().$('[aria-label="Tavily API key"]').getValue(), '');
  assert.deepEqual(await invoke('research_settings'), { provider: 'tavily', configured: true, web_enabled: true });
  await chooseProvider('brave');
  await textIncludes('#web', 'Without a key for this provider');
  await browser().$('[aria-label="Brave Search API key"]').setValue('synthetic-brave-fixture-key');
  await click('#web button=Save key');
  await textIncludes('#web', 'Brave Search key saved on this device.');
  await stop(); await launch();
  const database = await readFile(resolve(dir, 'memory.sqlite'));
  assert.equal(database.includes(Buffer.from('synthetic-tavily-fixture-key')), false);
  assert.equal(database.includes(Buffer.from('synthetic-brave-fixture-key')), false);
  assert.deepEqual(await invoke('research_settings'), { provider: 'brave', configured: true, web_enabled: true });
  await click('#research-online');
  await textIncludes('#research-scope', 'Brave Search');
  await settings('web');
  await chooseProvider('tavily');
  await textIncludes('#web', 'Tavily key saved on this device.');
  await browser().saveScreenshot(resolve(artifacts, 'search-settings.png'));
  await click('#settings-close');
  await textIncludes('#research-scope', 'Tavily');
  await settings('web');
  await click('#web button=Remove key');
  await textIncludes('#remove-item-dialog', 'Remove Tavily key?');
  assert.match(await browser().$('#remove-item-action').getAttribute('class') ?? '', /danger/);
  await click('#remove-item-dialog button[value=cancel]');
  assert.equal((await invoke('research_settings')).configured, true);
  await click('#web button=Remove key'); await click('#remove-item-action');
  await textIncludes('#web', 'Without a key for this provider');
  await click('#settings-close');
  await textIncludes('#research-scope', 'No Tavily key');
  await stop(); await launch();
  assert.deepEqual(await invoke('research_settings'), { provider: 'tavily', configured: false, web_enabled: true });
  await settings('web');
  await chooseProvider('brave');
  await textIncludes('#web', 'Brave Search key saved on this device.');
  await click('#web input[type=checkbox]');
  await browser().waitUntil(async () => !(await invoke('research_settings')).web_enabled);
  await click('#settings-close'); await click('#research-online');
  await textIncludes('#research-scope', 'Web is off in Settings');
});
