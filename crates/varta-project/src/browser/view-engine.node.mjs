// The checked-in view style engine compiled once for Node tests and probes
// (the bundle loads the same bytes as a verified file instead).
import { readFileSync } from "node:fs";

export const viewEngine = new WebAssembly.Module(readFileSync(new URL("./view-engine.wasm", import.meta.url)));
