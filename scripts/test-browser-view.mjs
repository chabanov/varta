// DOM-boundary acceptance. Does not claim a long-lived Worker session yet.
import { createServer } from "node:http";
import { readFile, writeFile } from "node:fs/promises";
import { resolve, sep } from "node:path";
import assert from "node:assert/strict";
import { registry } from "../crates/varta-common/browser/registry.mjs";
import { withBrowserPage } from "./browser-probe.mjs";

const root = resolve(import.meta.dirname, "..");
const server = createServer(async (request, response) => {
  try {
    const name = new URL(request.url, "http://loopback").pathname;
    if (name === "/__view_probe") {
      response.setHeader("Content-Type", "text/html"); response.end('<!doctype html><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src \'none\'; script-src \'self\'; style-src \'none\'"><main id="view"></main>'); return;
    }
    const path = resolve(root, `.${decodeURIComponent(name)}`); if (!path.startsWith(root + sep)) throw new Error("outside workspace");
    const bytes = await readFile(path); response.setHeader("Content-Type", "text/javascript"); response.end(bytes);
  } catch { response.writeHead(404).end(); }
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
try {
  await withBrowserPage(`http://127.0.0.1:${server.address().port}/__view_probe`, async (call) => {
    const result = await call("Runtime.evaluate", { awaitPromise: true, returnByValue: true, expression: `(async()=>{
      const {createViewDom}=await import('/crates/varta-project/src/browser/view-dom.mjs');
      const {encodeViewBatch}=await import('/crates/varta-project/src/browser/view-codec.mjs');
      const events=[];let title='';const host=document.querySelector('#view');
      const app=createViewDom({container:host,onEvent:(event)=>events.push({bytes:Array.from(event.bytes),gesture:event.gesture}),onTitle:(value)=>{title=value;}});
      const op=(name,fields={})=>({name,...fields}),batch=(...ops)=>encodeViewBatch(ops);
      app.apply(batch(op('create_element',{node:2,tag:'button'}),op('create_text',{node:3,text:'Привіт 😀'}),op('insert',{parent:2,node:3,before:0}),op('insert',{parent:1,node:2,before:0}),op('set_attribute',{node:2,attribute:'id',text:'button'}),op('set_style',{node:2,property:'color',text:'#123456'}),op('set_title',{text:'Varta view'}),op('listen',{node:2,event:'click',handler:1,options:1,key:0,required_modifiers:0,forbidden_modifiers:0})));
      const button=app.inspectNode(2),before=button.outerHTML;
      let rollback=false;try{app.apply(batch(op('set_text',{node:3,text:'must roll back'}),op('set_attribute',{node:1,attribute:'id',text:'button'})));}catch{rollback=true;}
      const unchanged=button.outerHTML===before;
      button.click();
      app.apply(batch(op('create_element',{node:4,tag:'input'}),op('insert',{parent:1,node:4,before:0}),op('listen',{node:4,event:'input',handler:2,options:0,key:0,required_modifiers:0,forbidden_modifiers:0})));
      const input=app.inspectNode(4);input.value='local edit';input.dispatchEvent(new InputEvent('input',{bubbles:true,composed:true}));
      app.apply(batch(op('set_value',{node:4,text:'stale server patch',base_seq:0})));
      const stale=input.value;
      app.apply(batch(op('set_value',{node:4,text:'acknowledged',base_seq:1})));
      const fresh=input.value;
      const stable=app.inspectNode(2)===button&&app.inspectNode(4)===input;
      let assetDenied=false;try{app.apply(batch(op('create_element',{node:5,tag:'img'}),op('set_attribute',{node:5,attribute:'src',text:'unpackaged.png'})));}catch{assetDenied=true;}
      return {rollback,unchanged,closedShadow:host.firstElementChild.shadowRoot===null,title,text:button.textContent,color:getComputedStyle(button).color,events,stale,fresh,stable,assetDenied,nodes:app.snapshot().nodes.length};
    })()` });
    assert(!result.exceptionDetails, JSON.stringify(result.exceptionDetails));
    const value = result.result.value;
    assert.equal(value.rollback, true); assert.equal(value.unchanged, true); assert.equal(value.closedShadow, true);
    assert.equal(value.title, "Varta view"); assert.equal(value.text, "Привіт 😀"); assert.equal(value.color, "rgb(18, 52, 86)");
    assert.equal(value.stale, "local edit"); assert.equal(value.fresh, "acknowledged"); assert.equal(value.stable, true); assert.equal(value.assetDenied, true); assert.equal(value.nodes, 4);
    assert.equal(value.events.length, 2); assert.equal(value.events[0].gesture, false); assert.equal(value.events[1].gesture, false);
    if (process.env.VARTA_VIEW_ORACLE) {
      const oracle=JSON.parse(await readFile(resolve(process.env.VARTA_VIEW_ORACLE),'utf8'));
      assert(oracle.flex,'native flex projection fixture missing');
      const comparison=await call('Runtime.evaluate',{awaitPromise:true,returnByValue:true,expression:`(async()=>{const {createViewDom}=await import('/crates/varta-project/src/browser/view-dom.mjs');const host=document.createElement('div');document.body.append(host);const app=createViewDom({container:host});app.apply(Uint8Array.from(${JSON.stringify(oracle.flex.batch)}));const value=getComputedStyle(app.inspectNode(3)).display;app.close();host.remove();return value;})()`});
      assert(!comparison.exceptionDetails,JSON.stringify(comparison.exceptionDetails));
      const cascaded=oracle.flex.cascade.tree.children[0].children[0].style.display;
      const projected=oracle.flex.projected.tree.children[0].children[0].style.display;
      assert.equal(cascaded,'inline');assert.equal(projected,'block');assert.equal(comparison.result.value,projected);
    }
    const input = new DataView(Uint8Array.from(value.events[1].bytes).buffer); assert.equal(input.getUint32(registry.view.wire.eventFields.find((field) => field.name === "seq").offset, true), 1);
    if (process.argv[2]) { const image = await call("Page.captureScreenshot", { format: "png" }); await writeFile(resolve(process.argv[2]), Buffer.from(image.data, "base64")); }
    console.log("Real Chrome view DOM boundary passed: atomic rejection, native fields, UTF16, CSSOM under CSP, events, stale edits, node identity and no ambient assets");
  });
} finally { await new Promise((resolve) => server.close(resolve)); }
