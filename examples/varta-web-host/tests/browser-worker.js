// Runs the shared cases in a dedicated module Worker, the recommended browser
// placement for the guest (the page can terminate it to enforce a deadline).
import { createHost } from "../host.js";
import { runJsCases } from "./cases.js";

const fetchText = async (url) => {
  const response = await fetch(url, { cache: "no-store" });
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
  return response;
};

self.onmessage = async () => {
  try {
    const contract = await (await fetchText("../contract.json")).json();
    const host = createHost(contract);
    const load = async (name) => ({
      bytes: new Uint8Array(await (await fetchText(`../fixtures/${name}/dist/app.wasm`)).arrayBuffer()),
      manifest: await (await fetchText(`../fixtures/${name}/dist/app.manifest.json`)).text(),
    });
    self.postMessage({ results: await runJsCases(host, load, contract), guarantees: host.guarantees });
  } catch (error) {
    self.postMessage({ error: String(error?.stack ?? error) });
  }
};
