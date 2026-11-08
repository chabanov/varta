// Worker handlers of the pilot effects (clock, entropy, environment snapshot,
// exit) and idle collection between turns, against the native contract.
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
import * as sessionChannel from './session-channel.mjs';
const code=(await Promise.all(['scheduler.js','timers.js','network-policy.js','network.js','worker.js'].map(name=>readFile(new URL(name,import.meta.url),'utf8')))).join('\n');
const deferred=()=>{let resolve;const promise=new Promise(done=>resolve=done);return{promise,resolve};};
const {clock,random,exit,collector:C}=registry.effects,E=registry.environment;
const TURN_FUEL=8000000,MAINTENANCE_FUEL=4000000,MAINTENANCE_DEADLINE_MS=1000;

// A Worker with a fake compiled module: `start`/`resume` drive the imports,
// `idle` is the module's idle export (absent when undefined).
function fixture({mode='run',importNames=[],grants=[],permissions,environmentInputs,start,resume,idle,now=()=>1700000000123,words=[],budgets={}}){
  const messages=[],checkpoint=deferred(),memory=new WebAssembly.Memory({initial:2,maximum:4}),tanks=[],reads={clock:0,entropy:0};
  const descriptor={profile:'checked-f64-utf16-v38',schemaVersion:1,fairFuel:true,turnBudgets:true,
    budgets:{fuelPerTurn:TURN_FUEL,hostCallsPerTurn:1024,outputBytes:1048576,deadlinePerTurnMs:1000,[C.fuelBudget]:MAINTENANCE_FUEL,[C.deadlineBudget]:MAINTENANCE_DEADLINE_MS,...budgets},
    imports:importNames.map(name=>({module:'varta',name})),hostModule:'varta',memoryExport:'memory',pendingExport:'pending',startExport:'_start',
    original:{identity:'sha256:'+'a'.repeat(64)},maxStringBytes:65536,decodeChunkBytes:65536,resources:[]};
  const self={postMessage(message){messages.push(message);if(message.type==='turn_end'){checkpoint.resolve(message);self.onmessage({data:{type:'turn_ack',id:message.id}});}}};
  class Tank{constructor(fuel){this.remaining=fuel;this.global={value:BigInt(-fuel)};this.begun=[];tanks.push(this);}charge(rate,units=1){this.remaining-=registry.fuel[rate]*units;}beginTurn(fuel){this.remaining=fuel;this.begun.push(fuel);}}
  const wasm={Memory:WebAssembly.Memory,compile:async()=>({}),Module:{imports:()=>[...descriptor.imports.map(item=>({...item,kind:'function'})),{module:registry.meter.module,name:registry.meter.counter,kind:'global'}],customSections:()=>{const bytes=new Uint8Array(4);new DataView(bytes.buffer).setUint32(0,registry.meter.version,true);return[bytes.buffer];}},instantiate:async(_,imports)=>{
    const version=registry.profiles.find(profile=>profile.name===descriptor.profile).exceptionVersion;
    const exports={memory,[E.resultLength]:{value:7},[E.resultStatus]:{value:7},[version.global]:{value:version.value},
      [registry.scheduler.resumeExport](token){return resume?.(imports.varta,token)??1;},_start(){start?.(imports.varta,memory,exports);}};
    if(idle)exports[C.idleExport]=units=>idle(units,tanks[0]);
    return{exports};
  }};
  const context=vm.createContext({registry,permissionBroker,eventSources,viewPolicy,viewWorker,environment,environmentAbi,sessionChannel,self,WebAssembly:wasm,FuelTank:Tank,bundleSchemaVersion:1,
    reasonReader:()=>()=>'',browserTransport:()=>{throw new Error('unexpected transport');},TextEncoder,TextDecoder,URL,Uint8Array,Uint32Array,DataView,performance,setTimeout,clearTimeout,
    Date:{now(){reads.clock++;return now();}},
    crypto:{getRandomValues(array){reads.entropy++;const next=words.shift();if(!next)throw new Error('test entropy exhausted');array.set(next);return array;}}});
  vm.runInContext(code,context);
  const running=self.onmessage({data:{descriptor,image:new Uint8Array(),manifest:new TextEncoder().encode(JSON.stringify({capabilities:grants})),mode,environmentInputs,permissions}});
  return{messages,running,checkpoint,tanks,reads,memory,self,result:()=>own(messages.at(-1)),logs:()=>own(messages.filter(m=>m.type==='log').map(m=>m.line)),audit:()=>own(messages.filter(m=>m.type==='permission_audit').map(m=>m.record))};
}
const put=(memory,text)=>{const view=new DataView(memory.buffer);for(let i=0;i<text.length;i++)view.setUint16(i*2,text.charCodeAt(i),true);return text.length*2;};
const own=value=>JSON.parse(JSON.stringify(value));
const read=(memory,ptr,bytes)=>new TextDecoder('utf-16le').decode(new Uint8Array(memory.buffer,ptr,bytes));

