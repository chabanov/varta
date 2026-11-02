// Trusted host accounting for a compiler-derived private execution image.
// This module does not validate artifacts or authorize any effect.
import { registry } from "./registry.mjs";

export class FuelExhausted extends Error {
  constructor() { super("fuel exhausted"); this.name = "FuelExhausted"; }
}

function count(value) {
  if (!Number.isSafeInteger(value) || value < 0) throw new RangeError("invalid fuel work count");
  return BigInt(value);
}

export class FuelTank {
  #counter = new WebAssembly.Global({ value: "i64", mutable: true }, 0n);

  constructor(budget) { this.beginTurn(budget); }
  // Reset only between turns with no active guest call. This is trusted host
  // state: never expose the tank or its mutable global to application scripts.
  beginTurn(budget) {
    const units = count(budget);
    if (units > BigInt(registry.maxTurnFuel)) throw new RangeError("fuel budget exceeds host limit");
    this.#counter.value = -units;
  }
  get global() { return this.#counter; }
  get remaining() {
    const left = -this.#counter.value;
    return left > 0n ? Number(left) : 0;
  }
  charge(rate, units = 1) {
    if (!Object.hasOwn(registry.fuel, rate)) throw new RangeError("unknown fuel work rate");
    const amount = count(units) * BigInt(registry.fuel[rate]);
    const left = BigInt(this.remaining);
    if (amount > left) {
      this.#counter.value = 0n;
      throw new FuelExhausted();
    }
    // Like native, a zero charge does not flush an already exhausted cached
    // counter. An exact charge succeeds with zero remaining fuel.
    if (amount !== 0n) this.#counter.value = -(left - amount);
  }
}
