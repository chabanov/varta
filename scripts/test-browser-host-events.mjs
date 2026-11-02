// Real Wasm/native parity for generic application instances and named sources.
import {createServer} from 'node:http';
import {readFile} from 'node:fs/promises';
import {resolve,join,sep} from 'node:path';
import assert from 'node:assert/strict';
import {withBrowserPage} from './browser-probe.mjs';
const root=resolve(process.argv[2]);
const server=createServer(async(request,response)=>{try{
  const route=new URL(request.url,'http://loopback').pathname;
  if(route==='/__embed'){response.setHeader('Content-Type','text/html');response.end(`<!doctype html><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; worker-src blob:; trusted-types varta-worker 'allow-duplicates'; require-trusted-types-for 'script'"><title>Host events</title>`);return;}
  const path=resolve(root,`.${route}`);if(!path.startsWith(root+sep))throw new Error('outside fixture');response.setHeader('Content-Type',path.endsWith('.js')?'text/javascript':'application/octet-stream');response.end(await readFile(path));
}catch{response.writeHead(404).end();}});
await new Promise(done=>server.listen(0,'127.0.0.1',done));const base=`http://127.0.0.1:${server.address().port}`;
const evaluate=async(call,expression)=>{const result=await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});assert(!result.exceptionDetails,JSON.stringify(result.exceptionDetails));return result.result.value;};
const wait=async(call,expression)=>{for(let i=0;i<250;i++){if(await evaluate(call,expression))return;await new Promise(done=>setTimeout(done,20));}throw new Error(`not settled: ${expression}: ${JSON.stringify(await evaluate(call,'probe'))}`);};
const setup=(call,name)=>evaluate(call,`(async()=>{const{mount}=await import(${JSON.stringify(base+'/'+name+'/varta-host.js')});globalThis.probe={logs:[],turns:[],prompts:0};globalThis.app=mount({${name==='events-permission'?"onPrompt:()=>{probe.prompts++;return 'allow';},":''}onLog:line=>probe.logs.push(line),onTurn:usage=>probe.turns.push(usage)});app.finished.then(result=>probe.result=result,error=>probe.result={status:'failed',error:error.message});await app.ready;return true;})()`);
try{
  for(const name of ['events-live','events-reopen','events-throw','events-permission'])await withBrowserPage(`${base}/__embed`,async(call)=>{
    await setup(call,name);const golden=JSON.parse(await readFile(join(root,`${name}.events.golden.json`),'utf8'));
    const records=name==='events-live'?[['a',false],['old',true],['new',true],['stop',false]]:name==='events-reopen'?[['reopen',false],['stale',false]]:name==='events-permission'?[['gesture',false]]:[['boom',false]];
    const deliveries=await evaluate(call,`Promise.all(${JSON.stringify(records)}.map(([record,coalesce])=>app.deliver({source:'ui',record,coalesce,gesture:${name==='events-permission'}})))`);assert.deepEqual(deliveries,golden.deliveries.map(value=>value.toLowerCase()));
    if(name==='events-reopen'){await wait(call,"probe.logs.length===1&&probe.turns.at(-1).turns===2");assert.equal(await evaluate(call,"app.deliver({source:'ui',record:'stop'})"),'queued');}
    await wait(call,'probe.result!==undefined');const value=await evaluate(call,'probe.result');assert.equal(value.status,golden.failed?'failed':'ok',name);assert.deepEqual(value.logs,golden.logs,name);
    const usage=Object.fromEntries(Object.keys(golden.usage).map(key=>[key,value[key]]));assert.deepEqual(usage,golden.usage,name);
    if(name==='events-permission')assert.equal(await evaluate(call,'probe.prompts'),1,'a trusted host gesture can request consent');
    assert.equal(await evaluate(call,"app.deliver({source:'ui',record:'late'}).then(()=>false,()=>true)"),true,'terminated sessions reject delivery');
  });
  await withBrowserPage(`${base}/__embed`,async(call)=>{
    await setup(call,'events-permission');await evaluate(call,"app.deliver({source:'ui',record:'background'})");await wait(call,'probe.result!==undefined');assert.equal(await evaluate(call,'probe.prompts'),0);assert.deepEqual(await evaluate(call,'probe.logs'),['varta log: prompt']);
  });
  for(const coalesce of [false,true])await withBrowserPage(`${base}/__embed`,async(call)=>{
    await setup(call,'events-live');const results=await evaluate(call,`Promise.allSettled(Array.from({length:257},(_,index)=>app.deliver({source:'ui',record:String(index),coalesce:${coalesce}})))`);
    if(!coalesce){assert(results.every(result=>result.status==='rejected'));await wait(call,'probe.result!==undefined');assert.match(await evaluate(call,'probe.result.error'),/queue is full/);assert.deepEqual(await evaluate(call,'probe.logs'),['varta log: init','varta log: TypeError']);}
    else{assert(results.every(result=>result.status==='fulfilled'));assert.equal(results.at(-1).value,'dropped');assert.equal(results[0].value,'queued');assert(results.slice(1,-1).every(result=>result.value==='coalesced'));await wait(call,'probe.logs.length===3');assert.equal((await evaluate(call,'probe.logs')).at(-1),'varta log: 255');await evaluate(call,"app.deliver({source:'ui',record:'stop'})");await wait(call,'probe.result!==undefined');assert.equal(await evaluate(call,'probe.result.status'),'ok');}
  });
  await withBrowserPage(`${base}/__embed`,async(call)=>{
    const value=await evaluate(call,`(async()=>{const{mount}=await import(${JSON.stringify(base+'/events-live/varta-host.js')});const logs=[];const app=mount({onLog:line=>logs.push(line)}),input={source:'ui',record:'early'};const pending=app.deliver(input);input.record='mutation';await pending;await app.deliver({source:'ui',record:'stop'});const result=await app.finished;return result.logs;})()`);
    assert.deepEqual(value,['varta log: init','varta log: TypeError','varta log: early','varta log: stop']);
  });
  await withBrowserPage(`${base}/__embed`,async(call)=>{
    await setup(call,'events-live');const errors=await evaluate(call,`Promise.all([{source:'missing',record:'x'},{source:'ui',record:'😀'.repeat(65536)},{source:'ui',record:'\uD800'}].map(event=>app.deliver(event).then(()=>false,error=>error.message)))`);
    assert.match(errors[0],/no open host/);assert.match(errors[1],/exceeds/);assert.match(errors[2],/state/);
    const result=await evaluate(call,`(async()=>{const event={source:'ui',record:'\0\uFEFF😀'};const delivered=app.deliver(event);event.record='mutation';await delivered;return true;})()`);assert.equal(result,true);await wait(call,'probe.logs.length===3');assert.equal((await evaluate(call,'probe.logs')).at(-1),'varta log: \0\uFEFF😀');
    await evaluate(call,"app.deliver({source:'ui',record:'stop'})");await wait(call,'probe.result!==undefined');assert.equal(await evaluate(call,'probe.result.status'),'ok');
  });
  console.log('Browser named host events match native: generic non-view mount, FIFO/coalescing, exact fuel/calls/jobs/turns, close/reopen generations, listener failure, recoverable invalid delivery and owned UTF-8 records');
}finally{await new Promise(done=>server.close(done));}