test('Date.now needs the exact time.now grant and reads the clock only when granted',{timeout:2000},async()=>{
  let value;const granted=fixture({importNames:[clock.operation],grants:[{id:clock.capability,enabled:true}],start(op){value=op.date_now();}});
  await granted.running;
  assert.equal(value,1700000000123);assert.equal(granted.reads.clock,1);assert.equal(granted.result().status,'ok');assert.equal(granted.result().hostCalls,1);
  assert.deepEqual(granted.audit().map(r=>[r.id,r.outcome]),[[clock.capability,'granted']]);
  // A missing grant, a disabled grant, an empty host ceiling and a ceiling
  // that does not enable it each deny before the clock is read.
  for(const options of [{grants:[]},{grants:[{id:clock.capability,enabled:false}]},{grants:[{id:clock.capability,enabled:true}],permissions:{}},{grants:[{id:clock.capability,enabled:true}],permissions:{[clock.capability]:false}},{grants:[{id:'time.timer',enabled:true}],permissions:{'time.timer':true}}]){
    let denied;const f=fixture({importNames:[clock.operation],...options,start(op){denied=op.date_now();}});await f.running;
    assert(Number.isNaN(denied),JSON.stringify(options));assert.equal(f.reads.clock,0,JSON.stringify(options));
    assert.deepEqual(f.logs(),['varta log: '+clock.denialText]);assert.equal(f.result().status,'ok');assert.equal(f.result().hostCalls,1);
    assert.deepEqual(f.audit().filter(r=>r.id===clock.capability).map(r=>r.outcome),['denied']);
  }
});

test('Math.random converts one 64-bit entropy word exactly as native and needs no grant',{timeout:2000},async()=>{
  const words=[[0,0],[0,2047],[0,2048],[0x80000000,0],[0xffffffff,0xffffffff],[0x12345678,0x9abcdef0]];
  const samples=[];const f=fixture({importNames:[random.operation],words:words.map(w=>Uint32Array.from(w)),start(op){for(let i=0;i<words.length;i++)samples.push(op.number_random());}});
  await f.running;assert.equal(f.result().status,'ok',f.result().error);assert.equal(f.reads.entropy,words.length);
  assert(Object.is(samples[0],0));assert(Object.is(samples[1],0));assert.equal(samples[2],2**-53);assert.equal(samples[3],0.5);assert.equal(samples[4],1-2**-53);
  // (word >> 11) * 2^-53 for the word 0x123456789abcdef0.
  assert.equal(samples[5],Number(0x123456789abcdef0n>>11n)*2**-53);
  for(const sample of samples)assert(sample>=0&&sample<1);
  assert.equal(f.result().hostCalls,words.length);assert.deepEqual(f.audit(),[]);
  const failing=fixture({importNames:[random.operation],start(op){op.number_random();}});await failing.running;
  assert.equal(failing.result().status,'failed');assert.match(failing.result().error,/test entropy exhausted/);
});

