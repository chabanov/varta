// Node runner for the JS host, used to compare it with the native runtime.
//
//   node cli.mjs run <app.wasm> [--manifest <file>] [--ceiling <file>] [--env NAME=VALUE]...
//   node cli.mjs invoke <app.wasm> <tool> [--params <json>] [--manifest <file>] [--ceiling <file>] [--env NAME=VALUE]...
//   node cli.mjs validate <app.wasm>
//
// A sidecar <app>.manifest.json next to the Wasm is used when --manifest is absent.
import { readFileSync, existsSync } from "node:fs";
import { createHost } from "./host.js";

const contract = JSON.parse(readFileSync(new URL("./contract.json", import.meta.url), "utf8"));
const host = createHost(contract);

const [command, wasmPath, ...rest] = process.argv.slice(2);
const options = { env: new Map(), manifest: null, ceiling: null };
const positional = [];
let params = "{}";
for (let i = 0; i < rest.length; i += 1) {
  const arg = rest[i];
  if (arg === "--manifest") options.manifest = readFileSync(rest[++i], "utf8");
  else if (arg === "--ceiling") options.ceiling = readFileSync(rest[++i], "utf8");
  else if (arg === "--params") params = rest[++i];
  else if (arg === "--env") {
    const [name, ...value] = rest[++i].split("=");
    options.env.set(name, value.join("="));
  } else positional.push(arg);
}
if (!command || !wasmPath) {
  console.error("usage: cli.mjs run|invoke|validate <app.wasm> ...");
  process.exit(2);
}
const sidecar = wasmPath.replace(/\.wasm$/, ".manifest.json");
if (options.manifest === null && existsSync(sidecar)) options.manifest = readFileSync(sidecar, "utf8");
const bytes = new Uint8Array(readFileSync(wasmPath));

if (command === "validate") {
  const info = host.validate(bytes, options);
  console.log(JSON.stringify({ profile: info.profile?.name ?? null, hostAbi: info.hostAbi, imports: info.used.map((o) => o.name), tools: info.catalog?.tools.map((t) => t.name) ?? [] }));
} else if (command === "run") {
  const outcome = await host.run(bytes, options);
  console.log(JSON.stringify(outcome));
  process.exitCode = outcome.status === "ok" ? 0 : 1;
} else if (command === "invoke") {
  const outcome = await host.invoke(bytes, positional[0], params, options);
  console.log(JSON.stringify(outcome));
  process.exitCode = outcome.status === "ok" ? 0 : 1;
} else {
  console.error(`unknown command ${command}`);
  process.exitCode = 2;
}
