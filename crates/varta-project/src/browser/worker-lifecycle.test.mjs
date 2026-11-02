import {test} from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import {readFile} from 'node:fs/promises';
import {registry} from '../../../varta-common/browser/registry.mjs';
import * as permissionBroker from './permission-broker.mjs';
import * as eventSources from './event-sources.mjs';
import * as viewPolicy from './view-policy.mjs';
import * as viewWorker from './view-worker.mjs';
import * as environment from './environment.mjs';
import * as environmentAbi from './environment-abi.mjs';
const code=(await Promise.all(['scheduler.js','timers.js','network-policy.js','network.js','worker.js'].map(name=>readFile(new URL(name,import.meta.url),'utf8')))).join('\n');
const deferred=()=>{let resolve;const promise=new Promise(done=>resolve=done);return{promise,resolve};};
function fixture({mode='run',holdInstantiate=false,autoAck=true,importNames=['permission_query'],grants=[{id:'clipboard.write',enabled:true}],start,resume,clock=performance,transport=()=>{throw new Error('unexpected transport');},environmentInputs,permissions,testEnvironment=false,memoryMaximum=2}={}){
  const compile=deferred(),instantiate=deferred(),instantiating=deferred(),checkpoint=deferred(),messages=[],answers=[];
  const memory=new WebAssembly.Memory({initial:1,maximum:memoryMaximum});
  const text=[registry.permissions.descriptor.version,'clipboard.write',registry.permissions.descriptor.none,''].join('\n');
  new Uint16Array(memory.buffer,0,text.length).set(Array.from(text,unit=>unit.charCodeAt(0)));
  const descriptor={profile:'checked-f64-utf16-v38',schemaVersion:1,fairFuel:true,turnBudgets:true,
    budgets:{fuelPerTurn:8000000,hostCallsPerTurn:1024,outputBytes:1048576,deadlinePerTurnMs:1000},
    imports:importNames.map(name=>({module:'varta',name})),hostModule:'varta',memoryExport:'memory',pendingExport:'pending',startExport:'_start',
    original:{identity:'sha256:'+'a'.repeat(64)},maxStringBytes:65536,decodeChunkBytes:65536,resources:[]};
  let context;
  const self={postMessage(message){messages.push(message);if(message.type==='turn_end'){checkpoint.resolve(message);if(autoAck)self.onmessage({data:{type:'turn_ack',id:message.id}});}}};
  class Tank{constructor(fuel){this.remaining=fuel;this.global={value:BigInt(fuel)};}charge(){}beginTurn(fuel){this.remaining=fuel;}}
  const wasm={Memory:WebAssembly.Memory,compile:()=>compile.promise,Module:{imports:()=>[...descriptor.imports.map(item=>({...item,kind:'function'})),{module:registry.meter.module,name:registry.meter.counter,kind:'global'}],customSections:()=>{const bytes=new Uint8Array(4);new DataView(bytes.buffer).setUint32(0,registry.meter.version,true);return[bytes.buffer];}},instantiate:async(_,imports)=>{
    instantiating.resolve();if(holdInstantiate)await instantiate.promise;
    const version=registry.profiles.find(profile=>profile.name===descriptor.profile).exceptionVersion;
    return{exports:{memory,[registry.environment.resultLength]:{value:0},[registry.environment.resultStatus]:{value:0},[version.global]:{value:version.value},[registry.scheduler.resumeExport](token){return resume?.(imports.varta,token)??1;},_start(){if(start)start(imports.varta,memory);else answers.push(imports.varta.permission_query(0,text.length*2));}}};
  }};
  const testRegistry=testEnvironment?{...registry,operations:registry.operations.map(op=>['env_get16','env_has16'].includes(op.name)?{...op,capability:null}:op)}:registry;
  context=vm.createContext({registry:testRegistry,permissionBroker,eventSources,viewPolicy,viewWorker,environment,environmentAbi,self,WebAssembly:wasm,FuelTank:Tank,bundleSchemaVersion:1,
    reasonReader:()=>()=>'',browserTransport:transport,TextEncoder,TextDecoder,URL,Uint8Array,DataView,performance:clock,setTimeout,clearTimeout});
  vm.runInContext(code,context);
  const running=self.onmessage({data:{descriptor,image:new Uint8Array(),manifest:new TextEncoder().encode(JSON.stringify({capabilities:grants})),platform:{'clipboard.write':'granted'},mode,environmentInputs,permissions}});
  return{self,messages,answers,running,compile,instantiate,instantiating,checkpoint};
}
test('environment Worker handler honors the host ceiling before accessing the owned snapshot',{timeout:2000},async()=>{
  let observed;const fixtureRun=fixture({testEnvironment:true,importNames:['env_get16'],grants:[{id:'env.get',patterns:['PUBLIC','SECRET']}],permissions:{'env.get':{patterns:['PUBLIC']}},environmentInputs:[['PUBLIC','Привіт 😀'],['SECRET','private']],start(op,memory){
    const put=name=>{const view=new DataView(memory.buffer);for(let i=0;i<name.length;i++)view.setUint16(i*2,name.charCodeAt(i),true);return name.length*2;};
    const ptr=op.env_get16(0,put('PUBLIC'));const value=new TextDecoder('utf-16le').decode(new Uint8Array(memory.buffer,ptr,'Привіт 😀'.length*2));
    const denied=op.env_get16(0,put('SECRET'));observed={value,denied};
  }});
  fixtureRun.compile.resolve({});await fixtureRun.running;
  assert.deepEqual(observed,{value:'Привіт 😀',denied:0});
  assert.equal(fixtureRun.messages.at(-1).status,'ok');
  const records=fixtureRun.messages.filter(message=>message.type==='permission_audit').map(message=>message.record.outcome);
  assert.deepEqual(records,['granted','denied']);
  assert(!JSON.stringify(fixtureRun.messages).includes('private'));
});
test('environment imports use the production Worker admission gate',{timeout:2000},async()=>{
  let entered=false;const f=fixture({importNames:['env_get16'],grants:[{id:'env.get',patterns:['PUBLIC']}],environmentInputs:[['PUBLIC','value']],start(){entered=true;}});
  f.compile.resolve({});await f.running;
  assert.equal(entered,true);assert.equal(f.messages.at(-1).status,'ok');
});
test('environment result growth failure returns IO and leaves the Worker alive',{timeout:2000},async()=>{
  let result;const f=fixture({memoryMaximum:1,importNames:['env_get16'],grants:[{id:'env.get',patterns:['PUBLIC']}],environmentInputs:[['PUBLIC','x']],start(op,memory){const name='PUBLIC',view=new DataView(memory.buffer);for(let i=0;i<name.length;i++)view.setUint16(i*2,name.charCodeAt(i),true);result=op.env_get16(0,name.length*2);}});
  f.compile.resolve({});await f.running;assert.equal(result,0);assert.equal(f.messages.at(-1).status,'ok');
  assert(f.messages.some(message=>message.type==='log'&&message.line==='varta log: <host I/O error: failed to grow memory by `1`>'));
});
test('platform changes during compilation coalesce before the first guest effect',{timeout:2000},async()=>{
  const f=fixture();for(const state of ['prompt','denied'])await f.self.onmessage({data:{type:'permission_platform',id:'clipboard.write',state}});
  f.compile.resolve({});await f.running;assert.deepEqual(f.answers,[registry.permissions.states.denied]);assert.equal(f.messages.at(-1).status,'ok');assert.equal(f.messages.at(-1).hostCalls,0);
});
test('a checkpoint acknowledgement is required before bounded execution can finish',{timeout:2000},async()=>{
  const f=fixture({autoAck:false});f.compile.resolve({});const checkpoint=await f.checkpoint.promise;
  assert.equal(f.messages.some(message=>message.type==='result'),false);
  await f.self.onmessage({data:{type:'turn_ack',id:checkpoint.id}});await f.running;assert.equal(f.messages.at(-1).status,'ok');
});
test('bad startup messages and acknowledgements fail before entering guest code',{timeout:2000},async()=>{
  const startup=fixture();await startup.self.onmessage({data:{type:'turn_ack',id:1}});startup.compile.resolve({});await startup.running;
  assert.deepEqual(startup.answers,[]);assert.equal(startup.messages.at(-1).status,'failed');assert.equal(startup.messages.some(message=>message.type==='entered'),false);
  const later=fixture({holdInstantiate:true});later.compile.resolve({});await later.instantiating.promise;
  await later.self.onmessage({data:{type:'turn_ack',id:1}});later.instantiate.resolve();await later.running;
  assert.deepEqual(later.answers,[]);assert.match(later.messages.at(-1).error,/turn acknowledgement/);
});

