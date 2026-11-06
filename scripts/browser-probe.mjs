// Shared real-wall-clock Chrome driver for local host acceptance probes.
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { spawn } from "node:child_process";

const START_TIMEOUT_MS = 10000;
const START_POLL_MS = 50;
const CDP_CALL_TIMEOUT_MS = 12000;
const LOAD_POLL_LIMIT = 100;
const LOAD_POLL_MS = 20;
// Teardown: SIGTERM grace, then a bounded wait for the SIGKILLed process to be
// reaped, then profile removal that retries ENOTEMPTY/EBUSY/EPERM. One gate run
// failed the removal with ENOTEMPTY; the race was never reproduced and its cause
// (presumably a Chrome process still writing the profile) is not established, so
// the wait and the retries are a mitigation, not a verified fix.
const TERM_GRACE_MS = 3000;
const KILL_REAP_MS = 3000;
const PROFILE_RM_RETRIES = 10;
const PROFILE_RM_RETRY_DELAY_MS = 200;

const running = (child) => child && child.exitCode === null && child.signalCode === null;
// Resolves when the child exits or after `ms`, whichever comes first, and leaves
// no timer or listener behind. A child that has already exited resolves at once;
// 'exit' is emitted from a later event-loop turn, so the synchronous check and
// subscription cannot miss it.
const exitWithin = (child, ms) => new Promise((resolve) => {
  if (!running(child)) { resolve(); return; }
  const done = () => { clearTimeout(timer); child.off("exit", done); resolve(); };
  const timer = setTimeout(done, ms);
  child.once("exit", done);
});

// `macDefaults` sets macOS user defaults for this Chrome process only, in
// AppKit's argument domain (`-Key Value`), for example
// `{ AppleShowScrollBars: "Always" }` for classic scrollbars; no system
// setting changes. Chrome then reads the last value as its initial target,
// so the process starts in the private profile directory beside an empty
// file of that name, which it opens locally instead of the default page.
export async function withBrowserPage(url, action, { macDefaults } = {}) {
  const profile = await mkdtemp(join(tmpdir(), "varta-browser-probe-"));
  const executable = process.env.VARTA_CHROME_BIN ?? (process.platform === "darwin" ? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" : "chromium");
  let child, socket, spawnError;
  const waiting = new Map();
  try {
    const defaults = Object.entries(macDefaults ?? {});
    if (defaults.length && process.platform !== "darwin") throw new Error("macDefaults needs macOS");
    for (const [key, value] of defaults) {
      if (!/^[A-Za-z]+$/.test(key) || !/^[A-Za-z]+$/.test(value)) throw new Error(`invalid macOS default ${key}`);
    }
    if (defaults.length) await writeFile(join(profile, defaults.at(-1)[1]), "");
    const initial = defaults.length ? defaults.flatMap(([key, value]) => [`-${key}`, value]) : ["about:blank"];
    child = spawn(executable, ["--headless", "--no-first-run", "--no-default-browser-check", "--disable-background-networking", "--remote-debugging-port=0", `--user-data-dir=${profile}`, ...initial], { stdio: "ignore", cwd: profile });
    child.on("error", (error) => { spawnError = error; });
    const started = Date.now(); let port;
    while (!port) {
      if (spawnError) throw spawnError;
      if (child.exitCode !== null || Date.now() - started > START_TIMEOUT_MS) throw new Error("Chrome did not start");
      try { port = Number((await readFile(join(profile, "DevToolsActivePort"), "utf8")).split("\n")[0]); }
      catch { await new Promise((resolve) => setTimeout(resolve, START_POLL_MS)); }
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
      const id = ++next, timer = setTimeout(() => { waiting.delete(id); reject(new Error(`CDP timeout: ${method}`)); }, CDP_CALL_TIMEOUT_MS);
      waiting.set(id, { resolve, reject, timer }); socket.send(JSON.stringify({ id, method, params }));
    });
    await call("Page.enable"); await call("Runtime.enable"); await call("Page.navigate", { url });
    for (let i = 0; ; i++) {
      const result = await call("Runtime.evaluate", { expression: `document.readyState === "complete" && location.href === ${JSON.stringify(url)}`, returnByValue: true });
      if (result.result.value === true) break;
      if (i > LOAD_POLL_LIMIT) throw new Error("probe page did not load");
      await new Promise((resolve) => setTimeout(resolve, LOAD_POLL_MS));
    }
    return await action(call);
  } finally {
    for (const entry of waiting.values()) { clearTimeout(entry.timer); entry.reject(new Error("browser closed")); }
    socket?.close();
    if (running(child)) {
      child.kill("SIGTERM");
      await exitWithin(child, TERM_GRACE_MS);
    }
    if (running(child)) {
      child.kill("SIGKILL");
      await exitWithin(child, KILL_REAP_MS);
    }
    await rm(profile, { recursive: true, force: true, maxRetries: PROFILE_RM_RETRIES, retryDelay: PROFILE_RM_RETRY_DELAY_MS });
  }
}
