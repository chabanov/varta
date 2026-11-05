// The console's settings in the official browser host: the API key the
// person saves goes to the browser's KV storage (OPFS) and is read back
// after a reload, as natively. The product (`varta.json`) runs:
//   - natively, headless (`varta dev --view-script`), three sessions over one
//     KV directory: a fresh store (no key: the settings dialog is open),
//     typing a key and pressing Enter (saved), and a later start (the key is
//     read back: no dialog, the console connects);
//   - in local Chrome from its own `varta build --target browser` bundle,
//     mounted on a page of a temporary profile: the same three states, the
//     key typed with trusted input, the third after a page reload.
// The network stays closed on both hosts: the host ceiling grants KV and the
// clock but not `net.request`, so the console's first request is denied
// before any socket, with the same diagnostic. In each state the same
// markers must be visible on both hosts (no key stored, settings saved,
// connecting) and the settings dialog open or closed alike; the log lines
// must be equal, and the key must appear in no snapshot or page. (The whole
// accessibility trees are not compared: Chrome hides what a modal dialog
// covers, which the headless model does not; package w4-native-dialog.)
//
//   node scripts/test-browser-settings.mjs <varta-executable>
import { createServer } from "node:http";
import { readFile, writeFile, rm, mkdtemp, mkdir } from "node:fs/promises";
import { join, resolve, sep, dirname } from "node:path";
import { tmpdir } from "node:os";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";
import { withBrowserPage } from "../../../scripts/browser-probe.mjs";

// A test key: never a real credential.
const KEY = "uarp_browser-settings-test";
const NO_KEY = "Ключ API не збережено.";
const SAVED = "Налаштування збережено.";
const CONNECTING = "Підключення…";
const MARKERS = [NO_KEY, SAVED, CONNECTING];
const SETTINGS_DIALOG = "settings";
const KEY_FIELD = "api-key";
const STATE_TIMEOUT_MS = 60_000;
const POLL_MS = 50;
const TEXT_NODE = 3;

const executableArgument = process.argv[2];
if (!executableArgument) throw new Error("Usage: node scripts/test-browser-settings.mjs <varta-executable>");
const executable = resolve(executableArgument);
const console_ = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const run = promisify(execFile);
const id = `${process.pid}`;
// Ignored by .gitignore (`varta.sdk-test.loopback-*.json`) and dist/.
const configName = `varta.sdk-test.loopback-settings-${id}.json`;
const out = `dist/settings-browser-${id}`;

