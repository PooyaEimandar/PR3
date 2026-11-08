import assert from 'node:assert/strict';
import { test } from 'node:test';
import { turnStop } from '../src/turn-stop.ts';

for (const early of [true, false]) {
  test(`Stop is delivered once to the acknowledged turn (${early ? 'early' : 'registered'} click)`, () => {
    const calls = [];
    let notices = 0;
    const control = turnStop(() => notices++, id => calls.push(id));
    if (!early) control.started(42);
    control.request(); control.request();
    assert.deepEqual(calls, early ? [] : [42]);
    control.started(42); control.started(99);
    assert.deepEqual(calls, [42]);
    assert.equal(notices, 1);
  });
}

test('a delayed start from a finished turn cannot cancel the next turn', () => {
  const calls = [];
  const old = turnStop(() => {}, id => calls.push(id));
  old.request(); old.finish();
  const next = turnStop(() => {}, id => calls.push(id));
  next.started(7);
  old.started(6); old.request(); old.failed(); old.request();
  assert.deepEqual(calls, []);
  next.request();
  assert.deepEqual(calls, [7]);
});

test('a rejected Stop can be explicitly retried without automatic resubmission', () => {
  const calls = [];
  const control = turnStop(() => {}, id => calls.push(id));
  control.started(42); control.request(); control.failed();
  assert.deepEqual(calls, [42]);
  assert.equal(control.requested, false);
  control.request();
  assert.deepEqual(calls, [42, 42]);
});
