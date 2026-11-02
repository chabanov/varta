// Full generated loader/Worker view path, using real trusted CDP inputs.
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { resolve, join, sep } from "node:path";
import assert from "node:assert/strict";
import { withBrowserPage } from "./browser-probe.mjs";

const root = resolve(process.argv[2]);
const artifactFetches=new Map();
const server = createServer(async (request, response) => {
  try {
    if(new URL(request.url,"http://loopback").pathname==="/__passive_embed"){
      response.setHeader("Content-Type","text/html");response.end(`<!doctype html><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; worker-src blob:; trusted-types varta-worker 'allow-duplicates'; require-trusted-types-for 'script'"><main id="varta-view"></main><pre id="varta-output">host-owned output</pre>`);return;
    }
    const path = resolve(root, `.${new URL(request.url, "http://loopback").pathname}`); if (!path.startsWith(root + sep)) throw new Error("outside fixtures");
    if(path.endsWith("app.wasm")){const name=path.split(sep).at(-2);artifactFetches.set(name,(artifactFetches.get(name)??0)+1);}
    const bytes = await readFile(path); response.setHeader("Cache-Control", "no-store"); response.setHeader("Content-Type", path.endsWith(".js") ? "text/javascript" : path.endsWith(".html") ? "text/html" : "application/octet-stream"); response.end(bytes);
  } catch { response.writeHead(404).end(); }
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const base = `http://127.0.0.1:${server.address().port}`;
try {
  await withBrowserPage(base + "/view-live/index.html", async (call) => {
    const output = async () => {
      const r = await call("Runtime.evaluate", { expression: `(()=>{const e=document.querySelector('#varta-output');return{status:e.dataset.status,text:e.textContent,usage:e.dataset.usage?JSON.parse(e.dataset.usage):null};})()`, returnByValue: true });
      assert(!r.exceptionDetails, JSON.stringify(r.exceptionDetails)); return r.result.value;
    };
    const wait = async (predicate) => {
      const started = Date.now();
      while (true) { const value = await output(); if (predicate(value)) return value; if (value.status === "failed" || Date.now() - started > 10000) throw new Error(JSON.stringify(value)); await new Promise((resolve) => setTimeout(resolve, 20)); }
    };
    const native = JSON.parse(await readFile(join(root, "view-live.golden.json"), "utf8"));
    const initial = await wait((value) => value.status === "ready"); assert.equal(initial.text, native.logs.join("\n")); assert.deepEqual(initial.usage, native.usage);
    function find(node, id) {
      for (let i = 0; i < (node.attributes?.length ?? 0); i += 2) if (node.attributes[i] === "id" && node.attributes[i + 1] === id) return node;
      for (const child of [...(node.children ?? []), ...(node.shadowRoots ?? [])]) { const found = find(child, id); if (found) return found; }
    }
    const locate = async (id) => { const { root } = await call("DOM.getDocument", { depth: -1, pierce: true }); const node = find(root, id); assert(node, id); return node; };
    const click = async (id) => {
      const node = await locate(id), { model } = await call("DOM.getBoxModel", { nodeId: node.nodeId }), q = model.content;
      const x = (q[0] + q[2]) / 2, y = (q[1] + q[5]) / 2;
      await call("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1 }); await call("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1 });
    };
    await click("counter"); await wait((value) => value.text.endsWith("varta log: 1") && value.usage.turns === 2);
    await click("entry"); await call("Input.insertText", { text: "hello" }); await wait((value) => value.text.endsWith("varta log: hello") && value.usage.turns === 3);
    await click("counter"); const final = await wait((value) => value.text.endsWith("varta log: 2") && value.usage.turns === 4);
    const nativeEvents=JSON.parse(await readFile(join(root,"view-live.events.golden.json"),"utf8"));
    assert.equal(final.text,nativeEvents.logs.join("\n"));assert.deepEqual(final.usage,nativeEvents.usage);
    const button = await locate("counter"); const object = await call("DOM.resolveNode", { nodeId: button.nodeId });
    const text = await call("Runtime.callFunctionOn", { objectId: object.object.objectId, functionDeclaration: "function(){return this.textContent;}", returnByValue: true }); assert.equal(text.result.value, "2");
    // A bounded run renders and discards events, matching native R1.
    const bounded = await call("Runtime.evaluate", { expression: `(async()=>{const{run}=await import(${JSON.stringify(base + "/view-live/varta-host.js")});return await run();})()`, awaitPromise: true, returnByValue: true });
    assert(!bounded.exceptionDetails, JSON.stringify(bounded.exceptionDetails)); assert.equal(bounded.result.value.status, "ok"); assert.deepEqual(bounded.result.value.logs, native.logs);
    const closed=await call("Runtime.evaluate",{expression:`(async()=>{const{mount}=await import(${JSON.stringify(base+"/view-live/varta-host.js")});const container=document.createElement('div');document.body.append(container);const app=mount({container});await app.ready;app.close();const closed=await app.finished;const again=mount({container});await again.ready;again.close();const restarted=await again.finished;return {...closed,restarted:restarted.logs};})()`,awaitPromise:true,returnByValue:true});
    assert(!closed.exceptionDetails,JSON.stringify(closed.exceptionDetails));assert.equal(closed.result.value.status,"closed");assert.deepEqual(closed.result.value.logs,native.logs);assert.deepEqual(closed.result.value.restarted,native.logs);
    console.log("Generated view application passed: native initial usage, trusted click/input turns, persistent closure state, DOM updates and bounded rendering");
  });
  await withBrowserPage(`${base}/view-async/index.html`,async(call)=>{
    let approved=false;
    for(let i=0;i<250&&!approved;i++){
      const{root}=await call('DOM.getDocument',{depth:-1,pierce:true});const find=node=>node.attributes?.some((value,index)=>index%2===0&&value==='data-decision'&&node.attributes[index+1]==='allow')?node:[...(node.children??[]),...(node.shadowRoots??[])].map(find).find(Boolean);const button=find(root);
      if(button){const{model}=await call('DOM.getBoxModel',{nodeId:button.nodeId}),q=model.content,x=(q[0]+q[2])/2,y=(q[1]+q[5])/2;await call('Input.dispatchMouseEvent',{type:'mousePressed',x,y,button:'left',clickCount:1});await call('Input.dispatchMouseEvent',{type:'mouseReleased',x,y,button:'left',clickCount:1});approved=true;}else await new Promise(done=>setTimeout(done,20));
    }
    assert(approved,'generated timer application must request first-run consent');
    let result;const end=Date.now()+10000;
    do{const r=await call("Runtime.evaluate",{expression:"document.querySelector('#varta-output').dataset.status",returnByValue:true});result=r.result.value;if(result!=="ready")await new Promise((done)=>setTimeout(done,20));}while(result!=="ready"&&Date.now()<end);assert.equal(result,"ready");
    const {root:doc}=await call("DOM.getDocument",{depth:-1,pierce:true});const search=(node)=>node.nodeName==="BUTTON"?node:[...(node.children??[]),...(node.shadowRoots??[])].map(search).find(Boolean);const button=search(doc),{model}=await call("DOM.getBoxModel",{nodeId:button.nodeId}),q=model.content,x=(q[0]+q[2])/2,y=(q[1]+q[5])/2;
    await call("Input.dispatchMouseEvent",{type:"mousePressed",x,y,button:"left",clickCount:1});await call("Input.dispatchMouseEvent",{type:"mouseReleased",x,y,button:"left",clickCount:1});
    const golden=JSON.parse(await readFile(join(root,"view-async.events.golden.json"),"utf8"));const until=Date.now()+5000;
    do{const r=await call("Runtime.evaluate",{expression:"(()=>{const e=document.querySelector('#varta-output');return{status:e.dataset.status,text:e.textContent,usage:JSON.parse(e.dataset.usage)};})()",returnByValue:true});result=r.result.value;if(result.usage.turns!==golden.usage.turns)await new Promise((done)=>setTimeout(done,20));}while(result.usage.turns!==golden.usage.turns&&Date.now()<until);
    assert.equal(result.text,golden.logs.join("\n"));assert.deepEqual(result.usage,golden.usage);
  });
  for (const name of ["view-invalid", "view-second"]) await withBrowserPage(`${base}/${name}/index.html`, async (call) => {
    let result; const started = Date.now();
    do {
      const r = await call("Runtime.evaluate", { expression: `(()=>{const e=document.querySelector('#varta-output');return{status:e.dataset.status,text:e.textContent};})()`, returnByValue: true }); result = r.result.value;
      if (result.status !== "failed") await new Promise((resolve) => setTimeout(resolve, 20));
    } while (result.status !== "failed" && Date.now() - started < 10000);
    assert.equal(result.status, "failed", JSON.stringify(result)); assert(!result.text.includes("must not run"), result.text);
  });
  await withBrowserPage(`${base}/view-loop/index.html`,async(call)=>{
    const setup=await call("Runtime.evaluate",{expression:`(async()=>{const{mount}=await import(${JSON.stringify(base+"/view-loop/varta-host.js")});const container=document.createElement('div');document.body.append(container);globalThis.sessionProbe={logs:[]};const app=mount({container,budgetCeiling:{fuelPerTurn:1000000000,deadlinePerTurnMs:250},onLog:(line)=>sessionProbe.logs.push(line)});app.finished.then((result)=>sessionProbe.result=result,(error)=>sessionProbe.result={status:'failed',text:[...error.logs,error.message].join('\\n')});await app.ready;return true;})()`,awaitPromise:true,returnByValue:true});
    assert(!setup.exceptionDetails,JSON.stringify(setup.exceptionDetails));
    await new Promise((done)=>setTimeout(done,400));
    const idle=await call("Runtime.evaluate",{expression:"sessionProbe.result===undefined",returnByValue:true});assert.equal(idle.result.value,true,"idle time must not consume a turn deadline");
    const {root:doc}=await call("DOM.getDocument",{depth:-1,pierce:true});
    const buttons=[];const search=(node)=>{if(node.nodeName==="BUTTON")buttons.push(node);for(const child of [...(node.children??[]),...(node.shadowRoots??[])])search(child);};search(doc);
    const button=buttons.at(-1),{model}=await call("DOM.getBoxModel",{nodeId:button.nodeId}),q=model.content,x=(q[0]+q[2])/2,y=(q[1]+q[5])/2;
    await call("Input.dispatchMouseEvent",{type:"mousePressed",x,y,button:"left",clickCount:1});await call("Input.dispatchMouseEvent",{type:"mouseReleased",x,y,button:"left",clickCount:1});
    let result;const limit=Date.now()+5000;
    do{const r=await call("Runtime.evaluate",{expression:"sessionProbe.result",returnByValue:true});result=r.result.value;if(!result)await new Promise((done)=>setTimeout(done,20));}while(!result&&Date.now()<limit);
    assert.equal(result?.status,"failed");assert.match(result.text,/entered callback.*deadline exceeded/s);
  });
  await withBrowserPage(`${base}/__passive_embed`,async(call)=>{
    const setup=await call("Runtime.evaluate",{expression:`(async()=>{const{mount}=await import(${JSON.stringify(base+"/view-permissions/varta-host.js")});globalThis.permissionProbe={logs:[],clipboard:0,opened:0};Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async()=>{permissionProbe.clipboard++;}}});window.open=()=>{permissionProbe.opened++;throw new Error('unexpected open');};const container=document.createElement('div');document.body.append(container);globalThis.permissionApp=mount({container,permissionCeiling:{},onLog:(line)=>permissionProbe.logs.push(line),onTurn:(usage)=>permissionProbe.usage=usage});await permissionApp.ready;return true;})()`,awaitPromise:true,returnByValue:true});
    assert(!setup.exceptionDetails,JSON.stringify(setup.exceptionDetails));
    const locate=async(id)=>{const{root}=await call("DOM.getDocument",{depth:-1,pierce:true});const found=[];const visit=(node)=>{for(let i=0;i<(node.attributes?.length??0);i+=2)if(node.attributes[i]==='id'&&node.attributes[i+1]===id)found.push(node);for(const child of [...(node.children??[]),...(node.shadowRoots??[])])visit(child);};visit(root);return found.at(-1);};
    const click=async(id)=>{const node=await locate(id),{model}=await call("DOM.getBoxModel",{nodeId:node.nodeId}),q=model.content,x=(q[0]+q[2])/2,y=(q[1]+q[5])/2;await call("Input.dispatchMouseEvent",{type:"mousePressed",x,y,button:"left",clickCount:1});await call("Input.dispatchMouseEvent",{type:"mouseReleased",x,y,button:"left",clickCount:1});};
    const poll=async(text)=>{let result;const end=Date.now()+5000;do{const r=await call("Runtime.evaluate",{expression:"permissionProbe",returnByValue:true});result=r.result.value;if(!result.logs.includes(text))await new Promise((done)=>setTimeout(done,20));}while(!result.logs.includes(text)&&Date.now()<end);assert(result.logs.includes(text),JSON.stringify(result));return result;};
    await click('copy');await poll('varta log: copy 1');await click('external');const result=await poll('varta log: link 1');
    const golden=JSON.parse(await readFile(join(root,'view-permissions.denied.golden.json'),'utf8'));assert.deepEqual(result.logs,golden.logs);
    // Wait for the final turn's acknowledged accounting, after its streamed log.
    let usage;const end=Date.now()+5000;do{const r=await call("Runtime.evaluate",{expression:"permissionProbe.usage",returnByValue:true});usage=r.result.value;if(usage?.turns!==golden.usage.turns)await new Promise((done)=>setTimeout(done,20));}while(usage?.turns!==golden.usage.turns&&Date.now()<end);
    assert.deepEqual(usage,golden.usage);assert.equal(result.clipboard,0);assert.equal(result.opened,0);
    const link=await locate('external'),remote=await call('DOM.resolveNode',{nodeId:link.nodeId});
    const href=await call('Runtime.callFunctionOn',{objectId:remote.object.objectId,functionDeclaration:"function(){return this.getAttribute('href');}",returnByValue:true});assert.equal(href.result.value,'#');
    const before=await call('Runtime.evaluate',{expression:'location.href',returnByValue:true});
    await call('Runtime.evaluate',{expression:'permissionApp.close();permissionApp.finished',awaitPromise:true,returnByValue:true});
    await click('external');const after=await call('Runtime.evaluate',{expression:'location.href',returnByValue:true});assert.equal(after.result.value,before.result.value,'a frozen view must never navigate');
    const approved=await call("Runtime.evaluate",{expression:`(async()=>{globalThis.approvedProbe={logs:[],writes:[],opens:[]};Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async(text)=>approvedProbe.writes.push(text)}});window.open=(...args)=>{approvedProbe.opens.push(args);return null;};const{mount}=await import(${JSON.stringify(base+"/view-permissions/varta-host.js")});const container=document.createElement('div');document.body.append(container);globalThis.approvedApp=mount({container,permissionCeiling:{'clipboard.write':true,'ui.open_url':{patterns:['https://not-authorized.invalid/**']}},onLog:(line)=>approvedProbe.logs.push(line)});await approvedApp.ready;return true;})()`,awaitPromise:true,returnByValue:true});
    assert(!approved.exceptionDetails,JSON.stringify(approved.exceptionDetails));
    await click('copy');await click('external');let allowed;const allowedEnd=Date.now()+5000;
    do{const r=await call('Runtime.evaluate',{expression:'approvedProbe',returnByValue:true});allowed=r.result.value;if(allowed.logs.length<2)await new Promise((done)=>setTimeout(done,20));}while(allowed.logs.length<2&&Date.now()<allowedEnd);
    assert(allowed.logs.includes('varta log: copy 0'),JSON.stringify(allowed));assert(allowed.logs.includes('varta log: link 0'),JSON.stringify(allowed));assert.deepEqual(allowed.writes,['test clipboard']);assert.deepEqual(allowed.opens,[['https://not-authorized.invalid/path','_blank','noopener,noreferrer']]);

  });
  await withBrowserPage(`${base}/__passive_embed`,async(call)=>{
    const before=artifactFetches.get("view-permissions")??0;
    const passive=await call("Runtime.evaluate",{expression:`import(${JSON.stringify(base+"/view-permissions/varta-host.js")}).then(()=>document.querySelector('#varta-output').textContent)`,awaitPromise:true,returnByValue:true});
    assert(!passive.exceptionDetails,JSON.stringify(passive.exceptionDetails));assert.equal(passive.result.value,"host-owned output");await new Promise((done)=>setTimeout(done,100));
    assert.equal(artifactFetches.get("view-permissions")??0,before,"importing a loader must not start a package before the embedder supplies its ceiling");
  });
  console.log("Invalid batch and second commit fail before subsequent guest logging");
} finally { await new Promise((resolve) => server.close(resolve)); }
