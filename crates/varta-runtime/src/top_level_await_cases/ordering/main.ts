import { events, note, tick } from "./log.ts";
import { c } from "./c.ts";
import { b } from "./b.ts";
import { d } from "./d.ts";
const pending = tick("main tick");
const sum = note("main module " + String(b + c + d));
export async function main(): Promise<void> {
  note("main");
  await pending;
  console.log(events.join(" | "));
}
