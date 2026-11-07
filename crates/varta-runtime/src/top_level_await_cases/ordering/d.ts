import { note, tick } from "./log.ts";
const before = note("d start");
const one = await tick("d tick 1");
const two = await tick("d tick 2");
export const d = note("d end " + String(two - one + before));
