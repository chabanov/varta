// Runs one v38 view application as a long-lived instance in a module Worker: the
// page applies committed batches with the L0 applier and sends view records back.
import { createHost } from "../host.js";

let instance = null;

self.onmessage = async ({ data }) => {
  if (data.type === "start") {
    try {
      const contract = await (await fetch("../contract.json", { cache: "no-store" })).json();
      const host = createHost(contract);
      const bytes = new Uint8Array(await (await fetch(data.wasm, { cache: "no-store" })).arrayBuffer());
      const manifest = await (await fetch(data.manifest, { cache: "no-store" })).text();
      instance = host.start(bytes, {
        manifest,
        view: { commit: (batch) => self.postMessage({ type: "commit", batch }, [batch.buffer]) },
      });
      instance.finished.then((result) => self.postMessage({ type: "finished", result }));
    } catch (error) {
      self.postMessage({ type: "finished", result: { status: "refused", error: String(error?.message ?? error) } });
    }
  } else if (data.type === "event") instance?.deliver(data.record);
  else if (data.type === "refused") instance?.fail(`the view refused a batch: ${data.message}`);
  else if (data.type === "stop") instance?.stop();
};