test('an overdue interval cannot prevent a watched network transfer from starting',{timeout:2000},async()=>{
  let now=0,starts=0,timer,serial,request,startedBeforeTimer;
  const N=registry.network,text=[N.version,'GET','https://api.test/x','0',N.noBody,''].join(N.separator);
  const f=fixture({
    importNames:['promise_react','timer_start_v2','timer_clear','timer_take','net_request_start_v2','net_request_watch','net_request_take','net_request_abort'],
    grants:[{id:'time.timer',enabled:true},{id:'net.request',patterns:['https://api.test/**']}],clock:{timeOrigin:0,now:()=>now},
    transport:()=>{starts++;return{head:new Promise(()=>{}),cancel(){}};},
    start(op,memory){new Uint16Array(memory.buffer,0,text.length).set(Array.from(text,unit=>unit.charCodeAt(0)));request=op.net_request_start_v2(0,text.length*2);const watched=op.promise_react(0,2n);op.net_request_watch(watched,request);serial=op.promise_react(0,1n);timer=op.timer_start_v2(serial,1,registry.scheduler.timer.interval);now=10;},
    resume(op,token){if(token===1n){startedBeforeTimer=starts;op.timer_take(serial);op.timer_clear(timer);op.timer_take(serial);op.net_request_abort(request);}else if(token===2n)op.net_request_take(request);return 1;},
  });f.compile.resolve({});await f.running;
  assert.equal(f.messages.at(-1).status,'ok',f.messages.at(-1).error);assert.equal(startedBeforeTimer,1);assert.equal(starts,1);
});

