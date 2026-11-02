import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import vm from "node:vm";
import { registry } from "../../../varta-common/browser/registry.mjs";

const source = await readFile(new URL("./scheduler.js", import.meta.url), "utf8");
function scheduler({ held = 8, jobs = 100, serials = 128, kept = 2, render = String } = {}) {
  const data = structuredClone(registry);
  Object.assign(data.scheduler, { maxJobs: jobs, maxSerials: serials, retainedReasons: kept });
  const context = vm.createContext({ registry: data });
  vm.runInContext(source + "\nglobalThis.create = createScheduler;", context);
  const profile = structuredClone(data.profiles.at(-1));
  profile.schedulerLimits = { held, queueError: `queue ${held}` };
  const state = context.create(profile, render);
  return { state, op: state.implementations, S: data.scheduler };
}

test("FIFO reactions and reentrant jobs share one checkpoint", () => {
  const { state, op, S } = scheduler();
  const id = op.promise_react(0, 1n);
  op.promise_react(id, 2n); op.scheduler_enqueue(3n); op.promise_settle(id);
  const order = [];
  state.drain({ exports: { [S.resumeExport](token, kind, value) {
    assert.equal(kind, S.normal); assert.equal(value, 0n); order.push(token);
    if (token === 3n) op.scheduler_enqueue(4n);
    return S.outcomes[1];
  } } }, () => {});
  assert.deepEqual(order, [3n, 1n, 2n, 4n]);
  assert.equal(state.jobs, 4);
  assert.throws(() => op.promise_react(id, 5n), /unknown promise serial/);
});
test("queue limit precedes duplicate detection and dispatched tokens release their holds", () => {
  const { state, op, S } = scheduler({ held: 2 });
  op.scheduler_enqueue(1n);
  assert.throws(() => op.scheduler_enqueue(1n), /already scheduled/);
  op.scheduler_enqueue(2n);
  assert.throws(() => op.scheduler_enqueue(1n), /queue 2/);
  state.drain({ exports: { [S.resumeExport]() { return S.outcomes[0]; } } }, () => {});
  op.scheduler_enqueue(1n);
});
test("serials, job budget and resume outcomes fail closed", () => {
  const { state, op, S } = scheduler({ jobs: 1 });
  for (const id of [-1, 0.5, NaN, Infinity, 1]) assert.throws(() => op.promise_react(id, 1n), /unknown promise serial/);
  assert.throws(() => op.promise_settle(0), /unknown promise serial/);
  op.scheduler_enqueue(1n); op.scheduler_enqueue(2n);
  assert.throws(() => state.drain({ exports: { [S.resumeExport]() { return S.outcomes[1]; } } }, () => {}), /microtasks/);
  assert.equal(state.jobs, 1);
  const other = scheduler(); other.op.scheduler_enqueue(1n);
  assert.throws(() => other.state.drain({ exports: { [other.S.resumeExport]() { return 2; } } }, () => {}), /invalid outcome/);
});
test("rejection diagnostics are snapshots, bounded in retention and checked after jobs", () => {
  let reason = "first";
  const { state, op, S } = scheduler({ render: () => reason });
  const id = op.promise_unhandled(123n); reason = "changed";
  assert.throws(() => state.drain({}, () => {}), /rejection: first/);
  op.scheduler_enqueue(1n);
  state.drain({ exports: { [S.resumeExport]() { op.promise_handled(id); return S.outcomes[1]; } } }, () => {});
  for (let i = 0; i < 4; i++) op.promise_unhandled(1n);
  op.promise_handled(2); op.promise_handled(3);
  assert.throws(() => state.drain({}, () => {}), /<reason not retained> \(2 rejected/);
  assert.throws(() => op.promise_handled(2), /unknown promise serial/);
});
test("tracking crosses bit-word boundaries and serial limits remain bounded", () => {
  const { state, op } = scheduler({ kept: 0, serials: 65 });
  for (let id = 1; id <= 65; id++) assert.equal(op.promise_unhandled(0n), id);
  for (let id = 1; id <= 65; id++) if (id !== 32) op.promise_handled(id);
  assert.throws(() => state.drain({}, () => {}), /\(1 rejected/);
  assert.throws(() => op.promise_unhandled(0n), /promise serials/);
  op.promise_handled(32); state.drain({}, () => {});
});
