// Real-wall-clock Chrome acceptance for browser_bundle_probe's fixed bundles.
// No virtual-time acceleration: the watchdog must interrupt a running Worker.
import { createServer } from "node:http";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { resolve, sep, join } from "node:path";
import { tmpdir } from "node:os";
import { spawn, execFile } from "node:child_process";
import { promisify } from "node:util";
import assert from "node:assert/strict";

const root = resolve(process.argv[2] ?? "target/browser-bundle-probe");
const executable = process.env.VARTA_CHROME_BIN ?? (process.platform === "darwin"
  ? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" : "chromium");
const profile = await mkdtemp(join(tmpdir(), "varta-bundle-chrome-"));
let deniedNetworkHits=0,cookieNetworkHits=0;
const serve = async (request, response) => {
  try {
    const route = new URL(request.url,"http://loopback").pathname;
    if(route==="/ungranted-target") { deniedNetworkHits++; response.end("forbidden effect");return; }
    if(route.startsWith("/_net/")) {
      if(request.headers.cookie)cookieNetworkHits++;
      response.setHeader("Access-Control-Allow-Origin","*");
      response.setHeader("Access-Control-Expose-Headers","content-type,x-id,content-encoding");
      if(request.method==="OPTIONS") {response.setHeader("Access-Control-Allow-Headers","x-request");response.setHeader("Access-Control-Allow-Methods","GET,POST");response.writeHead(204).end();return;}
      if(route==="/_net/redirect") {response.writeHead(302,{Location:"/ungranted-target"}).end();return;}
      if(route==="/_net/slow") await new Promise((done)=>setTimeout(done,100));
      if(route==="/_net/denied") {deniedNetworkHits++;throw new Error("denied endpoint must never be reached");}
      const chunks=[]; for await(const chunk of request) chunks.push(chunk);
      const body=route==="/_net/echo" ? Buffer.concat(chunks) : Buffer.from("Привіт 😀");
      response.setHeader("Content-Type","text/plain");response.setHeader("X-Id","123");
      response.end(body);return;
    }
    const name = decodeURIComponent(new URL(request.url, "http://loopback").pathname);
    const path = resolve(root, `.${name}`);
    if (!path.startsWith(root + sep)) throw new Error("outside fixtures");
    const bytes = await readFile(path);
    response.setHeader("Cache-Control", "no-store");
    response.setHeader("Content-Type", path.endsWith(".html") ? "text/html" : path.endsWith(".js") ? "text/javascript" : "application/octet-stream");
    response.end(bytes);
  } catch { response.writeHead(404).end(); }
};
const server=createServer(serve),networkServer=createServer(serve);
await new Promise((done) => networkServer.listen(0,"127.0.0.1",done));
const networkBase=`http://127.0.0.1:${networkServer.address().port}`;
await new Promise((done) => server.listen(0, "127.0.0.1", done));
const base = `http://127.0.0.1:${server.address().port}`;
let child, socket;
try {
  if(process.argv.includes("--generate-network")) await promisify(execFile)("cargo",["run","-p","varta-project","--example","browser_bundle_probe","--locked","--",root,networkBase],{maxBuffer:1024*1024});
  child = spawn(executable, ["--headless", "--no-first-run", "--no-default-browser-check", "--disable-background-networking", "--remote-debugging-port=0", `--user-data-dir=${profile}`, "about:blank"], { stdio: "ignore" });
  let spawnError;
  child.on("error", (error) => { spawnError = error; });
  let port;
  const start = Date.now();
  while (!port) {
    if (spawnError) throw spawnError;
    if (child.exitCode !== null || Date.now() - start > 10000) throw new Error("Chrome did not start");
    try { port = Number((await readFile(join(profile, "DevToolsActivePort"), "utf8")).split("\n")[0]); }
    catch { await new Promise((done) => setTimeout(done, 50)); }
  }
  const target = await (await fetch(`http://127.0.0.1:${port}/json/new?about:blank`, { method: "PUT" })).json();
  socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((done, fail) => { socket.onopen = done; socket.onerror = fail; });
  let sequence = 0;
  const pending = new Map();
  socket.onmessage = ({ data }) => {
    const message = JSON.parse(data);
    if (!message.id) return;
    const waiter = pending.get(message.id);
    if (!waiter) return;
    clearTimeout(waiter.timer); pending.delete(message.id);
    if (message.error) waiter.reject(new Error(JSON.stringify(message.error))); else waiter.resolve(message.result);
  };
  const call = (method, params = {}) => new Promise((resolve, reject) => {
    const id = ++sequence;
    const timer = setTimeout(() => { pending.delete(id); reject(new Error(`CDP timeout: ${method}`)); }, 12000);
    pending.set(id, { resolve, reject, timer });
    socket.send(JSON.stringify({ id, method, params }));
  });
  await call("Page.enable");
  let navigation = 0;
  const run = async (name) => {
    const query = `?probe=${++navigation}`;
    await call("Page.navigate", { url: `${base}/${name}/index.html${query}` });
    // Evaluate after navigation commits, then wait on actual DOM completion.
    let result;
    const started = Date.now();
    while (!result) {
      const value = await call("Runtime.evaluate", { expression: `(() => { if(location.search!==${JSON.stringify(query)}) return null; const el=document.querySelector('#varta-output'); return el?.dataset.status ? {status:el.dataset.status,text:el.textContent,guestStarted:el.dataset.guestStarted,usage:el.dataset.usage?JSON.parse(el.dataset.usage):null} : null; })()`, returnByValue: true });
      result = value.result?.value;
      if (Date.now() - started > 10000) throw new Error(`bundle ${name} did not finish`);
      if (!result) await new Promise((done) => setTimeout(done, 25));
    }
    return result;
  };
  const ok = await run("ok");
  assert.equal(ok.status, "ok");
  assert.equal(ok.text, "varta log: 123\nvarta log: Привіт 😀\nvarta log: \uFEFFstart\uFFFDend\nvarta log: [1,2,3]");
  const nativeOk = JSON.parse(await readFile(join(root, "ok.golden.json"), "utf8"));
  assert.equal(ok.status,nativeOk.status);
  assert.equal(ok.text,nativeOk.logs.join("\n"));
  const chunks=await run("chunks");
  const nativeChunks=JSON.parse(await readFile(join(root,"chunks.golden.json"),"utf8"));
  assert.equal(chunks.status,"ok");
  assert.equal(chunks.text,nativeChunks.logs.join("\n"));
  for(const name of ["async","fifo","handled","unhandled","reject-string","reject-number","reject-bigint","reject-class","reject-exponent","reject-retention","reject-long-error","reject-long-class","pending","async-calls","async-budget","timer-order","timer-cancel","timer-interval","timer-denied","timer-optional","timer-throw","timer-deadline","timer-calls","timer-unref","timer-normalize","timer-v36","timer-v37","timer-unref-alive","timer-long-throw"]) {
    const result=await run(name);
    const golden=JSON.parse(await readFile(join(root,`${name}.golden.json`),"utf8"));
    assert.equal(result.status,golden.status,`${name}: ${result.text}`);
    const expected=golden.logs.join("\n");
    if(golden.status==="ok") assert.equal(result.text,expected,name);
    else {
      assert(result.text.startsWith(expected),name);
      if(name.startsWith("reject-")||name==="unhandled"||name.startsWith("timer-")) assert(result.text.endsWith(golden.error),`${name}: ${result.text} != ${golden.error}`);
    }
    assert.deepEqual(result.usage,golden.usage,`${name}: usage`);
  }
  await run("timer-ceiling");
  const deniedGolden=JSON.parse(await readFile(join(root,"timer-ceiling.golden.json"),"utf8"));
  const ceilings=await call("Runtime.evaluate",{
    expression:`(async()=>{const {run}=await import(${JSON.stringify(base+"/timer-ceiling/varta-host.js")});const results=[];for(const permissionCeiling of [{},{"time.timer":false},{"time.now":true},{"time.timer":true}])results.push(await run({permissionCeiling}));for(const permissionCeiling of [null,[],{"time.timer":1},{"time":true}]){try{await run({permissionCeiling});results.push('unexpected success');}catch(error){results.push(error.message);}}return results;})()`,
    awaitPromise:true,returnByValue:true,
  });
  assert(!ceilings.exceptionDetails,JSON.stringify(ceilings.exceptionDetails));
  for(const result of ceilings.result.value.slice(0,3)){
    assert.equal(result.status,deniedGolden.status);
    assert.deepEqual(result.logs,deniedGolden.logs);
    const {fuelRemaining,hostCalls,microtasks,outputBytes,fuelUsed,turns}=result;
    assert.deepEqual({fuelRemaining,hostCalls,microtasks,outputBytes,fuelUsed,turns},deniedGolden.usage);
  }
  assert.deepEqual(ceilings.result.value[3].logs,["varta log: allowed"]);
  for(const error of ceilings.result.value.slice(4))assert.match(error,/invalid.*permission ceiling/);
  if(process.argv.includes("--generate-network")) await call("Runtime.evaluate",{expression:"document.cookie='varta_browser_secret=private; SameSite=Lax'"});
  if(process.argv.includes("--generate-network")) for(const name of ["net-get","net-post","net-bytes","net-denied","net-abort","net-active-abort","net-custom"]) {
    const result=await run(name),golden=JSON.parse(await readFile(join(root,`${name}.golden.json`),"utf8"));
    assert.equal(result.status,golden.status,`${name}: ${result.text}`);
    assert.equal(result.text,golden.logs.join("\n"),`${name}: logs`);
    assert.deepEqual(result.usage,golden.usage,`${name}: usage`);
  }
  if(process.argv.includes("--generate-network")) {
    await run("net-ceiling");
    const golden=JSON.parse(await readFile(join(root,"net-ceiling.golden.json"),"utf8"));
    const observed=await call("Runtime.evaluate",{
      expression:`(async()=>{const {run}=await import(${JSON.stringify(base+"/net-ceiling/varta-host.js")});const results=[];for(const permissionCeiling of [{},{"net.request":{patterns:[]}},{"net.request":{patterns:[${JSON.stringify(networkBase+"/_net/**")}],methods:["POST"]}},{"net.request":{patterns:[${JSON.stringify(networkBase+"/_net/**")}],methods:["GET"]}}])results.push(await run({permissionCeiling}));for(const value of [true,{patterns:["http://*.test/**"]},{patterns:[],methods:["get"]},{patterns:[],responseHeaders:["set-cookie"]}]){try{await run({permissionCeiling:{"net.request":value}});results.push("unexpected success");}catch(error){results.push(error.message);}}return results;})()`,awaitPromise:true,returnByValue:true,
    });
    assert(!observed.exceptionDetails,JSON.stringify(observed.exceptionDetails));
    for(const result of observed.result.value.slice(0,3)) {
      assert.equal(result.status,golden.status);assert.deepEqual(result.logs,golden.logs);
      const {fuelRemaining,hostCalls,microtasks,outputBytes,fuelUsed,turns}=result;
      assert.deepEqual({fuelRemaining,hostCalls,microtasks,outputBytes,fuelUsed,turns},golden.usage);
    }
    assert.deepEqual(observed.result.value[3].logs,["varta log: Привіт 😀"]);
    for(const error of observed.result.value.slice(4))assert.match(error,/invalid network permission ceiling/);
    const redirect=await run("net-redirect");assert.equal(redirect.status,"ok");assert.equal(redirect.text,"varta log: fetch failed");
    assert.equal(cookieNetworkHits,0,"guest network must never attach browser cookies");
    assert.equal(deniedNetworkHits,0,"neither native nor browser may touch a denied URL or follow its redirect");
  }
  const thrown = await run("throw");
  assert.equal(thrown.status, "failed");
  const nativeThrow = JSON.parse(await readFile(join(root, "throw.golden.json"), "utf8"));
  assert.equal(thrown.status,nativeThrow.status);
  assert(thrown.text.endsWith(nativeThrow.error));
  assert(thrown.text.startsWith(nativeThrow.logs.join("\n")));
  for (const name of ["fuel","calls","output"]) {
    const result=await run(name);
    const golden=JSON.parse(await readFile(join(root,`${name}.golden.json`),"utf8"));
    assert.equal(result.status,golden.status,name);
    assert.equal(result.status,"failed",name);
    const observed=result.text.split("\n").filter((line)=>line.startsWith("varta log:"));
    assert.deepEqual(observed,golden.logs,name);
  }
  await run("loop");
  const loopCall=await call("Runtime.evaluate", {
    expression:`(async()=>{const {run}=await import(${JSON.stringify(base+"/loop/varta-host.js")});try{return await run({budgetCeiling:{fuelPerTurn:1000000000}});}catch(error){return{status:'failed',guestStarted:error.guestStarted,text:[...error.logs,error.message].join('\\n')};}})()`,
    awaitPromise:true,returnByValue:true,
  });
  assert(!loopCall.exceptionDetails,JSON.stringify(loopCall.exceptionDetails));
  const loop=loopCall.result.value;
  assert.equal(loop.status,"failed");
  assert.equal(loop.guestStarted,true,"deadline must interrupt guest execution, not just file loading");
  assert.match(loop.text,/before loop.*deadline exceeded/s);
  const invalidCeilings=await call("Runtime.evaluate", {
    expression:`(async()=>{const {run}=await import(${JSON.stringify(base+"/loop/varta-host.js")});const results=[];for(const budgetCeiling of [{fuelPerTurn:0},{fuelPerTurn:1000000001},{unregistered:1},{memoryBytes:1}]){try{await run({budgetCeiling});results.push('unexpected success');}catch(error){results.push(error.message);}}return results;})()`,
    awaitPromise:true,returnByValue:true,
  });
  assert(!invalidCeilings.exceptionDetails,JSON.stringify(invalidCeilings.exceptionDetails));
  for(const message of invalidCeilings.result.value) assert.match(message,/invalid host budget ceiling|cannot enforce a different memory ceiling/);
  const cancellation = await call("Runtime.evaluate", {
    expression: `(async () => {
      const {run}=await import(${JSON.stringify(base+"/loop/varta-host.js")});
      const early=new AbortController();early.abort();
      let earlyResult;
      try {await run({signal:early.signal});} catch(error) {earlyResult={message:error.message,started:error.guestStarted};}
      const active=new AbortController();
      const OriginalWorker=globalThis.Worker;
      globalThis.Worker=class extends OriginalWorker {
        constructor(...args){super(...args);this.addEventListener('message',({data})=>{if(data.type==='entered')setTimeout(()=>active.abort(),0);});}
      };
      try {await run({signal:active.signal,budgetCeiling:{fuelPerTurn:1000000000}});throw new Error('abort did not stop execution');}
      catch(error) {return {early:earlyResult,active:{message:error.message,started:error.guestStarted}};}
      finally {globalThis.Worker=OriginalWorker;}
    })()`, awaitPromise: true, returnByValue:true,
  });
  assert(!cancellation.exceptionDetails,JSON.stringify(cancellation.exceptionDetails));
  assert.match(cancellation.result.value.early.message,/aborted/);
  assert.equal(cancellation.result.value.early.started,false);
  assert.match(cancellation.result.value.active.message,/aborted/);
  assert.equal(cancellation.result.value.active.started,true);
  // Same-length changes must fail the digest check before the first guest effect.
  for (const name of ["app.wasm", "app.private.wasm", "app.manifest.json", "varta-worker.js"]) {
    const path = join(root, "ok", name);
    const original = await readFile(path);
    const changed = Buffer.from(original); changed[changed.length - 1] ^= 1;
    try {
      await writeFile(path, changed);
      const result = await run("ok");
      assert.equal(result.status, "failed", name);
      assert.match(result.text, /integrity mismatch/, name);
      assert.doesNotMatch(result.text, /varta log:/, name);
    } finally { await writeFile(path, original); }
  }
  const boundedPath=join(root,"ok","app.wasm");
  const original=await readFile(boundedPath);
  for(const changed of [Buffer.concat([original,Buffer.from([0])]),original.subarray(0,original.length-1)]) {
    try {
      await writeFile(boundedPath,changed);
      const result=await run("ok");
      assert.equal(result.status,"failed");
      assert.match(result.text,/length mismatch/);
      assert.doesNotMatch(result.text,/varta log:/);
    } finally {await writeFile(boundedPath,original);}
  }
  console.log("Real Chrome bundle acceptance passed: native output/chunked UTF16/async/timers/network/CORS/permission ceilings/FIFO/late handling/rejection snapshots and retention/fuel/call/output limits, running-Worker deadline, early/active abort, four invalid ceilings, four integrity and two length substitutions");
} finally {
  socket?.close();
  if (child && child.exitCode === null && child.signalCode === null) {
    child.kill("SIGTERM");
    await Promise.race([new Promise((done) => child.once("exit", done)), new Promise((done) => setTimeout(done, 3000))]);
    if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
  }
  await Promise.all([server,networkServer].map((listener)=>new Promise((done)=>listener.close(done))));
  await rm(profile, { recursive: true, force: true });
}
