// Prompt callback seam with real Wasm, trusted view gestures and native oracles.
import {createServer} from 'node:http';
import {readFile,writeFile} from 'node:fs/promises';
import {resolve,sep,join} from 'node:path';
import assert from 'node:assert/strict';
import {withBrowserPage} from './browser-probe.mjs';
const root=resolve(process.argv[2]);
const server=createServer(async(request,response)=>{
  try{
    const route=new URL(request.url,'http://loopback').pathname;
    if(route==='/__embed'){response.setHeader('Content-Type','text/html');response.end(`<!doctype html><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; worker-src blob:; trusted-types varta-worker 'allow-duplicates'; require-trusted-types-for 'script'"><title>Consent test</title>`);return;}
    const path=resolve(root,`.${route}`);if(!path.startsWith(root+sep))throw new Error('outside fixture');response.setHeader('Content-Type',path.endsWith('.js')?'text/javascript':path.endsWith('.html')?'text/html':'application/octet-stream');response.setHeader('Cache-Control','no-store');response.end(await readFile(path));
  }catch{response.writeHead(404).end();}
});
await new Promise(done=>server.listen(0,'127.0.0.1',done));const base=`http://127.0.0.1:${server.address().port}`;
const evaluate=async(call,expression)=>{const result=await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});assert(!result.exceptionDetails,JSON.stringify(result.exceptionDetails));return result.result.value;};
const wait=async(call,predicate)=>{for(let i=0;i<250;i++){if(await evaluate(call,predicate))return;await new Promise(done=>setTimeout(done,20));}throw new Error(`condition did not settle: ${predicate}; ${JSON.stringify(await evaluate(call,"typeof probe==='undefined'?document.body.textContent:probe"))}`);};
const click=async(call)=>{const{root}=await call('DOM.getDocument',{depth:-1,pierce:true});const find=node=>node.nodeName==='BUTTON'?node:[...(node.children??[]),...(node.shadowRoots??[])].map(find).find(Boolean);const button=find(root);assert(button);const{model}=await call('DOM.getBoxModel',{nodeId:button.nodeId}),q=model.content,x=(q[0]+q[2])/2,y=(q[1]+q[5])/2;await call('Input.dispatchMouseEvent',{type:'mousePressed',x,y,button:'left',clickCount:1});await call('Input.dispatchMouseEvent',{type:'mouseReleased',x,y,button:'left',clickCount:1});};
const normalized=report=>({...report,audit:report.audit.map(({at_ms,...entry})=>entry)});
const find=(node,predicate)=>predicate(node)?node:[...(node.children??[]),...(node.shadowRoots??[])].map(child=>find(child,predicate)).find(Boolean);
const attribute=(node,key,value)=>node.attributes?.some((name,index)=>index%2===0&&name===key&&node.attributes[index+1]===value);
const decisionNode=async(call,answer)=>{const{root}=await call('DOM.getDocument',{depth:-1,pierce:true});const button=find(root,node=>attribute(node,'data-decision',answer));assert(button);return button;};
const choose=async(call,answer)=>{const button=await decisionNode(call,answer),{model}=await call('DOM.getBoxModel',{nodeId:button.nodeId}),q=model.content,x=(q[0]+q[2])/2,y=(q[1]+q[5])/2;await call('Input.dispatchMouseEvent',{type:'mousePressed',x,y,button:'left',clickCount:1});await call('Input.dispatchMouseEvent',{type:'mouseReleased',x,y,button:'left',clickCount:1});};
const setup=async(call,name,extra='')=>evaluate(call,`(async()=>{
  Object.defineProperty(navigator,'permissions',{configurable:true,value:{query:async()=>({state:'granted'})}});
  const{mount,createDecisionStore,createPermissionPrompt}=await import(${JSON.stringify(base+'/'+name+'/varta-host.js')});globalThis.probe={logs:[],audit:[],prompts:[],turns:[],ready:false};
  globalThis.store=createDecisionStore();const container=document.createElement('div');document.body.append(container);
  globalThis.app=mount({container,decisionStore:store,budgetCeiling:{deadlinePerTurnMs:700},onLog:line=>probe.logs.push(line),onAudit:entry=>probe.audit.push(entry),onTurn:usage=>probe.turns.push(usage),onPrompt:(info,{signal})=>{probe.prompts.push(info);return new Promise(resolve=>{globalThis.answerPrompt=resolve;signal.addEventListener('abort',()=>{probe.aborted=true;},{once:true});});},${extra}});
  app.ready.then(()=>probe.ready=true,error=>probe.readyError=error.message);app.finished.then(value=>probe.result=value,error=>probe.result={status:'failed',error:error.message,guestStarted:error.guestStarted,logs:error.logs,permissions:error.permissions});return true;
})()`);
try{
  for(const name of ['consent-optional','consent-required','consent-parallel'])for(const answer of ['allow','deny','dismiss']){
    const golden=JSON.parse(await readFile(join(root,`${name}.${answer}.golden.json`),'utf8'));
    await withBrowserPage(`${base}/__embed`,async(call)=>{
      await setup(call,name);
      if(name==='consent-required'){
        await wait(call,'probe.prompts.length===1');assert.deepEqual(await evaluate(call,'probe.logs'),[]);assert.equal(await evaluate(call,'probe.ready'),false);
        if(answer==='allow'){await new Promise(done=>setTimeout(done,1000));assert.equal(await evaluate(call,'probe.result===undefined'),true,'human wait must not hit the 700ms startup watchdog');}
        await evaluate(call,`answerPrompt(${JSON.stringify(answer)});true`);
        if(answer!=='allow'){
          await wait(call,'probe.result!==undefined');const value=await evaluate(call,'probe.result');assert.equal(value.guestStarted,false);assert.match(value.error,/refused at first run/);assert.deepEqual(value.logs,golden.logs);assert.equal(value.permissions.decisions['clipboard.write'],answer==='deny'?'deny':undefined);return;
        }
      }
      await wait(call,'probe.ready');assert.equal(await evaluate(call,'probe.prompts.length'),name==='consent-required'?1:0,'background requests cannot prompt');
      for(let i=0;i<3;i++){
        const before=await evaluate(call,"probe.logs.filter(line=>line.startsWith('varta log: answer ')).length");
        await click(call);
        const shouldPrompt=name!=='consent-required'&&(i===0||answer==='dismiss');
        if(shouldPrompt){await wait(call,`probe.prompts.length===${i===0?1:i+1}`);assert.equal(await evaluate(call,"probe.logs.filter(line=>line.startsWith('varta log: answer ')).length"),before);
          if(i===0&&name==='consent-optional'&&answer==='allow'){
            const checkpoint=await evaluate(call,'probe.turns.at(-1)');await new Promise(done=>setTimeout(done,1000));assert.equal(await evaluate(call,'probe.result===undefined'),true);assert.deepEqual(await evaluate(call,'probe.turns.at(-1)'),checkpoint,'waiting does not run a guest turn');
          }
          await evaluate(call,`answerPrompt(${JSON.stringify(answer)});true`);
        }
        await wait(call,`probe.logs.filter(line=>line.startsWith('varta log: answer ')).length===${before+1}`);
      }
      await wait(call,`probe.logs.length===${golden.logs.length}&&probe.turns.at(-1)?.turns===${golden.usage.turns}`);
      await evaluate(call,'app.close();true');await wait(call,'probe.result!==undefined');const value=await evaluate(call,'probe.result');
      assert.deepEqual(value.logs,golden.logs,`${name}/${answer}`);assert.deepEqual(value.usage,golden.usage,`${name}/${answer}`);assert.deepEqual(normalized(value.permissions),normalized(golden.permissions),`${name}/${answer}`);
      const prompts=await evaluate(call,'probe.prompts');assert.equal(prompts.length,name==='consent-required'||answer!=='dismiss'?1:3);
      for(const info of prompts){assert.equal(info.capability,'clipboard.write');assert.equal(info.required,name==='consent-required');assert.equal(info.purpose,'Quoted <package> purpose');assert.deepEqual(info.scope,[]);}
    });
  }
  for(const action of ['close','revoke'])await withBrowserPage(`${base}/__embed`,async(call)=>{
    await setup(call,'consent-optional');await wait(call,'probe.ready');await click(call);await wait(call,'probe.prompts.length===1');
    if(action==='close'){
      await evaluate(call,"app.close();answerPrompt('allow');true");await wait(call,'probe.result!==undefined');const state=await evaluate(call,'probe');assert.equal(state.result.status,'closed');assert.deepEqual(state.logs,['varta log: initial prompt prompt']);assert.equal(await evaluate(call,'(async()=> (await store.read()).applications.length)()'),0);
    }else{
      assert.equal(await evaluate(call,"app.revoke('clipboard.write')"),true);await evaluate(call,"answerPrompt('allow');true");await wait(call,"probe.logs.includes('varta log: answer denied')");await evaluate(call,'app.close();true');await wait(call,'probe.result!==undefined');assert.equal(await evaluate(call,"probe.result.permissions.decisions['clipboard.write']"),'deny');
    }
  });
  await withBrowserPage(`${base}/__embed`,async(call)=>{
    await setup(call,'consent-optional',"decisionStore:{read:options=>store.read(options),set:async(...args)=>{if(args[2]==='allow')await new Promise(resolve=>globalThis.finishSave=resolve);return store.set(...args);}},");
    await wait(call,'probe.ready');await click(call);await wait(call,'probe.prompts.length===1');await evaluate(call,"answerPrompt('allow');true");await wait(call,"typeof finishSave==='function'");
    await evaluate(call,"globalThis.revoked=app.revoke('clipboard.write').then(value=>probe.revoked=value);true");
    assert.deepEqual(await evaluate(call,'probe.audit.map(entry=>entry.sequence)'),[1,2,3]);assert.equal(await evaluate(call,'probe.revoked===undefined'),true);
    await evaluate(call,'finishSave();revoked');await wait(call,"probe.logs.includes('varta log: answer denied')");
    assert.equal(await evaluate(call,"(async()=>(await store.read()).applications[0].decisions[0].decision)()"),'deny','slow allow storage cannot overwrite later revocation');assert.deepEqual(await evaluate(call,'probe.audit.map(entry=>entry.sequence)'),[1,2,3]);await evaluate(call,'app.close();true');
  });
  for(const answer of ['allow','deny','escape','abort'])await withBrowserPage(`${base}/__embed`,async(call)=>{
    await setup(call,'consent-required','onPrompt:createPermissionPrompt(),');await wait(call,"document.querySelectorAll('[data-varta-permission-prompt]').length===1");assert.equal(await evaluate(call,'probe.ready'),false);
    const{root:tree}=await call('DOM.getDocument',{depth:-1,pierce:true});const dialog=find(tree,node=>node.nodeName==='DIALOG');assert(dialog);
    const object=await call('DOM.resolveNode',{nodeId:dialog.nodeId});const contents=await call('Runtime.callFunctionOn',{objectId:object.object.objectId,functionDeclaration:"function(){return{text:this.textContent,parsedPackage:this.querySelector('package')!==null,open:this.open};}",returnByValue:true});
    assert.equal(contents.result.value.parsedPackage,false);assert.equal(contents.result.value.open,true);assert(contents.result.value.text.includes('Quoted <package> purpose'));assert(contents.result.value.text.includes('clipboard.write'));assert(contents.result.value.text.includes('sha256:'));assert(contents.result.value.text.includes('required to start'));
    const button=await decisionNode(call,'allow'),resolved=await call('DOM.resolveNode',{nodeId:button.nodeId});await call('Runtime.callFunctionOn',{objectId:resolved.object.objectId,functionDeclaration:'function(){this.click();}'});assert.equal(await evaluate(call,'probe.ready'),false,'synthetic click cannot grant');
    if(answer==='allow'){const screenshot=await call('Page.captureScreenshot',{format:'png'});await writeFile(join(root,'permission-dialog.png'),Buffer.from(screenshot.data,'base64'));await choose(call,'allow');await wait(call,'probe.ready');await evaluate(call,'app.close();true');}
    else if(answer==='deny')await choose(call,'deny');
    else if(answer==='escape'){await call('Input.dispatchKeyEvent',{type:'keyDown',key:'Escape',code:'Escape',windowsVirtualKeyCode:27});await call('Input.dispatchKeyEvent',{type:'keyUp',key:'Escape',code:'Escape',windowsVirtualKeyCode:27});}
    else await evaluate(call,'app.close();true');
    await wait(call,'probe.result!==undefined');assert.equal(await evaluate(call,"document.querySelectorAll('[data-varta-permission-prompt]').length"),0);
    const result=await evaluate(call,'probe.result');if(answer==='allow'||answer==='abort')assert.equal(result.status,'closed');else{assert.equal(result.guestStarted,false);assert.match(result.error,/refused at first run/);assert.equal(result.permissions.decisions['clipboard.write'],answer==='deny'?'deny':undefined);}
  });
  for(const answer of ['allow','deny'])await withBrowserPage(`${base}/consent-required/index.html`,async(call)=>{
    await wait(call,"document.querySelectorAll('[data-varta-permission-prompt]').length===1");await choose(call,answer);
    const expected=answer==='allow'?'ready':'failed';await wait(call,`document.querySelector('#varta-output').dataset.status===${JSON.stringify(expected)}`);
    const stored=await evaluate(call,`(async()=>{const{createDecisionStore}=await import(${JSON.stringify(base+'/consent-required/varta-host.js')});return await createDecisionStore().read();})()`);assert.equal(stored.applications.length,1);assert.equal(stored.applications[0].decisions[0].decision,answer);
    await call('Page.reload');await wait(call,`document.readyState==='complete'&&document.querySelector('#varta-output')?.dataset.status===${JSON.stringify(expected)}`);
    assert.equal(await evaluate(call,"document.querySelectorAll('[data-varta-permission-prompt]').length"),0,'saved choices must not prompt again');
    if(answer==='deny'){assert.equal(await evaluate(call,"document.querySelector('#varta-output').dataset.guestStarted"),'false');assert.match(await evaluate(call,"document.querySelector('#varta-output').textContent"),/stored decision/);}
  });
  console.log('Browser consent matches native: required first run and optional gesture answers, exact usage/audits, one open prompt, bounded dismissals, new host-task turns, uncharged human waits, abort and stale-allow revocation protection');
}finally{await new Promise(done=>server.close(done));}