test('env_snapshot lists exact names of request and ceiling in native order and JSON, and filters lookups',{timeout:2000},async()=>{
  const observed={};
  const snapshot=(op,memory,exports,name)=>{
    const ptr=op.env_snapshot(0,name===undefined?1:put(memory,name));
    return{ptr,status:exports[E.resultStatus].value,text:ptr?read(memory,ptr,exports[E.resultLength].value):''};
  };
  const f=fixture({importNames:[registry.effects.environment.operation],
    grants:[{id:'env.get',patterns:['APP_ONE','9','10','SECRET','WILD_*','UNSET','QUOTE']}],permissions:{'env.get':{patterns:['APP_ONE','9','10','WILD_*','UNSET','QUOTE']}},
    environmentInputs:[['APP_ONE','Привіт 😀'],['9','nine'],['10','ten'],['SECRET','private'],['WILD_X','wild'],['QUOTE','a"b\\c\n\u0001']],
    start(op,memory,exports){for(const name of ['','WILD_X','UNSET','SECRET','WILD_Y','A=B',undefined])observed[name??'invalid']=snapshot(op,memory,exports,name);}});
  await f.running;assert.equal(f.result().status,'ok',f.result().error);
  const S=E.statuses;
  // Byte (Unicode scalar) order like the native sorted map, never JavaScript's
  // integer-key order; the wildcard and the name outside the ceiling are not listed.
  assert.deepEqual(observed[''],{ptr:registry.network.result.base,status:S.ok,text:'{"10":"ten","9":"nine","APP_ONE":"Привіт 😀","QUOTE":"a\\"b\\\\c\\n\\u0001"}'});
  assert.deepEqual(observed.WILD_X,{ptr:registry.network.result.base,status:S.ok,text:'{"WILD_X":"wild"}'});
  assert.deepEqual(observed.UNSET,{ptr:registry.network.result.base,status:S.ok,text:'{}'});
  assert.deepEqual(observed.WILD_Y.text,'{}');
  assert.deepEqual(observed.SECRET,{ptr:0,status:S.denied,text:''});
  assert.deepEqual(observed['A=B'],{ptr:0,status:S.denied,text:''});
  assert.deepEqual(observed.invalid,{ptr:0,status:S.invalid,text:''});
  assert.deepEqual(f.logs(),['varta log: <capability denied: env> (SECRET)','varta log: <capability denied: env> (A=B)','varta log: '+registry.effects.failures.invalid]);
  assert(!JSON.stringify(f.messages).includes('private'));
  // Like native, the snapshot is not an audited broker operation.
  assert.deepEqual(f.audit(),[]);
  assert.equal(f.result().hostCalls,7);
});

test('env_snapshot without a grant or under an empty ceiling lists nothing and denies every name',{timeout:2000},async()=>{
  for(const options of [{grants:[]},{grants:[{id:'env.get',patterns:['A']}],permissions:{}},{grants:[{id:'env.get',patterns:['A']}],permissions:{'env.get':{patterns:[]}}}]){
    const observed=[];const f=fixture({importNames:[registry.effects.environment.operation],environmentInputs:[['A','value']],...options,
      start(op,memory,exports){const ptr=op.env_snapshot(0,0);observed.push(read(memory,ptr,exports[E.resultLength].value));observed.push(op.env_snapshot(0,put(memory,'A')),exports[E.resultStatus].value);}});
    await f.running;assert.equal(f.result().status,'ok',f.result().error);
    assert.deepEqual(observed,['{}',0,E.statuses.denied],JSON.stringify(options));
    assert(!JSON.stringify(f.messages).includes('value'));
  }
});

test('env_snapshot reports a result above the native limit as a recoverable too-large status',{timeout:2000},async()=>{
  // Two values that fit the input limits but whose UTF-16 result does not.
  let observed;
  const f=fixture({importNames:[registry.effects.environment.operation],grants:[{id:'env.get',patterns:['A','B']}],environmentInputs:[['A','x'.repeat(E.maxResultBytes*3/8)],['B','y'.repeat(E.maxResultBytes/4)]],
    start(op,memory,exports){observed=[op.env_snapshot(0,0),exports[E.resultStatus].value,exports[E.resultLength].value];}});
  await f.running;assert.equal(f.result().status,'ok',f.result().error);
  assert.deepEqual(observed,[0,E.statuses.tooLarge,0]);assert.deepEqual(f.logs(),['varta log: '+registry.effects.failures.tooLarge]);
});

