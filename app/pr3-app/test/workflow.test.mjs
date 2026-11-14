// Native WKWebView/WebKitGTK/WebView2 -> production Tauri IPC -> Host -> SQLite.
// Model/native-device adapters are fixtures; there is no command mocking here.
import assert from 'node:assert/strict';
import { afterEach, beforeEach, test } from 'node:test';
import { spawn, execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { once } from 'node:events';
import { createServer } from 'node:net';
import { mkdtemp, writeFile, readFile, mkdir, open, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { remote } from 'webdriverio';

const binary = resolve('../../target/debug/pr3-e2e');
const artifacts = resolve('test-results');
let dir, child, driver, logFile;
let scenario = 0;
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));

async function launch() {
  const socket = createServer();
  socket.listen(0, '127.0.0.1');
  await once(socket, 'listening');
  const port = socket.address().port;
  await new Promise(resolve => socket.close(resolve));
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
  await driver.setTimeout({ implicit: 0, script: 10000 });
  await driver.$('#text').waitForDisplayed({ timeout: 15000 });
  assert.match((await invoke('platform_info')).model, /E2E scripted model/);
}

async function stop() {
  if (driver) { await driver.deleteSession().catch(() => {}); driver = undefined; }
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
  if (context.error && driver) {
    await driver.saveScreenshot(resolve(artifacts, `${scenario}-failure.png`)).catch(() => {});
    await writeFile(resolve(artifacts, `${scenario}-page.html`), await driver.getPageSource()).catch(() => {});
    const layout = await driver.execute(() => ({
      visibility: document.visibilityState,
      settings: getComputedStyle(document.querySelector('#settings')).transform,
      animations: document.getAnimations().map(a => ({
        name: a.animationName, state: a.playState, time: a.currentTime,
      })),
    })).catch(() => null);
    await writeFile(resolve(artifacts, `${scenario}-layout.json`), JSON.stringify(layout, null, 2)).catch(() => {});
  }
  await stop();
  if (!context.error) await rm(dir, { recursive: true, force: true });
  else console.error(`Failed test data retained at ${dir}`);
});

// Read-only assertions through the same IPC used by the UI. All workflow actions
// below (including approval, retry and deletion) are clicks or typed messages.
async function invoke(command) {
  return driver.execute(async name => {
    return window.__TAURI__.core.invoke(name);
  }, command);
}
async function click(selector) {
  const split = selector.indexOf(' button=');
  const element = split < 0 ? await driver.$(selector)
    : await (await driver.$(selector.slice(0, split))).$(selector.slice(split + 1));
  await element.waitForClickable({ timeout: 10000 });
  await element.click();
  await settingsAnimationDone();
}
async function textIncludes(selector, value) {
  await driver.waitUntil(async () => (await driver.$(selector).getText()).includes(value), { timeout: 10000, timeoutMsg: `Expected ${selector} to include ${value}` });
}
async function idle() {
  await driver.waitUntil(async () => (await driver.$('#composer button[type=submit]').isEnabled()), { timeout: 15000, timeoutMsg: 'Composer did not return to idle' });
}
async function fixtureReached(name) {
  await driver.waitUntil(async () => {
    try { await readFile(resolve(dir, name)); return true; }
    catch (e) { if (e.code === 'ENOENT') return false; throw e; }
  }, { timeout: 5000, timeoutMsg: `Fixture did not reach ${name}` });
}
async function send(text, wait = true) {
  await driver.$('#text').setValue(text);
  await click('#composer button[type=submit]');
  if (wait) await idle();
}
async function settings(tab) {
  if (!(await driver.$('#settings').isDisplayed())) await click('#settings-button');
  await click(`[data-tab="${tab}"]`);
}
async function settingsAnimationDone() {
  await driver.execute(async () => {
    const animations = document.querySelector('#settings').getAnimations();
    // WKWebView freezes CSS animations at time zero when its native window is
    // occluded. Finish only this visual transition so UI clicks remain testable
    // on unattended runners; commands, events and workflow state stay real.
    if (document.hidden) for (const animation of animations) animation.finish();
    await Promise.all(animations.map(animation => animation.finished.catch(() => {})));
  });
}
async function storageFault(sql) {
  // Fault injection only in the runner's marked private database, never user data.
  await promisify(execFile)('python3', ['-c', 'import sqlite3,sys; db=sqlite3.connect(sys.argv[1]); db.executescript(sys.argv[2]); db.close()', resolve(dir, 'memory.sqlite'), sql]);
}

