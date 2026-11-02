import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import vm from "node:vm";
import { registry } from "../../../varta-common/browser/registry.mjs";
const source = (await Promise.all(["scheduler.js", "timers.js"].map((file) => readFile(new URL(file, import.meta.url), "utf8")))).join("\n");
function state({ allow = true, capacity = 4 } = {}) {
  const profile = structuredClone(registry.profiles.at(-1)); profile.schedulerLimits.timers = capacity;
  const context = vm.createContext({ registry });
  vm.runInContext(source + "\nglobalThis.make=()=>{const s=createScheduler(profile,String);const t=createTimers(profile,s,()=>allow,emit,String);return {s,t};};", context);
  const logs = []; Object.assign(context, { profile, allow, emit: (line) => logs.push(line) });
  const { s, t } = context.make();
  return { s, t, op: t.implementations, logs, serial: (token) => s.implementations.promise_react(0, token) };
}
test("indexed due order removes cancelled minima and preserves equal-time arm order", () => {
  const { s, t, op, serial } = state();
  const a = serial(1n), b = serial(2n), c = serial(3n);
  op.timer_start_v2(a, 10, 0); const cancelled = op.timer_start_v2(b, 1, 0); op.timer_start_v2(c, 10, 0);
  op.timer_clear(cancelled); assert.equal(t.nextDue(), 10);
  const order = [];
  const dispatch = { exports: { [registry.scheduler.resumeExport](token) { order.push(token); op.timer_take(token === 1n ? a : token === 2n ? b : c); return 1; } } };
  s.drain(dispatch, () => {}); t.fire(); s.drain(dispatch, () => {}); t.fire(); s.drain(dispatch, () => {});
  assert.deepEqual(order, [2n, 1n, 3n]); assert.equal(t.nextDue(), undefined);
});
test("interval clear inside its callback releases the next await without another callback", () => {
  const { s, t, op, serial } = state(); const id = serial(1n);
  const timer = op.timer_start_v2(id, 1, registry.scheduler.timer.interval); t.fire();
  const results = [];
  s.drain({ exports: { [registry.scheduler.resumeExport]() {
    const value = op.timer_take(id); results.push(value);
    if (value) { op.timer_clear(timer); s.implementations.promise_react(id, 2n); }
    return 0;
  } } }, () => {});
  assert.deepEqual(results, [1, 0]); assert.equal(t.nextDue(), undefined);
});
test("unreferenced timers fire while other work keeps the loop alive", () => {
  const { s, t, op, serial } = state();
  const a = serial(1n), b = serial(2n);
  op.timer_start_v2(a, 1, registry.scheduler.timer.unref); assert.equal(t.nextDue(), undefined);
  op.timer_start_v2(b, 10, 0); assert.equal(t.nextDue(), 1); t.fire();
  s.drain({ exports: { [registry.scheduler.resumeExport]() { assert.equal(op.timer_take(a), 1); return 1; } } }, () => {});
  assert.equal(t.nextDue(), 10);
});
test("denial arms nothing, returns zero and schedules only frame cleanup", () => {
  const { s, t, op, serial, logs } = state({ allow: false }); const id = serial(1n);
  assert.equal(op.timer_start_v2(id, 100, 0), 0); assert.equal(t.nextDue(), undefined);
  assert.deepEqual(logs, [registry.scheduler.timer.denialText]);
  s.drain({ exports: { [registry.scheduler.resumeExport]() { assert.equal(op.timer_take(id), 0); return 1; } } }, () => {});
  assert.throws(() => op.timer_take(id), /invalid timer operation/);
  assert.throws(() => op.timer_start_v2(0, 1, 8), /invalid timer operation/);
  assert.throws(() => op.timer_start(0, 1, 8), /capability denied/);
});
test("normalization, capacity and repeated cleanup stay bounded", () => {
  const { s, t, op, serial } = state({ capacity: 1 });
  for (const delay of [NaN, Infinity, -1, 0, 1.9]) {
    const id = serial(1n), timer = op.timer_start_v2(id, delay, 0);
    assert.equal(t.nextDue(), 1);
    const other = serial(2n); assert.throws(() => op.timer_start_v2(other, 1, 0), /pending timers/);
    s.implementations.promise_settle(other);
    op.timer_clear(timer);
    s.drain({ exports: { [registry.scheduler.resumeExport](token) { if (token === 1n) assert.equal(op.timer_take(id), 0); return 1; } } }, () => {});
    assert.equal(t.nextDue(), undefined);
  }
});