test('an authorized exit ends the run with its code and runs no later guest code',{timeout:2000},async()=>{
  let after=false,globals;
  const f=fixture({importNames:['log_i32',exit.operation],grants:[{id:exit.capability,enabled:true}],
    start(op,memory,exports){op.log_i32(1);try{op.process_exit_f64(3);}finally{globals=[exports[E.resultLength].value,exports[E.resultStatus].value];}after=true;}});
  await f.running;
  assert.equal(after,false);assert.deepEqual(globals,[0,E.statuses.ok]);
  assert.equal(f.result().status,'ok');assert.equal(f.result().exitCode,3);assert.equal(f.result().error,undefined);
  assert.deepEqual(f.result().logs,['varta log: 1']);assert.equal(f.result().hostCalls,2);
  // An unremarkable finish reports exit code 0; -0 is the code 0.
  const zero=fixture({importNames:[exit.operation],grants:[{id:exit.capability,enabled:true}],start(op){op.process_exit_f64(-0);}});await zero.running;
  assert(Object.is(zero.result().exitCode,0));
  const plain=fixture({importNames:['log_i32'],start(op){op.log_i32(2);}});await plain.running;assert.equal(plain.result().exitCode,0);
});

test('a denied or invalid exit is a recoverable host failure and execution continues',{timeout:2000},async()=>{
  for(const [options,value,status,text] of [
    [{grants:[]},3,E.statuses.denied,exit.denialText],
    [{grants:[{id:exit.capability,enabled:true}],permissions:{}},3,E.statuses.denied,exit.denialText],
    [{grants:[{id:exit.capability,enabled:true}],permissions:{[exit.capability]:false}},3,E.statuses.denied,exit.denialText],
    [{grants:[{id:exit.capability,enabled:true}]},1.5,E.statuses.invalid,registry.effects.failures.invalid],
    [{grants:[{id:exit.capability,enabled:true}]},NaN,E.statuses.invalid,registry.effects.failures.invalid],
    [{grants:[{id:exit.capability,enabled:true}]},2**31,E.statuses.invalid,registry.effects.failures.invalid],
  ]){
    let observed;const f=fixture({importNames:[exit.operation],...options,start(op,memory,exports){op.process_exit_f64(value);observed=[exports[E.resultLength].value,exports[E.resultStatus].value];}});
    await f.running;
    assert.equal(f.result().status,'ok',JSON.stringify(options));assert.equal(f.result().exitCode,0);
    assert.deepEqual(observed,[0,status]);assert.deepEqual(f.logs(),['varta log: '+text]);
  }
  let reached=false;const allowed=fixture({importNames:[exit.operation],grants:[{id:exit.capability,enabled:true}],permissions:{[exit.capability]:true},start(op){op.process_exit_f64(-7);reached=true;}});
  await allowed.running;assert.equal(reached,false);assert.equal(allowed.result().exitCode,-7);
});

// Event-driven application instance: main opens source "ui" and watches it;
// each event turn takes the record, and the last closes the source.
const EVENTS=['event_source_open','event_source_close','event_take','event_watch','promise_react'];
function session({events=2,idle,budgets,extraImports=[],grants=[],onEvent}={}){
  let id;
  const f=fixture({mode:'application',importNames:[...EVENTS,...extraImports],grants,idle,budgets,
    start(op,memory){new Uint16Array(memory.buffer,0,2).set([117,105]);id=op.event_source_open(0,4);op.event_watch(op.promise_react(0,1n),id);},
    resume(op,token){op.event_take(id);onEvent?.(op,token);if(token<BigInt(events))op.event_watch(op.promise_react(0,token+1n),id);else{op.event_source_close(id);op.event_take(id);}return 1;}});
  const turnEnds=()=>f.messages.filter(m=>m.type==='turn_end').length;
  const deliver=async()=>{
    for(let i=1;i<=events;i++){
      while(turnEnds()<i&&!f.messages.some(m=>m.type==='result'))await new Promise(done=>setTimeout(done,0));
      if(f.messages.some(m=>m.type==='result'))break;
      await new Promise(done=>setTimeout(done,0));
      await f.self.onmessage({data:{type:'host_events',id:i,events:[{sourceId:id,event:{source:'ui',record:'e'+i},arrived:performance.timeOrigin+performance.now()}]}});
    }
    await f.running;return f.result();
  };
  return{f,deliver};
}