test('encrypted storage locks on key loss and recovers without resetting memories', { timeout: 90000 }, async context => {
  await settings('memory');
  await driver.$('[aria-label="New memory"]').setValue('private encrypted memory 8392');
  await click('#memory button=Add');
  await textIncludes('#memory', 'private encrypted memory 8392');
  await stop();
  await writeFile(resolve(dir, '.encrypted-storage'), 'synthetic encryption fixture\n');
  await launch();
  await settings('storage');
  await textIncludes('#storage', 'encrypted on this device');
  await settingsAnimationDone();
  await driver.saveScreenshot(resolve(artifacts, 'storage-encrypted.png'));
  await click('#storage button=Show recovery code');
  if (await driver.execute(() => document.hidden)) {
    const showCode = await (await driver.$('#storage')).$('button=Show recovery code');
    await driver.waitUntil(async () => showCode.isEnabled());
    assert.equal(await driver.$('#recovery-code').getValue(), '');
    assert.equal(await driver.$('#recovery-code-dialog').isDisplayed(), false);
    context.skip('Recovery code correctly withheld: this workflow needs a visible, unlocked desktop session');
    return;
  }
  await driver.$('#recovery-code-dialog').waitForDisplayed();
  const code = await driver.$('#recovery-code').getValue();
  assert.match(code, /^PR3(?:-[0-9a-f]{4}){16}$/);
  await click('#recovery-code-dialog button=Done');
  await driver.waitUntil(async () => (await driver.$('#recovery-code').getValue()) === '');
  await stop();
  const before = await readFile(resolve(dir, 'memory.sqlite'));
  assert.equal(before.includes(Buffer.from('private encrypted memory 8392')), false);
  await rm(resolve(dir, 'fixture-storage.key'));
  await launch();
  await driver.$('#storage-warning').waitForDisplayed();
  assert.equal(await driver.$('#text').isEnabled(), false);
  const rejection = await driver.execute(async () => {
    try { await window.__TAURI__.core.invoke('send_message', {
      text: 'hello', utcOffsetMinutes: 0, onEvent: new window.__TAURI__.core.Channel(),
    }); return 'unexpectedly allowed'; } catch (e) { return String(e); }
  });
  assert.match(rejection, /Storage is locked/);
  await click('#storage-open');
  await textIncludes('#storage', 'Your saved data is locked');
  await settingsAnimationDone();
  await driver.saveScreenshot(resolve(artifacts, 'storage-locked.png'));
  await driver.$('#storage [aria-label="Recovery code"]').setValue('PR3-' + '0000-'.repeat(15) + '0000');
  await click('#storage button=Recover access');
  await textIncludes('#storage [role=status]', 'locked or damaged');
  assert.deepEqual(await readFile(resolve(dir, 'memory.sqlite')), before);
  await driver.$('#storage [aria-label="Recovery code"]').setValue(code);
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
  await click(`button[data-page="${cards[0].page}"]`);
  await click(`.page[data-page="${cards[0].page}"] .task-card`);
}

test('Stop leaves a recoverable task and only explicit Retry resumes it', { timeout: 45000 }, async () => {
  await send('Slow request', false);
  await fixtureReached('slow-request-started');
  await click('#stop-task'); await idle();
  const before = await invoke('current_task');
  assert.equal(before.retryable, true);
  assert.equal(before.page, 'todo');
  await taskDetail();
  await click('#task-detail .body button=Retry last request');
  await idle();
  const after = await invoke('current_task');
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
      assert.equal(await driver.$('#stop-task').isEnabled(), false);
    } finally {
      await rm(resolve(dir, 'hold-turn-events'), { force: true });
    }
    await idle();
    await textIncludes('#log', 'Stopped by you');
    const stopped = await invoke('current_task');
    assert.equal(stopped.retryable, true);
    assert.equal(stopped.running, false);
    if (existing) assert.equal(stopped.id, previous.id);
    await send('Continue after stopping.');
    await textIncludes('#log', 'We can continue with: Continue after stopping.');
    assert.equal((await invoke('current_task')).id, stopped.id);
    });
}