test('revocation preserves an authorized transfer while denying new ones in its completion turn',{timeout:2000},async()=>{
  const head=deferred(),started=deferred(),N=registry.network;
  const text=[N.version,'GET','https://api.test/x','0',N.noBody,''].join(N.separator);
  let request,starts=0,cancels=0,completed;
  const f=fixture({mode:'application',grants:[{id:'net.request',patterns:['https://api.test/**']}],
    importNames:['promise_react','net_request_start_v2','net_request_watch','net_request_take'],
    transport(){starts++;started.resolve();return{head:head.promise,cancel(){cancels++;}};},
    start(op,memory){new Uint16Array(memory.buffer,0,text.length).set(Array.from(text,unit=>unit.charCodeAt(0)));request=op.net_request_start_v2(0,text.length*2);op.net_request_watch(op.promise_react(0,1n),request);},
    resume(op){completed=op.net_request_take(request);const denied=op.net_request_start_v2(0,text.length*2);assert.equal(op.net_request_take(denied),N.completion.denied);throw new Error('revocation probe complete');},
  });
  f.compile.resolve({});await started.promise;
  await f.self.onmessage({data:{type:'permission_revoke',id:1,capability:'net.request'}});
  assert.equal(cancels,0);assert.equal(f.messages.at(-1).type,'permission_revoked');assert.equal(f.messages.at(-1).revoked,true);
  head.resolve({kind:N.completion.head,text:'head'});await f.running;
  assert.equal(completed,N.completion.head);assert.equal(starts,1);assert.match(f.messages.at(-1).error,/probe complete/);
  assert.equal(cancels,1,'terminal cleanup releases the still-open body');
});