test('idle collection runs once per idle period under the maintenance budget and restores turn fuel',{timeout:4000},async()=>{
  const calls=[];
  const idle=(units,tank)=>{calls.push({units,remaining:tank.remaining});tank.remaining-=300000;return calls.length%3===0?C.done:C.more;};
  const {f,deliver}=session({events:2,idle});const result=await deliver();
  assert.equal(result.status,'ok',result.error);assert.equal(result.turns,3);
  // After turns 1 and 2 (the instance is alive and idle); none after the
  // last turn, which closes the only source.
  assert.deepEqual(result.maintenance.map(({afterTurn,fuel,steps,completed})=>({afterTurn,fuel,steps,completed})),
    [{afterTurn:1,fuel:900000,steps:3,completed:true},{afterTurn:2,fuel:900000,steps:3,completed:true}]);
  assert.equal(result.maintenanceFuel,1800000);assert.equal(result.maintenancePeriods,2);
  assert(calls.every(call=>call.units===C.stepUnits));
  assert.deepEqual(calls.map(call=>call.remaining),[MAINTENANCE_FUEL,MAINTENANCE_FUEL-300000,MAINTENANCE_FUEL-600000,MAINTENANCE_FUEL,MAINTENANCE_FUEL-300000,MAINTENANCE_FUEL-600000]);
  assert.deepEqual(f.messages.filter(m=>m.type.startsWith('maintenance_')).map(m=>[m.type,m.turn]),[['maintenance_begin',1],['maintenance_end',1],['maintenance_begin',2],['maintenance_end',2]]);
  // Identical turn usage with and without the idle export: maintenance never
  // spends or renews a turn's fuel, host calls or microtasks.
  const reference=session({events:2}),plain=await reference.deliver();
  for(const key of ['fuelUsed','fuelRemaining','hostCalls','microtasks','turns','outputBytes'])assert.equal(result[key],plain[key],key);
  assert.deepEqual(plain.maintenance,[]);
  const usages=({messages})=>own(messages.filter(m=>m.type==='turn_end').map(m=>m.usage));
  assert.deepEqual(usages(f),usages(reference.f));assert.equal(usages(f).length,3);
});

test('idle steps stop below one step of fuel, at no work, and at a completed cycle',{timeout:4000},async()=>{
  const heavy=await session({events:1,idle:(units,tank)=>{tank.remaining-=MAINTENANCE_FUEL-C.stepFuel+1;return C.more;}}).deliver();
  assert.deepEqual(heavy.maintenance.map(p=>[p.steps,p.completed,p.fuel]),[[1,false,MAINTENANCE_FUEL-C.stepFuel+1]]);
  // Like native, a period without a step is not listed, but its fuel counts.
  const none=await session({events:1,idle:(units,tank)=>{tank.remaining-=1000;return C.none;}}).deliver();
  assert.deepEqual([none.maintenance,none.maintenancePeriods,none.maintenanceFuel],[[],0,1000]);
  // A maintenance budget below one step's fuel runs no step.
  let called=false;const small=await session({events:1,budgets:{[C.fuelBudget]:C.stepFuel-1},idle:()=>{called=true;return C.done;}}).deliver();
  assert.equal(called,false);assert.deepEqual([small.maintenance,small.maintenancePeriods,small.maintenanceFuel],[[],0,0]);
  // A zero maintenance budget or a bounded run runs no maintenance at all.
  const zero=await session({events:1,budgets:{[C.fuelBudget]:0},idle:()=>{throw new Error('called');}}).deliver();
  assert.equal(zero.status,'ok');assert.deepEqual(zero.maintenance,[]);
  const bounded=fixture({idle:()=>{throw new Error('called');}});await bounded.running;
  assert.equal(bounded.result().status,'ok');assert.deepEqual(bounded.result().maintenance,[]);
});

