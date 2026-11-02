// Native oracle for headless broker states, source answers, fuel and audit rows.
import {createServer} from 'node:http';
import {readFile} from 'node:fs/promises';
import {resolve,join,sep} from 'node:path';
import assert from 'node:assert/strict';
import {withBrowserPage} from './browser-probe.mjs';
const root=resolve(process.argv[2]);let artifactFetches=0;
const server=createServer(async(request,response)=>{
  try{
    const route=new URL(request.url,'http://loopback').pathname;
    if(route==='/__embed'){response.setHeader('Content-Type','text/html');response.end(`<!doctype html><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; worker-src blob:; trusted-types varta-worker 'allow-duplicates'; require-trusted-types-for 'script'"><title>Permission test host</title>`);return;}
    const path=resolve(root,`.${route}`);if(!path.startsWith(root+sep))throw new Error('outside fixture');if(route.endsWith('/app.wasm'))artifactFetches++;
    const bytes=await readFile(path);response.setHeader('Content-Type',path.endsWith('.js')?'text/javascript':path.endsWith('.html')?'text/html':'application/octet-stream');response.setHeader('Cache-Control','no-store');response.end(bytes);
  }catch{response.writeHead(404).end();}
});
await new Promise(done=>server.listen(0,'127.0.0.1',done));
const base=`http://127.0.0.1:${server.address().port}`;
const normalized=(report)=>({...report,audit:report.audit.map(({at_ms,...row})=>row)});
try{
  for(const name of ['permission-query','permission-optional','permission-optional-allow','permission-optional-deny','permission-optional-other','permission-optional-empty','permission-required-deny','permission-required-deny-empty','permission-bad-nul','permission-bad-surrogate','permission-bad-utf8']){
    const golden=JSON.parse(await readFile(join(root,`${name}.golden.json`),'utf8'));
    let decisions;try{decisions=JSON.parse(await readFile(join(root,`${name}.decisions.json`),'utf8'));}catch{}
    await withBrowserPage(`${base}/__embed`,async(call)=>{
      const result=await call('Runtime.evaluate',{expression:`(async()=>{const{run}=await import(${JSON.stringify(base+'/'+name+'/varta-host.js')});const observed=[],logs=[];try{const value=await run({decisions:${JSON.stringify(decisions)??'undefined'},${name.endsWith('-empty')?'permissionCeiling:{},':''}onAudit:entry=>observed.push(entry),onLog:line=>logs.push(line)});return{...value,observed};}catch(error){return{status:'failed',error:error.message,logs,observed,guestStarted:error.guestStarted,permissions:error.permissions};}})()`,awaitPromise:true,returnByValue:true});
      assert(!result.exceptionDetails,JSON.stringify(result.exceptionDetails));const value=result.result.value;
      assert.equal(value.status,golden.status,name);assert.deepEqual(value.logs,golden.logs,name);
      if(golden.usage){const{fuelRemaining,hostCalls,outputBytes,microtasks,fuelUsed,turns}=value;assert.deepEqual({fuelRemaining,hostCalls,outputBytes,microtasks,fuelUsed,turns},golden.usage,name);}
      else{assert.equal(value.guestStarted,false);assert.match(value.error,/required permission time.timer.*stored decision/);}
      assert.deepEqual(normalized(value.permissions),normalized(golden.permissions),name);
      assert.deepEqual(value.observed,value.permissions.audit,name);
      let at=0;for(const entry of value.permissions.audit){assert(Number.isSafeInteger(entry.at_ms)&&entry.at_ms>=at);at=entry.at_ms;}
    });
  }
  await withBrowserPage(`${base}/__embed`,async(call)=>{
    const before=artifactFetches;
    const invalid=await call('Runtime.evaluate',{expression:`(async()=>{const{run}=await import(${JSON.stringify(base+'/permission-optional/varta-host.js')});try{await run({decisions:{applications:[{artifact:'sha256:'+'b'.repeat(64),decisions:[{id:'time.fake',decision:'allow'}]}]}});return 'unexpected';}catch(error){return error.message;}})()`,awaitPromise:true,returnByValue:true});
    assert.match(invalid.result.value,/invalid permission decisions/);assert.equal(artifactFetches,before,'validate even other-identity decisions before artifact fetch');
    const decisions=JSON.parse(await readFile(join(root,'permission-optional-allow.decisions.json'),'utf8'));
    const owned=await call('Runtime.evaluate',{expression:`(async()=>{const{run}=await import(${JSON.stringify(base+'/permission-optional-allow/varta-host.js')});const decisions=${JSON.stringify(decisions)};const result=run({decisions});decisions.applications[0].decisions[0].decision='deny';return await result;})()`,awaitPromise:true,returnByValue:true});
    assert(!owned.exceptionDetails,JSON.stringify(owned.exceptionDetails));assert(owned.result.value.logs.includes('varta log: timer'));assert.equal(owned.result.value.permissions.decisions['time.timer'],'allow');
  });
  for(const state of ['granted','denied','prompt','unsupported','empty'])await withBrowserPage(`${base}/__embed`,async(call)=>{
    const setup=await call('Runtime.evaluate',{expression:`(async()=>{const status=new EventTarget();status.state=${JSON.stringify(state)};globalThis.platformProbe={status,queries:0,added:0,removed:0,logs:[],audit:[]};const add=status.addEventListener.bind(status),remove=status.removeEventListener.bind(status);status.addEventListener=(...args)=>{platformProbe.added++;return add(...args);};status.removeEventListener=(...args)=>{platformProbe.removed++;return remove(...args);};Object.defineProperty(navigator,'permissions',{configurable:true,value:{query:async descriptor=>{platformProbe.queries++;if(descriptor.name!=='clipboard-write')throw new Error('unexpected permission');if(${JSON.stringify(state)}==='unsupported')throw new TypeError('unsupported permission');return status;}}});const{mount}=await import(${JSON.stringify(base+'/permission-platform/varta-host.js')});const container=document.createElement('div');document.body.append(container);globalThis.platformApp=mount({container,${state==='empty'?'permissionCeiling:{},':''}onLog:line=>platformProbe.logs.push(line),onAudit:entry=>platformProbe.audit.push(entry)});const ready=await platformApp.ready;return{logs:platformProbe.logs,queries:platformProbe.queries,usage:ready.usage};})()`,awaitPromise:true,returnByValue:true});
    assert(!setup.exceptionDetails,JSON.stringify(setup.exceptionDetails));const initial=state==='unsupported'?'granted':state==='empty'?'denied':state;
    assert.deepEqual(setup.result.value.logs,[`varta log: ${initial} ${initial}`]);assert.equal(setup.result.value.queries,state==='empty'?0:1);
    if(state==='granted'){
      const golden=JSON.parse(await readFile(join(root,'permission-platform.golden.json'),'utf8'));assert.deepEqual(setup.result.value.usage,golden.usage);
      await call('Runtime.evaluate',{expression:"platformProbe.status.state='denied';platformProbe.status.dispatchEvent(new Event('change'));",returnByValue:true});
      const {root:document}=await call('DOM.getDocument',{depth:-1,pierce:true});const find=node=>node.nodeName==='BUTTON'?node:[...(node.children??[]),...(node.shadowRoots??[])].map(find).find(Boolean);const button=find(document),{model}=await call('DOM.getBoxModel',{nodeId:button.nodeId}),q=model.content,x=(q[0]+q[2])/2,y=(q[1]+q[5])/2;
      await call('Input.dispatchMouseEvent',{type:'mousePressed',x,y,button:'left',clickCount:1});await call('Input.dispatchMouseEvent',{type:'mouseReleased',x,y,button:'left',clickCount:1});
      let logs;const end=Date.now()+5000;do{const r=await call('Runtime.evaluate',{expression:'platformProbe.logs',returnByValue:true});logs=r.result.value;if(logs.length<2)await new Promise(done=>setTimeout(done,20));}while(logs.length<2&&Date.now()<end);assert.deepEqual(logs,['varta log: granted granted','varta log: denied']);
    }
    const closed=await call('Runtime.evaluate',{expression:`(async()=>{platformApp.close();const result=await platformApp.finished;return{added:platformProbe.added,removed:platformProbe.removed,audit:result.permissions.audit};})()`,awaitPromise:true,returnByValue:true});
    assert(!closed.exceptionDetails,JSON.stringify(closed.exceptionDetails));assert.equal(closed.result.value.removed,closed.result.value.added);
    if(state==='granted')assert.equal(closed.result.value.audit.at(-1).outcome,'denied');
  });
  for(const failure of ['platform-denied','audit-observer'])await withBrowserPage(`${base}/__embed`,async(call)=>{
    const setup=await call('Runtime.evaluate',{expression:`(async()=>{globalThis.copyProbe={writes:0,logs:[]};Object.defineProperty(navigator,'permissions',{configurable:true,value:{query:async()=>({state:${JSON.stringify(failure==='platform-denied'?'denied':'granted')}})}});Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async()=>{copyProbe.writes++;}}});const{mount}=await import(${JSON.stringify(base+'/view-permissions/varta-host.js')});const container=document.createElement('div');document.body.append(container);globalThis.copyApp=mount({container,onLog:line=>copyProbe.logs.push(line),onAudit:entry=>{if(${JSON.stringify(failure)}==='audit-observer'&&entry.sequence>2)throw new Error('audit observer failed');}});copyApp.finished.then(value=>copyProbe.result=value,error=>copyProbe.result={status:'failed',message:error.message});await copyApp.ready;return true;})()`,awaitPromise:true,returnByValue:true});
    assert(!setup.exceptionDetails,JSON.stringify(setup.exceptionDetails));
    const{root}=await call('DOM.getDocument',{depth:-1,pierce:true});const find=node=>node.attributes?.some((value,index)=>index%2===0&&value==='id'&&node.attributes[index+1]==='copy')?node:[...(node.children??[]),...(node.shadowRoots??[])].map(find).find(Boolean);const button=find(root),{model}=await call('DOM.getBoxModel',{nodeId:button.nodeId}),q=model.content,x=(q[0]+q[2])/2,y=(q[1]+q[5])/2;
    await call('Input.dispatchMouseEvent',{type:'mousePressed',x,y,button:'left',clickCount:1});await call('Input.dispatchMouseEvent',{type:'mouseReleased',x,y,button:'left',clickCount:1});
    let value;const end=Date.now()+5000;do{const r=await call('Runtime.evaluate',{expression:'copyProbe',returnByValue:true});value=r.result.value;if(failure==='platform-denied'?value.logs.includes('varta log: copy 1'):value.result)break;await new Promise(done=>setTimeout(done,20));}while(Date.now()<end);
    assert.equal(value.writes,0,JSON.stringify(value));if(failure==='platform-denied')assert(value.logs.includes('varta log: copy 1'),JSON.stringify(value));else{assert.equal(value.result?.status,'failed');assert.match(value.result.message,/audit observer failed/);assert.deepEqual(value.logs,[]);}
  });
  console.log('Browser permissions match native: query/request descriptors and answers, optional/stored/required/identity/ceiling rules, exact accounting, audit ordering/ownership, invalid-input prefetch rejection, browser state narrowing/live updates and listener cleanup');
}finally{await new Promise(done=>server.close(done));}
