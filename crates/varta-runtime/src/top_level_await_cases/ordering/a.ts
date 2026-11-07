import { note, tick } from "./log.ts";
const started = note("a start");
const first = await tick("a tick");
export const value: string = "a" + String(first - started);
export const finished = note("a end " + value);
