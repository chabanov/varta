// Shared real-wall-clock Chrome driver for local host acceptance probes.
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { spawn } from "node:child_process";

export async function withBrowserPage(url, action) {
  const profile = await mkdtemp(join(tmpdir(), "varta-browser-probe-"));
  const executable = process.env.VARTA_CHROME_BIN ?? (process.platform === "darwin" ? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" : "chromium");
  let child, socket, spawnError;
  const waiting = new Map();
  try {
    child = spawn(executable, ["--headless", "--no-first-run", "--no-default-browser-check", "--disable-background-networking", "--remote-debugging-port=0", `--user-data-dir=${profile}`, "about:blank"], { stdio: "ignore" });
    child.on("error", (error) => { spawnError = error; });
    const started = Date.now(); let port;
    while (!port) {
      if (spawnError) throw spawnError;
      if (child.exitCode !== null || Date.now() - started > 10000) throw new Error("Chrome did not start");
      try { port = Number((await readFile(join(profile, "DevToolsActivePort"), "utf8")).split("\n")[0]); }
      catch { await new Promise((resolve) => setTimeout(resolve, 50)); }
    }
    const target = await (await fetch(`http://127.0.0.1:${port}/json/new?about:blank`, { method: "PUT" })).json();
    socket = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise((resolve, reject) => { socket.addEventListener("open", resolve, { once: true }); socket.addEventListener("error", reject, { once: true }); });
    let next = 0;
    socket.addEventListener("message", ({ data }) => {
      const message = JSON.parse(data); if (!message.id) return;
      const entry = waiting.get(message.id); if (!entry) return;
      clearTimeout(entry.timer); waiting.delete(message.id);
      if (message.error) entry.reject(new Error(JSON.stringify(message.error))); else entry.resolve(message.result);
    });
    const call = (method, params = {}) => new Promise((resolve, reject) => {
      const id = ++next, timer = setTimeout(() => { waiting.delete(id); reject(new Error(`CDP timeout: ${method}`)); }, 12000);
      waiting.set(id, { resolve, reject, timer }); socket.send(JSON.stringify({ id, method, params }));
    });
    await call("Page.enable"); await call("Runtime.enable"); await call("Page.navigate", { url });
    for (let i = 0; ; i++) {
      const result = await call("Runtime.evaluate", { expression: `document.readyState === "complete" && location.href === ${JSON.stringify(url)}`, returnByValue: true });
      if (result.result.value === true) break;
      if (i > 100) throw new Error("probe page did not load");
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    return await action(call);
  } finally {
    for (const entry of waiting.values()) { clearTimeout(entry.timer); entry.reject(new Error("browser closed")); }
    socket?.close();
    if (child && child.exitCode === null && child.signalCode === null) {
      child.kill("SIGTERM");
      await Promise.race([new Promise((resolve) => child.once("exit", resolve)), new Promise((resolve) => setTimeout(resolve, 3000))]);
    }
    if (child && child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
    await rm(profile, { recursive: true, force: true });
  }
}