test('an exhausted maintenance budget terminates the instance with the native diagnostic',{timeout:4000},async()=>{
  let later=false;
  const exhausted=session({events:2,onEvent:()=>{later=true;},idle:(units,tank)=>{tank.remaining=0;throw new Error('unreachable');}}),fuel=await exhausted.deliver();
  assert.equal(fuel.status,'failed');assert.equal(later,false);
  assert.equal(fuel.error,C.fuelExhausted.replace(C.turnPlaceholder,'1').replace(C.limitPlaceholder,String(MAINTENANCE_FUEL)));
  assert.deepEqual(fuel.maintenance.map(p=>[p.afterTurn,p.steps,p.completed,p.fuel]),[[1,1,false,MAINTENANCE_FUEL]]);
  assert.equal(fuel.maintenanceFuel,MAINTENANCE_FUEL);
  // The closed turn's usage stays the turn's: the report equals its checkpoint.
  const [checkpoint]=exhausted.f.messages.filter(m=>m.type==='turn_end');
  assert.equal(fuel.turns,1);assert.equal(fuel.fuelUsed,checkpoint.usage.fuelUsed);assert.equal(fuel.fuelRemaining,checkpoint.usage.fuelRemaining);
  const deadline=await session({events:1,budgets:{[C.deadlineBudget]:1},idle:()=>{const until=performance.now()+5;while(performance.now()<until);return C.more;}}).deliver();
  assert.equal(deadline.status,'failed');
  assert.equal(deadline.error,C.deadlineExhausted.replace(C.turnPlaceholder,'1').replace(C.limitPlaceholder,'1'));
  assert.deepEqual(deadline.maintenance.map(p=>[p.steps,p.completed]),[[1,false]]);
  const invalid=await session({events:1,idle:()=>7}).deliver();
  assert.equal(invalid.status,'failed');assert.equal(invalid.error,C.invalidStatus);assert.deepEqual(invalid.maintenance,[]);
  // A trap that is not fuel exhaustion keeps its own message.
  const trap=await session({events:1,idle:()=>{throw new Error('guest trap');}}).deliver();
  assert.equal(trap.error,'guest trap');
});

test('an exit in a later turn of an application ends it before any later turn',{timeout:4000},async()=>{
  let third=false;
  const result=await session({events:3,extraImports:[exit.operation],grants:[{id:exit.capability,enabled:true}],
    onEvent:(op,token)=>{if(token===2n)op.process_exit_f64(4);if(token===3n)third=true;}}).deliver();
  assert.equal(result.status,'ok');assert.equal(result.exitCode,4);assert.equal(result.turns,3);assert.equal(third,false);
});

test('log_stderr16 writes standard-error lines by level after the shared budget checks',{timeout:2000},async()=>{
  const C=registry.console;
  const f=fixture({importNames:['log_string16','log_stderr16'],start(op,memory){
    const len=put(memory,'text');
    op.log_string16(0,len);
    for(const level of C.stderr)op.log_stderr16(level.word,0,len);
  }});
  await f.running;assert.equal(f.result().status,'ok',f.result().error);
  assert.deepEqual(f.logs(),[C.logPrefix+'text',...C.stderr.map(level=>level.prefix+'text')]);
  assert.deepEqual(C.stderr.map(level=>level.method),['error','warn']);
  // An unknown level word is a noncanonical call: nothing is written.
  const bad=fixture({importNames:['log_stderr16'],start(op){op.log_stderr16(7,0,0);}});
  await bad.running;assert.equal(bad.result().status,'failed');assert.match(bad.result().error,/standard-error level/);
  assert.deepEqual(bad.logs(),[]);
});
