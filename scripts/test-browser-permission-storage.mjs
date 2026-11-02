// Real IndexedDB transactions plus live parent/Worker revocation. All OS effects
// are spies; this probe never writes the system clipboard or opens external URLs.
import {createServer} from 'node:http';
import {readFile} from 'node:fs/promises';
import {resolve,sep,join} from 'node:path';
import assert from 'node:assert/strict';
import {withBrowserPage} from './browser-probe.mjs';
const root=resolve(process.argv[2]);let artifactFetches=0;
const server=createServer(async(request,response)=>{
  try{
    const route=new URL(request.url,'http://loopback').pathname;
    if(route==='/__embed'){response.setHeader('Content-Type','text/html');response.end(`<!doctype html><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; worker-src blob:; trusted-types varta-worker 'allow-duplicates'; require-trusted-types-for 'script'"><title>Permission storage test</title>`);return;}
    const path=resolve(root,`.${route}`);if(!path.startsWith(root+sep))throw new Error('outside fixture');if(route.endsWith('/app.wasm'))artifactFetches++;
    response.setHeader('Content-Type',path.endsWith('.js')?'text/javascript':path.endsWith('.html')?'text/html':'application/octet-stream');response.setHeader('Cache-Control','no-store');response.end(await readFile(path));
  }catch{response.writeHead(404).end();}
});
await new Promise(done=>server.listen(0,'127.0.0.1',done));
const base=`http://127.0.0.1:${server.address().port}`,artifact=JSON.parse(await readFile(join(root,'permission-platform.golden.json'),'utf8')).permissions.artifact;
const evaluate=async(call,expression)=>{const result=await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});assert(!result.exceptionDetails,JSON.stringify(result.exceptionDetails));return result.result.value;};
const click=async(call,id)=>{
  const{root}=await call('DOM.getDocument',{depth:-1,pierce:true});
  const find=node=>node.attributes?.some((value,index)=>index%2===0&&value==='id'&&node.attributes[index+1]===id)?node:[...(node.children??[]),...(node.shadowRoots??[])].map(find).find(Boolean);
  const button=find(root);assert(button,`missing ${id}`);const{model}=await call('DOM.getBoxModel',{nodeId:button.nodeId}),q=model.content,x=(q[0]+q[2])/2,y=(q[1]+q[5])/2;
  await call('Input.dispatchMouseEvent',{type:'mousePressed',x,y,button:'left',clickCount:1});await call('Input.dispatchMouseEvent',{type:'mouseReleased',x,y,button:'left',clickCount:1});
};
try{
  await withBrowserPage(`${base}/__embed`,async(call)=>{
    const result=await evaluate(call,`(async()=>{
      const{createDecisionStore}=await import(${JSON.stringify(base+'/permission-platform/varta-host.js')});
      const name='transactions',a=createDecisionStore({name}),b=createDecisionStore({name}),hash='sha256:'+'a'.repeat(64),other='sha256:'+'b'.repeat(64);
      await Promise.all([a.set(hash,'time.timer','allow'),b.set(other,'clipboard.write','deny'),b.set(hash,'net.request','allow')]);
      const first=await a.read();if(first.applications.length!==2)throw new Error('lost artifact');
      const selected=first.applications.find(app=>app.artifact===hash);if(selected.decisions.length!==2)throw new Error('lost decision');
      await a.set(hash,'time.timer','deny');if(first.applications.find(app=>app.artifact===hash).decisions.find(record=>record.id==='time.timer').decision!=='allow')throw new Error('aliased snapshot');
      const controller=new AbortController();const cancelled=a.set(hash,'clipboard.write','allow',{signal:controller.signal});controller.abort();let aborted=false;try{await cancelled;}catch(error){aborted=error.name==='AbortError';}if(!aborted)throw new Error('abort ignored');
      if((await b.read()).applications.find(app=>app.artifact===hash).decisions.some(record=>record.id==='clipboard.write'))throw new Error('aborted write persisted');
      await a.clear(hash);if((await b.read()).applications[0].artifact!==other)throw new Error('wrong clear scope');
      const db=await new Promise((resolve,reject)=>{const r=indexedDB.open(name);r.onsuccess=()=>resolve(r.result);r.onerror=()=>reject(r.error);});
      await new Promise((resolve,reject)=>{const tx=db.transaction('choices','readwrite');tx.objectStore('choices').put({applications:[{artifact:hash,decisions:[{id:'time.fake',decision:'allow'}]}]},'decisions');tx.oncomplete=resolve;tx.onabort=()=>reject(tx.error);});db.close();
      let corrupt=false;try{await a.read();}catch(error){corrupt=/invalid permission decisions/.test(error.message);}if(!corrupt)throw new Error('corruption accepted');
      await a.clear();if((await a.read()).applications.length)throw new Error('corrupt store recovery failed');
      for(let i=0;i<64;i++)await a.set('sha256:'+i.toString(16).padStart(64,'0'),'time.timer','allow');
      let full=false;try{await b.set(hash,'time.timer','deny');}catch{full=true;}if(!full||(await a.read()).applications.length!==64)throw new Error('quota evicted choices');
      await a.clear();await new Promise((resolve,reject)=>{const r=indexedDB.deleteDatabase(name);r.onsuccess=resolve;r.onerror=()=>reject(r.error);r.onblocked=()=>reject(new Error('leaked database connection'));});return true;
    })()`);assert.equal(result,true);
    const before=artifactFetches;
    const prefetch=await evaluate(call,`(async()=>{const{run}=await import(${JSON.stringify(base+'/permission-optional/varta-host.js')});try{await run({decisionStore:{read:async()=>({applications:[{artifact:'sha256:'+'c'.repeat(64),decisions:[{id:'time.fake',decision:'allow'}]}]}),set:async()=>{}}});return false;}catch(error){return /invalid permission decisions/.test(error.message);}})()`);
    assert.equal(prefetch,true);assert.equal(artifactFetches,before);
    const mounted=await evaluate(call,`(async()=>{
      Object.defineProperty(navigator,'permissions',{configurable:true,value:{query:async()=>({state:'granted'})}});
      const{mount,createDecisionStore}=await import(${JSON.stringify(base+'/permission-platform/varta-host.js')});globalThis.store=createDecisionStore({name:'live'});await store.set(${JSON.stringify(artifact)},'clipboard.write','allow');
      globalThis.logs=[];globalThis.audit=[];const container=document.createElement('div');document.body.append(container);globalThis.app=mount({container,decisionStore:store,onLog:line=>logs.push(line),onAudit:row=>audit.push(row)});await app.ready;
      const revoked=await Promise.all([app.revoke('clipboard.write'),app.revoke('clipboard.write')]);const absent=await app.revoke('time.timer');return{logs,revoked,absent,audit};
    })()`);
    assert.deepEqual(mounted.logs,['varta log: granted granted']);assert.deepEqual(mounted.revoked,[true,true]);assert.equal(mounted.absent,false);assert.equal(mounted.audit.length,2);assert.equal(mounted.audit.at(-1).source,'stored');assert.equal(mounted.audit.at(-1).outcome,'denied');
    await click(call,'permission');
    let logs;for(let i=0;i<200;i++){logs=await evaluate(call,'logs');if(logs.length===2)break;await new Promise(done=>setTimeout(done,20));}
    assert.deepEqual(logs,['varta log: granted granted','varta log: denied']);
    const closed=await evaluate(call,`(async()=>{app.close();const result=await app.finished;let ended=false;try{await app.revoke('clipboard.write');}catch{ended=true;}return{status:result.status,decisions:result.permissions.decisions,ended};})()`);
    assert.deepEqual(closed,{status:'closed',decisions:{'clipboard.write':'deny'},ended:true});
    await call('Page.navigate',{url:`${base}/__embed`});for(let i=0;i<100;i++){if(await evaluate(call,`document.readyState==='complete'&&typeof app==='undefined'`))break;await new Promise(done=>setTimeout(done,20));}
    const restarted=await evaluate(call,`(async()=>{const{mount,createDecisionStore}=await import(${JSON.stringify(base+'/permission-platform/varta-host.js')});const store=createDecisionStore({name:'live'}),container=document.createElement('div');document.body.append(container);const app=mount({container,decisionStore:store});try{await app.ready;return false;}catch{}try{await app.finished;return false;}catch(error){return{message:error.message,entered:error.guestStarted,decisions:(await store.read()).applications};}})()`);
    assert.match(restarted.message,/stored decision/);assert.equal(restarted.entered,false);assert.equal(restarted.decisions[0].artifact,artifact);assert.equal(restarted.decisions[0].decisions[0].decision,'deny');
  });
  await withBrowserPage(`${base}/__embed`,async(call)=>{
    const prepared=await evaluate(call,`(async()=>{globalThis.writes=0;Object.defineProperty(navigator,'permissions',{configurable:true,value:{query:async()=>({state:'granted'})}});Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async()=>{writes++;}}});const{mount}=await import(${JSON.stringify(base+'/view-permissions/varta-host.js')});const container=document.createElement('div');document.body.append(container);globalThis.logs=[];globalThis.app=mount({container,onLog:line=>logs.push(line)});await app.ready;return await app.revoke('clipboard.write');})()`);assert.equal(prepared,true);
    await click(call,'copy');let outcome;for(let i=0;i<200;i++){outcome=await evaluate(call,'({writes,logs})');if(outcome.logs.length)break;await new Promise(done=>setTimeout(done,20));}
    assert.deepEqual(outcome,{writes:0,logs:['varta log: copy 1']});await evaluate(call,'app.close();true');
  });
  for(const failure of ['store','observer'])await withBrowserPage(`${base}/__embed`,async(call)=>{
    const result=await evaluate(call,`(async()=>{
      Object.defineProperty(navigator,'permissions',{configurable:true,value:{query:async()=>({state:'granted'})}});
      const{mount}=await import(${JSON.stringify(base+'/permission-platform/varta-host.js')});const container=document.createElement('div');document.body.append(container);
      const app=mount({container,${failure==='store'?"decisionStore:{read:async()=>undefined,set(){throw new Error('store failed');}},":"onAudit:row=>{if(row.sequence>1)throw new Error('observer failed');},"}});await app.ready;
      let controlError;try{await app.revoke('clipboard.write');}catch(error){controlError=error.message;}
      try{await app.finished;return false;}catch(error){return{controlError,error:error.message,decision:error.permissions.decisions['clipboard.write']};}
    })()`);
    assert.deepEqual(result,{controlError:`${failure} failed`,error:`${failure} failed`,decision:'deny'});
  });
  console.log('Browser permission storage/revocation: atomic concurrent choices, owned snapshots, abort, corruption/recovery, bounds, connection cleanup, prefetch validation, live Worker/UI denial and durable reload refusal passed');
}finally{await new Promise(done=>server.close(done));}