const logLines = (stdout) => stdout.split("\n").filter((line) => line.startsWith("varta log: "));
const state = ({ visible, dialog }) => ({ visible, dialog });
// A state, natively: the markers of the snapshot's accessibility tree and
// whether it holds the dialog.
function nativeState(stdout) {
  const start = stdout.search(/^\{/m), end = stdout.lastIndexOf("}\n") + 1;
  const snapshot = JSON.parse(stdout.slice(start, end));
  const seen = new Set();
  let dialog = false;
  const visit = (node) => {
    if (node.role === "text" && MARKERS.includes(node.name)) seen.add(node.name);
    if (node.role === "dialog") dialog = true;
    for (const child of node.children ?? []) visit(child);
  };
  visit(snapshot.accessibility);
  assert(!JSON.stringify(snapshot).includes(KEY), "the native snapshot holds no key");
  return { visible: MARKERS.filter((marker) => seen.has(marker)), dialog };
}

const work = await mkdtemp(join(tmpdir(), "varta-browser-settings-"));
let pages;
try {
  // Native: the product's manifest as the host policy, without net.request,
  // its data root mapped to a fresh directory.
  const config = JSON.parse(await readFile(join(console_, "varta.json"), "utf8"));
  await writeFile(join(console_, configName), JSON.stringify({ ...config, out: `${out}/app` }, null, 2));
  await run(executable, ["build", "--config", configName], { cwd: console_, maxBuffer: 1 << 20 });
  const manifest = JSON.parse(await readFile(join(console_, out, "app.manifest.json"), "utf8"));
  const capabilities = manifest.capabilities.filter((capability) => capability.id !== "net.request");
  assert.equal(capabilities.length, manifest.capabilities.length - 1, "the product requests net.request");
  const data = join(work, "data");
  await mkdir(data);
  const { entry, target, policy, ...rest } = manifest;
  await writeFile(join(work, "policy.json"), JSON.stringify({ ...rest, capabilities, fsRoots: { data } }));
  const save = { inputs: [{ input: "type", target: `#${KEY_FIELD}`, text: KEY }, { input: "key", target: `#${KEY_FIELD}`, key: "Enter" }] };
  await writeFile(join(work, "save.json"), JSON.stringify(save));
  await writeFile(join(work, "idle.json"), JSON.stringify({ inputs: [] }));
  const session = (script) => run(executable, ["dev", "--config", configName, "--policy", join(work, "policy.json"), "--view-script", join(work, script)], { cwd: console_, maxBuffer: 1 << 24 });
  const native = [];
  for (const script of ["idle.json", "save.json", "idle.json"]) {
    const { stdout } = await session(script);
    native.push({ logs: logLines(stdout), ...nativeState(stdout) });
  }
  // Natively: no key and the dialog open; saved and connecting; after a
  // later start connecting with the stored key.
  assert.deepEqual(native.map(state), [
    { visible: [NO_KEY], dialog: true },
    { visible: [SAVED, CONNECTING], dialog: false },
    { visible: [CONNECTING], dialog: false },
  ], JSON.stringify(native));

  // Chrome: the product's own bundle; the same ceiling.
  await run(executable, ["build", "--target", "browser", "--config", configName], { cwd: console_, maxBuffer: 1 << 20 });
  const web = join(console_, out, "web");
  pages = createServer(async (request, response) => {
    try {
      const name = decodeURIComponent(new URL(request.url, "http://loopback").pathname);
      if (name === "/blank.html") { response.writeHead(200, { "content-type": "text/html" }).end("<!doctype html><title>settings</title>"); return; }
      const path = resolve(web, `.${name}`);
      if (!path.startsWith(web + sep)) throw new Error("outside the bundle");
      const bytes = await readFile(path);
      response.writeHead(200, { "cache-control": "no-store", "content-type": path.endsWith(".js") ? "text/javascript" : "application/octet-stream" }).end(bytes);
    } catch { response.writeHead(404).end(); }
  });
  const pageBase = await new Promise((done) => pages.listen(0, "127.0.0.1", () => done(`http://127.0.0.1:${pages.address().port}`)));
  const ceiling = Object.fromEntries(capabilities.map(({ id: name, patterns }) => [name, patterns ? { patterns } : true]));
  const browser = await withBrowserPage(`${pageBase}/blank.html`, async (call) => {
    const evaluate = async (expression) => {
      const result = await call("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
      assert(!result.exceptionDetails, JSON.stringify(result.exceptionDetails));
      return result.result.value;
    };
    const mount = () => evaluate(`(async()=>{const {mount}=await import(${JSON.stringify(`${pageBase}/varta-host.js`)});
      const container=document.createElement('div');container.id='console';document.body.append(container);
      globalThis.probe={logs:[],failure:undefined};
      globalThis.app=mount({container,permissionCeiling:${JSON.stringify(ceiling)},onLog:(line)=>probe.logs.push(line)});
      app.finished.then((result)=>{if(result.status!=='ok')probe.failure=result.error;},(error)=>{probe.failure=String(error.message??error);});
      return true;})()`);
    const attributes = (node) => Object.fromEntries((node.attributes ?? []).flatMap((value, index, all) => index % 2 ? [] : [[value, all[index + 1]]]));
    const find = (node, wanted) => attributes(node).id === wanted ? node : [...(node.children ?? []), ...(node.shadowRoots ?? [])].map((child) => find(child, wanted)).find(Boolean);
    // Which markers Chrome renders visibly (the CSS visibility of the text's
    // element, reached through the closed shadow root by node id), and
    // whether the settings dialog is open.
    const current = async () => {
      const { root } = await call("DOM.getDocument", { depth: -1, pierce: true });
      const visible = new Set();
      let dialog = false;
      const visit = async (node, parent) => {
        const text = node.nodeType === TEXT_NODE ? node.nodeValue.trim() : undefined;
        if (text !== undefined && MARKERS.includes(text) && parent) {
          const { object } = await call("DOM.resolveNode", { nodeId: parent.nodeId });
          const shown = await call("Runtime.callFunctionOn", { objectId: object.objectId, functionDeclaration: "function(){return this.checkVisibility({visibilityProperty:true,opacityProperty:true});}", returnByValue: true });
          if (shown.result.value) visible.add(text);
        }
        if (attributes(node).id === SETTINGS_DIALOG && Object.hasOwn(attributes(node), "open")) dialog = true;
        const element = node.nodeType === TEXT_NODE ? parent : node;
        for (const child of [...(node.children ?? []), ...(node.shadowRoots ?? [])]) await visit(child, element);
      };
      const container = find(root, "console");
      if (container) await visit(container, undefined);
      return { visible: MARKERS.filter((marker) => visible.has(marker)), dialog };
    };
    const until = async (label, expected) => {
      const deadline = Date.now() + STATE_TIMEOUT_MS;
      let seen;
      do {
        const failure = await evaluate("probe.failure");
        assert.equal(failure, undefined, `${label}: ${failure}`);
        seen = await current();
        if (JSON.stringify(seen) === JSON.stringify(expected)) return seen;
        await new Promise((done) => setTimeout(done, POLL_MS));
      } while (Date.now() < deadline);
      assert.fail(`${label}: Chrome shows ${JSON.stringify(seen)}, natively ${JSON.stringify(expected)}`);
    };
    const states = [];
    await mount();
    states.push({ ...await until("fresh store", state(native[0])), logs: await evaluate("probe.logs") });
    // Trusted input: click the key field, type, press Enter.
    const field = find((await call("DOM.getDocument", { depth: -1, pierce: true })).root, KEY_FIELD);
    assert(field, "the settings dialog shows the key field");
    const { model } = await call("DOM.getBoxModel", { nodeId: field.nodeId });
    const [x, y] = [(model.content[0] + model.content[2]) / 2, (model.content[1] + model.content[5]) / 2];
    for (const type of ["mousePressed", "mouseReleased"]) await call("Input.dispatchMouseEvent", { type, x, y, button: "left", clickCount: 1 });
    await call("Input.insertText", { text: KEY });
    const { object } = await call("DOM.resolveNode", { nodeId: field.nodeId });
    const typed = await call("Runtime.callFunctionOn", { objectId: object.objectId, functionDeclaration: "function(){return this.value.length;}", returnByValue: true });
    assert.equal(typed.result.value, KEY.length, "the key field holds the typed key");
    // Enter as a key press with its text, which submits the form implicitly.
    await call("Input.dispatchKeyEvent", { type: "keyDown", key: "Enter", code: "Enter", text: "\r", windowsVirtualKeyCode: 13, nativeVirtualKeyCode: 13 });
    await call("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, nativeVirtualKeyCode: 13 });
    states.push({ ...await until("saved", state(native[1])), logs: await evaluate("probe.logs") });
    await evaluate("app.close()");
    // A reload: a new page of the same origin and profile, a new instance.
    await call("Page.reload", {});
    const loaded = Date.now() + STATE_TIMEOUT_MS;
    while ((await evaluate("document.readyState + String(globalThis.probe === undefined)")) !== "completetrue") {
      assert(Date.now() < loaded, "the page did not reload");
      await new Promise((done) => setTimeout(done, POLL_MS));
    }
    await mount();
    states.push({ ...await until("after reload", state(native[2])), logs: await evaluate("probe.logs") });
    const { root } = await call("DOM.getDocument", { depth: -1, pierce: true });
    assert(!JSON.stringify(root).includes(KEY), "the page DOM holds no key");
    return states;
  });
  const labels = ["fresh store", "saved", "after reload"];
  browser.forEach((browserState, index) => {
    assert.deepEqual(browserState.logs, native[index].logs, `${labels[index]}: Chrome logs the native session's lines`);
    console.log(`PASS ${labels[index]}: ${JSON.stringify(state(browserState))} and ${browserState.logs.length} log lines as natively`);
  });
  console.log("Browser settings acceptance passed: the key saved in the console's settings is stored in OPFS KV and read back after a reload, in the native sessions' states and with their log lines; no snapshot or page holds the key");
} finally {
  pages?.close();
  await rm(join(console_, configName), { force: true });
  await rm(join(console_, out), { recursive: true, force: true });
  await rm(work, { recursive: true, force: true });
}
