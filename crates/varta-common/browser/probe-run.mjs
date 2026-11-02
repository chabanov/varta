// Shared native/JS fixture observer, used under Node and a real browser.
// Only for the fixed, trusted browser_meter_probe outputs; not a general host.
import { FuelTank } from "./fuel.mjs";
import { registry } from "./registry.mjs";

export async function observe(module, budget) {
  const tank = new FuelTank(budget);
  const logs = [];
  let instance;
  instance = await WebAssembly.instantiate(module, {
    [registry.meter.module]: { [registry.meter.counter]: tank.global },
    varta: {
      log_string16(ptr, len) {
        const before = tank.remaining;
        tank.charge("call");
        tank.charge("byte", len);
        const input = new DataView(instance.exports.memory.buffer);
        const units = [];
        for (let i = 0; i < len; i += 2) units.push(input.getUint16(ptr + i, true));
        logs.push(`${before}\t${String.fromCharCode(...units)}`);
      },
    },
  });
  let ok = true;
  try { instance.exports._start(); } catch { ok = false; }
  return `${ok}\n${tank.remaining}\n${logs.join("\n")}`;
}