test('corrected memory survives restart; Forget requires confirmation and stays forgotten', { timeout: 60000 }, async () => {
  await settings('memory');
  await driver.$('[aria-label="New memory"]').setValue('I live in Oslo.');
  await click('#memory button=Add');
  await textIncludes('#memory', 'I live in Oslo.');
  await click('#memory .item button=Edit');
  await driver.$('[aria-label=Memory]').setValue('I live in Lisbon.');
  await click('#memory .item button=Save');
  await textIncludes('#memory', 'I live in Lisbon.');
  await stop(); await launch();
  await settings('memory');
  await textIncludes('#memory', 'I live in Lisbon.');
  assert.doesNotMatch(await driver.$('#memory').getText(), /I live in Oslo/);
  await click('#memory .item button=Forget');
  await click('#remove-item-dialog button[value=cancel]');
  assert.equal((await invoke('list_memories')).length, 1);
  await click('#memory .item button=Forget');
  assert.equal(await driver.$('#remove-item-action').getText(), 'Forget');
  assert.match(await driver.$('#remove-item-action').getAttribute('class'), /danger/);
  await click('#remove-item-action');
  await driver.waitUntil(async () => (await invoke('list_memories')).length === 0);
  await stop(); await launch();
  assert.deepEqual(await invoke('list_memories'), []);
});

// File selection is a fixture; change -> FileReader -> real import IPC -> encrypted
// library is the production path. OS picker UI is covered by device testing.
async function addDocument(name, text) {
  await settings('documents');
  await driver.execute((name, text) => {
    const selected = new DataTransfer(); selected.items.add(new File([text], name, { type: 'text/plain' }));
    const input = document.querySelector('#documents input[type=file]'); input.files = selected.files;
    input.dispatchEvent(new Event('change', { bubbles: true }));
  }, name, text);
  await textIncludes('#documents', name);
}
test('documents search, verified answer, restart, source inspection and deletion', { timeout: 90000 }, async () => {
  assert.equal(await driver.$('[data-tab=email]').isExisting(), false);
  await addDocument('permit.md', 'The permit code is JN-371.');
  await driver.$('[aria-label="Search documents"]').setValue('permit code');
  await click('#documents button=Search');
  await textIncludes('#documents .document-passage', 'JN-371');
  await driver.saveScreenshot(resolve(artifacts, 'documents.png'));
  await click('#documents button=Ask PR3'); await idle();
  await textIncludes('#log', 'From your documents:');
  assert.doesNotMatch(await driver.$('#log').getText(), /MADE-UP|sent everything/);
  await stop(); await launch();
  await textIncludes('#log', 'JN-371');
  await click('#log button=View source document:1:1');
  await textIncludes('#document-source-body', 'The permit code is JN-371.');
  await click('#document-source-dialog button=Close');
  await settings('documents'); await click('#documents .document-row button=Remove');
  await click('#remove-item-dialog button[value=cancel]');
  assert.equal((await invoke('list_documents')).length, 1);
  await click('#documents .document-row button=Remove');
  assert.equal(await driver.$('#remove-item-action').getText(), 'Remove');
  assert.match(await driver.$('#remove-item-action').getAttribute('class'), /danger/);
  await click('#remove-item-action');
  await driver.waitUntil(async () => (await invoke('list_documents')).length === 0);
  assert.deepEqual(await invoke('list_tasks'), []);
  await stop(); await launch();
  assert.deepEqual(await invoke('list_documents'), []);
  assert.doesNotMatch(await driver.$('#log').getText(), /JN-371/);
});
