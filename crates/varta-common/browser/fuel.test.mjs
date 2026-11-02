import { test } from "node:test";
import assert from "node:assert/strict";
import { FuelTank, FuelExhausted } from "./fuel.mjs";
import { registry } from "./registry.mjs";

test("generated registry is deeply frozen and distinguishes HTTP from tools", () => {
  const p = registry.profiles.find((p) => p.httpVersion === 2);
  assert.equal(p.toolEntries, false);
  assert.throws(() => { p.httpVersion = 1; }, TypeError);
  assert.throws(() => { registry.operations[0].params.push("i32"); }, TypeError);
});
test("guest and host work share one exact i64 counter", () => {
  const tank = new FuelTank(100);
  tank.global.value += 7n; // guest instruction consumption
  tank.charge("call");
  tank.charge("byte", 10);
  assert.equal(tank.remaining, 100 - 7 - registry.fuel.call - 10 * registry.fuel.byte);
  tank.charge("byte", tank.remaining);
  assert.equal(tank.remaining, 0);
  assert.throws(() => tank.charge("call"), FuelExhausted);
  assert.equal(tank.global.value, 0n);
});
test("invalid work and budgets fail without changing the tank", () => {
  const tank = new FuelTank(100);
  for (const value of [-1, 0.5, NaN, Infinity, Number.MAX_SAFE_INTEGER + 1]) {
    assert.throws(() => tank.charge("byte", value), RangeError);
    assert.throws(() => tank.beginTurn(value), RangeError);
    assert.equal(tank.remaining, 100);
  }
  assert.throws(() => tank.charge("unregistered"), RangeError);
  assert.throws(() => tank.beginTurn(registry.maxTurnFuel + 1), RangeError);
  assert.equal(tank.remaining, 100);
});
test("oversized host work empties the tank before effects and can reset next turn", () => {
  const tank = new FuelTank(100);
  let effect = false;
  assert.throws(() => { tank.charge("kvOperation", Number.MAX_SAFE_INTEGER); effect = true; }, FuelExhausted);
  assert.equal(effect, false);
  assert.equal(tank.remaining, 0);
  tank.beginTurn(500);
  assert.equal(tank.remaining, 500);
});