test('revocation controls reject gaps, unknown capabilities and bounded executions',{timeout:2000},async()=>{
  for(const options of [{mode:'run',id:1,capability:'clipboard.write'},{mode:'application',id:2,capability:'clipboard.write'},{mode:'application',id:1,capability:'time.fake'}]){
    const f=fixture({mode:options.mode,autoAck:false});f.compile.resolve({});await f.checkpoint.promise;
    await f.self.onmessage({data:{type:'permission_revoke',id:options.id,capability:options.capability}});await f.running;
    assert.equal(f.messages.at(-1).status,'failed');assert.match(f.messages.at(-1).error,/invalid permission revocation/);
  }
  const replay=fixture({mode:'application',autoAck:false});replay.compile.resolve({});await replay.checkpoint.promise;
  const control={type:'permission_revoke',id:1,capability:'clipboard.write'};
  await replay.self.onmessage({data:control});await replay.self.onmessage({data:control});await replay.running;
  assert.equal(replay.messages.filter(message=>message.type==='permission_revoked').length,1);assert.match(replay.messages.at(-1).error,/invalid permission revocation/);
});
test('named source delivery pins a generation and cannot be retargeted by reopening its name',{timeout:2000},async()=>{
  let old,current,seen=0;
  const f=fixture({mode:'application',importNames:['event_source_open','event_source_close','event_take','event_watch','promise_react'],
    start(op,memory){new Uint16Array(memory.buffer,0,2).set([117,105]);old=op.event_source_open(0,4);op.event_source_close(old);op.event_take(old);current=op.event_source_open(0,4);op.event_watch(op.promise_react(0,1n),current);},
    resume(op){seen++;op.event_take(current);op.event_source_close(current);op.event_take(current);return 1;},
  });f.compile.resolve({});await f.checkpoint.promise;
  await f.self.onmessage({data:{type:'host_events',id:1,events:[{sourceId:old,event:{source:'ui',record:'stale'},arrived:performance.timeOrigin+performance.now()}]}});
  const rejected=f.messages.find(message=>message.type==='host_events_queued');assert.match(rejected.results[0].error,/no open host/);assert.equal(seen,0);
  await f.self.onmessage({data:{type:'host_events',id:2,events:[{sourceId:current,event:{source:'ui',record:'live'},arrived:performance.timeOrigin+performance.now()}]}});await f.running;
  assert.equal(seen,1);assert.equal(f.messages.at(-1).status,'ok');
});
test('malformed delivery batch controls fail before a queued task can run',{timeout:2000},async()=>{
  const f=fixture({mode:'application',autoAck:false});f.compile.resolve({});await f.checkpoint.promise;
  await f.self.onmessage({data:{type:'host_events',id:2,events:[]}});await f.running;
  assert.equal(f.messages.at(-1).status,'failed');assert.match(f.messages.at(-1).error,/invalid host event batch/);
});
test('a host pause holds new tasks after the checkpoint and resumes without adding a turn',{timeout:2000},async()=>{
  let id,seen=0;const paused=deferred();
  const f=fixture({mode:'application',importNames:['event_source_open','event_source_close','event_take','event_watch','promise_react'],
    start(op,memory){new Uint16Array(memory.buffer,0,2).set([117,105]);id=op.event_source_open(0,4);op.event_watch(op.promise_react(0,1n),id);},
    resume(op){seen++;op.event_take(id);op.event_source_close(id);op.event_take(id);return 1;},
  });f.compile.resolve({});await f.checkpoint.promise;
  const post=f.self.postMessage;f.self.postMessage=message=>{post(message);if(message.type==='host_paused')paused.resolve();};
  await f.self.onmessage({data:{type:'host_pause',id:1}});await paused.promise;
  await f.self.onmessage({data:{type:'host_events',id:1,events:[{sourceId:id,event:{source:'ui',record:'held'},arrived:performance.timeOrigin+performance.now()}]}});
  await Promise.resolve();assert.equal(seen,0);assert.equal(f.messages.filter(message=>message.type==='turn_begin').length,0);
  await f.self.onmessage({data:{type:'host_resume',id:1}});await f.running;assert.equal(seen,1);assert.equal(f.messages.at(-1).turns,2);assert.equal(f.messages.at(-1).status,'ok');
});
test('pause controls reject gaps, premature resumes and duplicate requests',{timeout:2000},async()=>{
  for(const messages of [[{type:'host_pause',id:2}],[{type:'host_resume',id:1}],[{type:'host_pause',id:1},{type:'host_resume',id:1}],[{type:'host_pause',id:1},{type:'host_pause',id:1}]]){
    const f=fixture({mode:'application',autoAck:false});f.compile.resolve({});await f.checkpoint.promise;
    for(const data of messages)await f.self.onmessage({data});await f.running;assert.equal(f.messages.at(-1).status,'failed');assert.match(f.messages.at(-1).error,/invalid host (pause|resume)/);
  }
});
