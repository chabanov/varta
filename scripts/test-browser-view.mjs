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
      response.setHeader("Content-Type", "text/html"); response.end('<!doctype html><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src \'none\'; script-src \'self\' \'wasm-unsafe-eval\'; connect-src \'self\'; style-src \'none\'"><main id="view"></main>'); return;
    }
    const path = resolve(root, `.${decodeURIComponent(name)}`); if (!path.startsWith(root + sep)) throw new Error("outside workspace");
    const bytes = await readFile(path); response.setHeader("Content-Type", path.endsWith(".wasm") ? "application/wasm" : "text/javascript"); response.end(bytes);
  } catch { response.writeHead(404).end(); }
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
try {
  await withBrowserPage(`http://127.0.0.1:${server.address().port}/__view_probe`, async (call) => {
    const result = await call("Runtime.evaluate", { awaitPromise: true, returnByValue: true, expression: `(async()=>{
      globalThis.viewEngine??=await WebAssembly.compileStreaming(fetch('/crates/varta-project/src/browser/view-engine.wasm'));const {createViewDom}=await import('/crates/varta-project/src/browser/view-dom.mjs');
      const {encodeViewBatch}=await import('/crates/varta-project/src/browser/view-codec.mjs');
      const events=[];let title='';const host=document.querySelector('#view');
      const app=createViewDom({viewEngine,container:host,onEvent:(event)=>events.push({bytes:Array.from(event.bytes),gesture:event.gesture}),onTitle:(value)=>{title=value;}});
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
      // CSS-wide font-family keywords fail as indexed protocol errors before
      // the batch commits, in set_style and in stylesheets.
      const refusal=(ops)=>{const html=host.firstElementChild&&button.outerHTML;try{app.apply(batch(...ops));return null;}catch(error){return {name:error.constructor.name,rule:error.rule,operation:error.operation,unchanged:button.outerHTML===html};}};
      const fontKeyword=refusal([op('set_text',{node:3,text:'must roll back'}),op('set_style',{node:2,property:'font-family',text:'serif, inherit'})]);
      const sheetKeyword=refusal([op('set_text',{node:3,text:'must roll back'}),op('stylesheet',{sheet:1,text:'.a { font-family: initial }'})]);
      return {fontKeyword,sheetKeyword,rollback,unchanged,closedShadow:host.firstElementChild.shadowRoot===null,title,text:button.textContent,color:getComputedStyle(button).color,events,stale,fresh,stable,assetDenied,nodes:app.snapshot().nodes.length};
    })()` });
    assert(!result.exceptionDetails, JSON.stringify(result.exceptionDetails));
    const value = result.result.value;
    assert.equal(value.rollback, true); assert.equal(value.unchanged, true); assert.equal(value.closedShadow, true);
    assert.equal(value.title, "Varta view"); assert.equal(value.text, "Привіт 😀"); assert.equal(value.color, "rgb(18, 52, 86)");
    assert.equal(value.stale, "local edit"); assert.equal(value.fresh, "acknowledged"); assert.equal(value.stable, true); assert.equal(value.assetDenied, true); assert.equal(value.nodes, 4);
    assert.deepEqual(value.fontKeyword, { name: "ViewProtocolError", rule: "font-family keyword", operation: 1, unchanged: true });
    assert.deepEqual(value.sheetKeyword, { name: "ViewProtocolError", rule: "stylesheet value", operation: 1, unchanged: true });
    assert.equal(value.events.length, 2); assert.equal(value.events[0].gesture, false); assert.equal(value.events[1].gesture, false);
    if (process.env.VARTA_VIEW_ORACLE) {
      const oracle=JSON.parse(await readFile(resolve(process.env.VARTA_VIEW_ORACLE),'utf8'));
      assert(oracle.flex,'native flex projection fixture missing');
      const comparison=await call('Runtime.evaluate',{awaitPromise:true,returnByValue:true,expression:`(async()=>{globalThis.viewEngine??=await WebAssembly.compileStreaming(fetch('/crates/varta-project/src/browser/view-engine.wasm'));const {createViewDom}=await import('/crates/varta-project/src/browser/view-dom.mjs');const host=document.createElement('div');document.body.append(host);const app=createViewDom({viewEngine,container:host});app.apply(Uint8Array.from(${JSON.stringify(oracle.flex.batch)}));const value=getComputedStyle(app.inspectNode(3)).display;app.close();host.remove();return value;})()`});
      assert(!comparison.exceptionDetails,JSON.stringify(comparison.exceptionDetails));
      const cascaded=oracle.flex.cascade.tree.children[0].children[0].style.display;
      const projected=oracle.flex.projected.tree.children[0].children[0].style.display;
      assert.equal(cascaded,'inline');assert.equal(projected,'block');assert.equal(comparison.result.value,projected);
      const selectStates=oracle.cases.filter(item=>item.name.startsWith('select-state-'));
      assert(selectStates.length,'native select state fixtures missing');
      const stateResult=await call('Runtime.evaluate',{awaitPromise:true,returnByValue:true,expression:`(async()=>{
        globalThis.viewEngine??=await WebAssembly.compileStreaming(fetch('/crates/varta-project/src/browser/view-engine.wasm'));const {createViewDom}=await import('/crates/varta-project/src/browser/view-dom.mjs');
        let checks=0;const mismatches=[];
        const expectedFields=root=>{const fields=[],stack=[root];while(stack.length){const node=stack.pop();
          if(node.tag==='select')fields.push({id:node.id,value:node.value,seq:node.seq,selected:(node.children??[]).filter(option=>option.selected).map(option=>option.id)});
          stack.push(...node.children??[]);}return fields.sort((a,b)=>a.id-b.id);};
        for(const fixture of ${JSON.stringify(selectStates)}){
          const host=document.createElement('div');document.body.append(host);const app=createViewDom({viewEngine,container:host});
          for(let i=0;i<fixture.batches.length;i++){
            let ok=true;try{app.apply(Uint8Array.from(fixture.batches[i]));}catch{ok=false;}
            const expected=fixture.results[i];if(ok!==expected.ok)mismatches.push([fixture.name,i,'admission']);
            const fields=app.snapshot().nodes.filter(node=>node.tag==='select').map(node=>({id:node.id,value:app.inspectNode(node.id).value,seq:node.seq,
              selected:node.children.filter(id=>app.inspectNode(id).selected)})).sort((a,b)=>a.id-b.id);
            if(JSON.stringify(fields)!==JSON.stringify(expectedFields(expected.snapshot.tree)))mismatches.push([fixture.name,i,'DOM selectedness']);checks++;
          }
          app.close();host.remove();
        }
        return {checks,mismatches};
      })()`});
      assert(!stateResult.exceptionDetails,JSON.stringify(stateResult.exceptionDetails));assert.deepEqual(stateResult.result.value.mismatches,[]);
      console.log(`Native/Chrome disabled select, explicit none and reinsertion: ${stateResult.result.value.checks} batches passed`);
      const workCases=oracle.cases.filter(item=>/^(select|radio|css|cascade|expanded|naming)-work-|^retained-text-|^tree-depth-/.test(item.name));
      assert(workCases.some(item=>item.name.startsWith('radio-work-')),'native radio work fixtures missing');
      const work=await call('Runtime.evaluate',{awaitPromise:true,returnByValue:true,expression:`(async()=>{
        globalThis.viewEngine??=await WebAssembly.compileStreaming(fetch('/crates/varta-project/src/browser/view-engine.wasm'));const {createViewDom}=await import('/crates/varta-project/src/browser/view-dom.mjs');
        let checks=0;const mismatches=[];
        for(const fixture of ${JSON.stringify(workCases)}){
          const host=document.createElement('div');document.body.append(host);let title='';
          const app=createViewDom({viewEngine,container:host,onTitle:value=>{title=value;}});
          const signature=()=>JSON.stringify({model:app.snapshot(),text:app.inspectNode(1).textContent,
            index:app.inspectNode(2)?.selectedIndex,value:app.inspectNode(2)?.value,title});
          for(let i=0;i<fixture.batches.length;i++){
            const before=signature(),expected=fixture.results[i];let ok=true,error;
            try{app.apply(Uint8Array.from(fixture.batches[i]));}catch(caught){ok=false;error=caught;}
            if(ok!==expected.ok)mismatches.push([fixture.name,i,'acceptance']);
            if(!ok&&signature()!==before)mismatches.push([fixture.name,i,'rollback']);
            if(expected.budget&&(error?.rule!=='select work budget'||(error.operation??null)!==expected.operation))mismatches.push([fixture.name,i,'budget position']);
            if(expected.radioBudget&&(error?.rule!=='radio work budget'||(error.operation??null)!==expected.radioOperation))mismatches.push([fixture.name,i,'radio budget position']);
            if(expected.cascadeBudget&&(error?.rule!=='cascade work budget'||(error.operation??null)!==expected.cascadeOperation))mismatches.push([fixture.name,i,'cascade work position']);
            if(expected.expandedBudget&&(error?.rule!=='expanded value budget'||(error.operation??null)!==expected.expandedOperation))mismatches.push([fixture.name,i,'expanded value position']);
            if(expected.depthBudget&&(error?.rule!=='tree depth budget'||(error.operation??null)!==expected.depthOperation))mismatches.push([fixture.name,i,'tree depth position']);
            if(expected.retainedBudget&&(error?.rule!=='retained text budget'||error.operation!==undefined))mismatches.push([fixture.name,i,'retained text budget']);
            if(expected.stylesheetBudget&&(error?.rule!=='stylesheet work budget'||(error.operation??null)!==expected.stylesheetOperation))mismatches.push([fixture.name,i,'stylesheet work']);
            if(app.snapshot().title!==expected.snapshot.title)mismatches.push([fixture.name,i,'title']);
            const field=expected.snapshot.tree.children?.[0];
            if(field?.tag==='select'&&app.inspectNode(2).value!==field.value)mismatches.push([fixture.name,i,'DOM value']);
            if(field?.style?.color==='#123456'&&getComputedStyle(app.inspectNode(2)).color!=='rgb(18, 52, 86)')mismatches.push([fixture.name,i,'prepared stylesheet index']);
            checks++;
          }
          app.close();host.remove();
        }
        return {checks,mismatches};
      })()`});
      assert(!work.exceptionDetails,JSON.stringify(work.exceptionDetails));
      assert.deepEqual(work.result.value.mismatches,[]);
      console.log(`Native/Chrome resource admission and rollback: ${work.result.value.checks} batches passed`);
      const radioCases=oracle.cases.filter(item=>item.name.startsWith('radio-'));
      assert(radioCases.length,'native radio group fixture missing');
      const radios=await call('Runtime.evaluate',{awaitPromise:true,returnByValue:true,expression:`(async()=>{
        globalThis.viewEngine??=await WebAssembly.compileStreaming(fetch('/crates/varta-project/src/browser/view-engine.wasm'));const {createViewDom}=await import('/crates/varta-project/src/browser/view-dom.mjs');
        let checks=0;const mismatches=[];
        for(const fixture of ${JSON.stringify(radioCases)}){
          const host=document.createElement('div');document.body.append(host);const app=createViewDom({viewEngine,container:host});
          for(let i=0;i<fixture.batches.length;i++){
            let ok=true;try{app.apply(Uint8Array.from(fixture.batches[i]));}catch{ok=false;}
            if(ok!==fixture.results[i].ok)mismatches.push([fixture.name,i,'acceptance']);
            for(const node of app.snapshot().nodes){
              if(node.tag==='input'&&node.attributes.type==='radio'){
                if(app.inspectNode(node.id).checked!==node.checked)mismatches.push([fixture.name,i,node.id]);
                checks++;
              }
            }
          }
          app.close();host.remove();
        }
        return {checks,mismatches};
      })()`});
      assert(!radios.exceptionDetails,JSON.stringify(radios.exceptionDetails));
      assert.deepEqual(radios.result.value.mismatches,[]);
      console.log(`Native/model/Chrome radio ownership and patch rollback: ${radios.result.value.checks} checks passed`);
      assert(oracle.checkables,'native checkbox/radio activation fixture missing');
      const setup=await call('Runtime.evaluate',{awaitPromise:true,returnByValue:true,expression:`(async()=>{
        globalThis.viewEngine??=await WebAssembly.compileStreaming(fetch('/crates/varta-project/src/browser/view-engine.wasm'));const {createViewDom}=await import('/crates/varta-project/src/browser/view-dom.mjs');
        const host=document.createElement('div');document.body.append(host);const events=[];
        const app=createViewDom({viewEngine,container:host,onEvent:event=>events.push(Array.from(event.bytes))});
        app.apply(Uint8Array.from(${JSON.stringify(oracle.checkables.batch)}));
        window.__checkables={app,host,events};
      })()`});
      assert(!setup.exceptionDetails,JSON.stringify(setup.exceptionDetails));
      try {
        for(const step of oracle.checkables.steps){
          const action=await call('Runtime.evaluate',{returnByValue:true,expression:`(()=>{
            const {app,events}=window.__checkables;app.apply(Uint8Array.from(${JSON.stringify(step.patch)}));events.length=0;
            const field=app.inspectNode(${step.node});${step.arrow?'field.focus();':'field.click();'}
          })()`});
          assert(!action.exceptionDetails,JSON.stringify(action.exceptionDetails));
          if(step.arrow){
            for(const type of ['keyDown','keyUp'])await call('Input.dispatchKeyEvent',{type,key:'ArrowRight',code:'ArrowRight',windowsVirtualKeyCode:39});
          }
          const observed=await call('Runtime.evaluate',{returnByValue:true,expression:`(()=>{
            const {app,events}=window.__checkables;
            return {events,fields:app.snapshot().nodes.filter(n=>n.tag==='input').map(n=>({id:n.id,checked:n.checked,seq:n.seq}))};
          })()`});
          assert(!observed.exceptionDetails,JSON.stringify(observed.exceptionDetails));
          assert.deepEqual(observed.result.value,{events:step.events,fields:step.fields},`checkable ${step.node}, arrow=${step.arrow}`);
        }
        console.log(`Native/Chrome checkbox/radio event bytes and default prevention: ${oracle.checkables.steps.length} scenarios passed`);
      } finally {
        await call('Runtime.evaluate',{expression:'window.__checkables.app.close();window.__checkables.host.remove();delete window.__checkables;'});
      }
    }
    if(process.env.VARTA_VIEW_ORACLE){
      const oracle=JSON.parse(await readFile(resolve(process.env.VARTA_VIEW_ORACLE),'utf8'));assert(oracle.selects,'native select fixture missing');
      const selected=await call('Runtime.evaluate',{awaitPromise:true,returnByValue:true,expression:`(async()=>{
        globalThis.viewEngine??=await WebAssembly.compileStreaming(fetch('/crates/varta-project/src/browser/view-engine.wasm'));const {createViewDom}=await import('/crates/varta-project/src/browser/view-dom.mjs');
        const {createViewModel}=await import('/crates/varta-project/src/browser/view-model.mjs');
        const {decodeViewEvent}=await import('/crates/varta-project/src/browser/view-codec.mjs');
        const fixture=${JSON.stringify(oracle.selects)},mirror=createViewModel({viewEngine}),events=[],metadata=[];
        mirror.commit(Uint8Array.from(fixture.batch));const host=document.createElement('div');document.body.append(host);
        const app=createViewDom({viewEngine,container:host,onEvent:event=>{events.push(Array.from(event.bytes));metadata.push(event.selectedOption);mirror.syncEvent(decodeViewEvent(event.bytes),event.selectedOption);}});
        const states=[];
        try{app.apply(Uint8Array.from(fixture.batch));
          for(const step of fixture.steps){events.length=0;metadata.length=0;
            mirror.commit(Uint8Array.from(step.patch));app.apply(Uint8Array.from(step.patch));
            const field=app.inspectNode(2),choices=app.snapshot().nodes.filter(n=>n.tag==='option');
            const id=option=>choices.find(n=>app.inspectNode(n.id)===option)?.id;
            if(step.choice!==null&&id(field.selectedOptions[0])!==step.choice){
              field.selectedIndex=Array.from(field.options).indexOf(app.inspectNode(step.choice));
              field.dispatchEvent(new Event('input',{bubbles:true,composed:true}));field.dispatchEvent(new Event('change',{bubbles:true,composed:true}));
            }
            const node=app.snapshot().nodes.find(n=>n.id===2),worker=mirror.snapshot().nodes.find(n=>n.id===2);
            const workerSelected=mirror.snapshot().nodes.filter(n=>n.tag==='option'&&n.optionSelected).map(n=>n.id);
            states.push({events:[...events],metadata:[...metadata],field:{value:node.value,seq:node.seq,selected:choices.filter(n=>app.snapshot().nodes.find(o=>o.id===n.id).optionSelected).map(n=>n.id)},
              dom:{value:field.value,seq:node.seq,selected:Array.from(field.selectedOptions).map(id)},worker:{value:worker.value,seq:worker.seq,selected:workerSelected}});
          }
        }finally{app.close();host.remove();mirror.close();}
        return states;
      })()`});
      assert(!selected.exceptionDetails,JSON.stringify(selected.exceptionDetails));
      for(const [index,state]of selected.result.value.entries()){
        const expected=oracle.selects.steps[index];assert.deepEqual(state.events,expected.events,`select events ${index}`);
        for(const field of ['field','dom','worker'])assert.deepEqual(state[field],expected.field,`select ${field} ${index}`);
        assert(state.metadata.every(id=>id===expected.choice),`select choice envelope ${index}`);
      }
      console.log(`Native/DOM/Worker select identity, events and mutation: ${oracle.selects.steps.length} scenarios passed`);
    }
    const input = new DataView(Uint8Array.from(value.events[1].bytes).buffer); assert.equal(input.getUint32(registry.view.wire.eventFields.find((field) => field.name === "seq").offset, true), 1);
    if (process.argv[2]) { const image = await call("Page.captureScreenshot", { format: "png" }); await writeFile(resolve(process.argv[2]), Buffer.from(image.data, "base64")); }
    console.log("Real Chrome view DOM boundary passed: atomic rejection, native fields, UTF16, CSSOM under CSP, events, stale edits, node identity and no ambient assets");
  });
} finally { await new Promise((resolve) => server.close(resolve)); }
