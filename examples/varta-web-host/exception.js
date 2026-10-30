// embedding.md exception channel: the state checked after instantiation, `_start`
// and every scheduler job.
import { ExecutionFailure } from "./errors.js";

export function checkException(C, exec, info, phase) {
  if (!info.profile?.exceptionChannel) return;
  const x = exec.instance.exports;
  const pending = x["varta.exception.pending"].value;
  const value = x["varta.exception.value"].value;
  const root = new DataView(x.memory.buffer).getBigInt64(C.exceptionRoot, true);
  if (pending === 0 && value === 0n && root === 0n) return;
  if (phase !== "initial" && pending === 1 && value === root) {
    // Rendering the thrown value needs the unstable value layout: report it as opaque.
    throw new ExecutionFailure(`uncaught guest exception (opaque value 0x${BigInt.asUintN(64, value).toString(16)})`);
  }
  throw new ExecutionFailure(`invalid exception channel state ${phase === "initial" ? "after instantiation" : `after ${phase}`}`);
}
