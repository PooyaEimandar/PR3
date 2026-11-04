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
}
async function textIncludes(selector, value) {
  await driver.waitUntil(async () => (await driver.$(selector).getText()).includes(value), { timeout: 10000, timeoutMsg: `Expected ${selector} to include ${value}` });
}
async function idle() {
  await driver.waitUntil(async () => (await driver.$('#composer button[type=submit]').isEnabled()), { timeout: 15000 });
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
async function syncContacts() {
  await settings('email');
  await click('#email input[role=switch]');
  await driver.waitUntil(async () => (await invoke('contacts_info')).on === true);
  await click('#settings-close');
  await driver.waitUntil(async () => !(await driver.$('#settings').isDisplayed()));
}
async function capture() {
  await send('Prepare meeting details for Alex at 14:00.');
  await send('The meeting moved to 15:00.');
}
async function approval() {
  await send('Email Alex the meeting details.', false);
  await driver.$('.approval:not(.answered)').waitForDisplayed({ timeout: 10000 });
  const card = await driver.$('.approval:not(.answered)').getText();
  assert.match(card, /alex@example.com/);
  assert.match(card, /15:00/);
  assert.doesNotMatch(card, /14:00/);
}
async function handoffs() {
  try { return (await readFile(resolve(dir, 'handoffs.jsonl'), 'utf8')).trim().split('\n').filter(Boolean).map(JSON.parse); }
  catch (e) { if (e.code === 'ENOENT') return []; throw e; }
}
async function storageFault(sql) {
  // Fault injection only in the runner's marked private database, never user data.
  await promisify(execFile)('python3', ['-c', 'import sqlite3,sys; db=sqlite3.connect(sys.argv[1]); db.executescript(sys.argv[2]); db.close()', resolve(dir, 'memory.sqlite'), sql]);
}
async function taskDetail() {
  await click('#tasks-button');
  const cards = await invoke('list_tasks');
  assert.equal(cards.length, 1);
  await click(`button[data-page="${cards[0].page}"]`);
  await click(`.page[data-page="${cards[0].page}"] .task-card`);
}

test('capture, correct, restart, approve exact draft, explicitly finish and delete', { timeout: 90000 }, async () => {
  await syncContacts();
  await capture();
  const id = (await invoke('current_task')).id;
  await stop(); await launch();
  await approval();
  assert.deepEqual(await handoffs(), []);
  await click('.approval button=Open email app');
  await idle();
  const mails = await handoffs();
  assert.equal(mails.length, 1);
  assert.deepEqual(mails[0].to, [{ name: 'Alex Rivera', address: 'alex@example.com' }]);
  assert.equal(mails[0].subject, 'Meeting');
  assert.equal(mails[0].body, 'The meeting is at 15:00.');
  const active = await invoke('current_task');
  assert.equal(active.id, id);
  assert.equal(active.finished, false);
  await textIncludes('#log', 'email app');
  assert.doesNotMatch(await driver.$('#log').getText(), /I've sent your email|Everything is completed/);
  await taskDetail();
  await click('#task-detail [aria-label=More]');
  await click('#task-detail button=Finish task');
  await driver.waitUntil(async () => (await invoke('list_tasks'))[0]?.page === 'done');
  await stop(); await launch();
  await taskDetail();
  await click('#task-detail [aria-label=More]');
  await click('#task-detail button=Delete');
  await click('#delete-dialog button[value=cancel]');
  assert.equal((await invoke('list_tasks')).length, 1);
  await click('#task-detail [aria-label=More]');
  await click('#task-detail button=Delete');
  assert.match(await driver.$('#delete-dialog button[value=delete]').getAttribute('class'), /danger/);
  await click('#delete-dialog button[value=delete]');
  await driver.waitUntil(async () => (await invoke('list_tasks')).length === 0);
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
  const task = await invoke('current_task');
  assert.equal(task.finished, false);
  assert.doesNotMatch(await driver.$('#log').getText(), /I've sent your email|Everything is completed/);
  await textIncludes('#log', 'Fixture mail app refused');
});

test('interrupted approval requires a fresh approval after restart and never replays a handoff', { timeout: 60000 }, async () => {
  await syncContacts(); await capture(); await approval();
  const before = await invoke('current_task');
  assert.equal(before.waiting, true);
  await stop(); await launch();
  await driver.$('.approval:not(.answered)').waitForDisplayed({ timeout: 10000 });
  assert.deepEqual(await handoffs(), []);
  assert.equal((await invoke('current_task')).id, before.id);
  await textIncludes('.approval:not(.answered)', '15:00');
  await click('.approval:not(.answered) button=Open email app'); await idle();
  assert.equal((await handoffs()).length, 1);
  await stop(); await launch();
  assert.equal((await handoffs()).length, 1);
  assert.equal((await invoke('current_task')).waiting, false);
  assert.equal((await invoke('current_task')).finished, false);
});

test('Stop leaves a recoverable task and only explicit Retry resumes it', { timeout: 45000 }, async () => {
  await send('Slow request', false);
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
  assert.deepEqual(await handoffs(), []);
});

test('Retry saving commits the known mail result without executing the action twice', { timeout: 60000 }, async () => {
  await syncContacts(); await capture(); await approval();
  await storageFault("CREATE TRIGGER fail_result BEFORE INSERT ON messages WHEN new.kind='tool' BEGIN SELECT RAISE(FAIL, 'storage full'); END;");
  await click('.approval:not(.answered) button=Open email app'); await idle();
  assert.equal((await handoffs()).length, 1);
  assert.ok((await invoke('current_task')).save_error);
  await taskDetail();
  await textIncludes('#task-detail', 'Needs attention');
  await storageFault('DROP TRIGGER fail_result;');
  await click('#task-detail .body button=Retry saving');
  await driver.waitUntil(async () => !(await invoke('current_task')).save_error);
  assert.equal((await handoffs()).length, 1);
  await stop(); await launch();
  const task = await invoke('current_task');
  assert.equal(task.save_error, null);
  assert.equal(task.retryable, false);
  assert.equal(task.finished, false);
  assert.equal((await handoffs()).length, 1);
  await textIncludes('#log', 'email app');
});

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

test('contact removal confirms in red and persists sync off', { timeout: 45000 }, async () => {
  await syncContacts(); await settings('email');
  await click('#email button=Remove all contacts');
  await click('#remove-contacts-dialog button[value=cancel]');
  assert.equal((await invoke('contacts_info')).on, true);
  await click('#email button=Remove all contacts');
  assert.match(await driver.$('#remove-contacts-dialog button[value=remove]').getAttribute('class'), /danger/);
  await click('#remove-contacts-dialog button[value=remove]');
  await driver.waitUntil(async () => (await invoke('contacts_info')).on === false);
  await stop(); await launch();
  assert.equal((await invoke('contacts_info')).on, false);
  assert.equal((await invoke('contacts_info')).count, 0);
});
