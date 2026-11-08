import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';
import {registry} from '../../../varta-common/browser/registry.mjs';
import * as sessionChannel from './session-channel.mjs';
import {KvContainer,CONTAINER,FAILURES,crc32,compareBytes,versionstamp,openKvStorage,syncHandlesSupported,parseVirtualPath,virtualMatches,glob,createKvPolicy,createCodec,createKvHost} from './kv-store.mjs';

const K=registry.kv;
const PATH='/data/app.kv', OTHER='/data/other.kv';
const bytes=(...values)=>Uint8Array.from(values);
const u64=(value)=>{const b=new Uint8Array(8);new DataView(b.buffer).setBigUint64(0,value,true);return b;};
// A fixed version step keeps versionstamps predictable: 1 + 4 % 9 = 5.
const STEP_SOURCE=4, STEP=1+STEP_SOURCE%9;

/** A `FileSystemSyncAccessHandle` in memory. `durable` is what a flush made
 * persistent; `crashAt` throws at that numbered mutating call (a torn write
 * keeps the first half of its bytes); `quota` refuses growth past it. */
class FakeHandle {
  constructor(data=new Uint8Array(0)){this.data=data.slice();this.durable=data.slice();this.calls=0;this.crashAt=undefined;this.quota=undefined;this.closed=false;}
  getSize(){return this.data.length;}
  read(buffer,{at}){const n=Math.max(0,Math.min(buffer.length,this.data.length-at));buffer.set(this.data.subarray(at,at+n));return n;}
  #resize(size){const next=new Uint8Array(size);next.set(this.data.subarray(0,Math.min(size,this.data.length)));this.data=next;}
  write(buffer,{at}){
    const call=this.calls++;
    if(this.quota!==undefined&&at+buffer.length>Math.max(this.quota,this.data.length)){const error=new Error('quota');error.name=K.quotaError;throw error;}
    const torn=call===this.crashAt?buffer.subarray(0,buffer.length>>1):buffer;
    if(at+torn.length>this.data.length)this.#resize(at+torn.length);
    this.data.set(torn,at);
    if(call===this.crashAt)throw new Crash();
    return buffer.length;
  }
  truncate(size){if(this.calls++===this.crashAt)throw new Crash();this.#resize(size);}
  flush(){if(this.calls++===this.crashAt)throw new Crash();this.durable=this.data.slice();}
  close(){this.closed=true;}
}
class Crash extends Error {}
const container=(handle)=>new KvContainer(handle,{random:()=>STEP_SOURCE});
const set=(key,value,expiration)=>({kind:K.mutations.set,key:bytes(...key),encoding:K.encodings.v8,value:bytes(...value),expiration});
const snapshot=(store)=>JSON.stringify([...store.databases].map(([path,db])=>[path,db.version,db.entries.map(e=>[[...e.key],e.encoding,[...e.value],e.version,e.expiration])]));

test('a database is created, committed to and read back after a reload',()=>{
  const handle=new FakeHandle(),store=container(handle);
  assert.equal(store.unusable(PATH),undefined);
  assert.equal(store.has(PATH),false);
  assert.equal(store.create(PATH),undefined);
  const first=store.commit(PATH,[{key:bytes(1),versionstamp:null}],[set([1],[7]),set([3],[9])]);
  assert.deepEqual(first.versionstamp,versionstamp(STEP));
  // A check against a stale versionstamp fails and writes nothing.
  assert.deepEqual(store.commit(PATH,[{key:bytes(1),versionstamp:null}],[set([2],[8])]),{checkFailed:true});
  const second=store.commit(PATH,[{key:bytes(1),versionstamp:versionstamp(STEP)}],[{kind:K.mutations.remove,key:bytes(3)},set([2],[8])]);
  assert.deepEqual(second.versionstamp,versionstamp(2*STEP));
  const reloaded=container(new FakeHandle(handle.durable));
  assert.equal(snapshot(reloaded),snapshot(store));
  const [range]=reloaded.read(PATH,[{start:bytes(0),end:bytes(255),limit:10,reverse:false}]);
  assert.deepEqual(range.map(e=>[...e.key]),[[1],[2]]);
  assert.deepEqual(range.map(e=>e.version),[STEP,2*STEP]);
  const [reverse]=reloaded.read(PATH,[{start:bytes(0),end:bytes(255),limit:1,reverse:true}]);
  assert.deepEqual(reverse.map(e=>[...e.key]),[[2]]);
  // Keys order as SQLite blobs: bytes, then length.
  assert(compareBytes(bytes(1),bytes(1,0))<0&&compareBytes(bytes(2),bytes(1,255))>0);
});

test('KvU64 sum, min and max follow denokv_sqlite, and a failing one keeps the earlier mutations',()=>{
  const store=container(new FakeHandle());store.create(PATH);
  const op=(kind,key,number)=>({kind:K.mutations[kind],key:bytes(key),number});
  store.commit(PATH,[],[op('sum',1,5n),op('min',2,7n),op('max',3,7n)]);
  store.commit(PATH,[],[op('sum',1,(1n<<64n)-1n),op('min',2,3n),op('max',3,3n)]);
  const values=()=>store.read(PATH,[{start:bytes(0),end:bytes(255),limit:10,reverse:false}])[0].map(e=>[e.encoding,new DataView(e.value.buffer).getBigUint64(0,true),e.expiration]);
  assert.deepEqual(values(),[[K.encodings.u64,4n,-1],[K.encodings.u64,3n,-1],[K.encodings.u64,7n,-1]]);
  const before=store.databases.get(PATH).version;
  const outcome=store.commit(PATH,[],[set([4],[1]),op('sum',4,1n),set([5],[1])]);
  assert.deepEqual(outcome.error,{name:K.errors.typeError,message:"Failed to perform 'sum' mutation on a non-U64 value in the database"});
  // The set before the failing sum is applied under the new version; the
  // one after it is not (denokv_sqlite 0.14.0 commits the transaction).
  const keys=store.read(PATH,[{start:bytes(0),end:bytes(255),limit:10,reverse:false}])[0].map(e=>[e.key[0],e.version]);
  assert.deepEqual(keys,[[1,2*STEP],[2,2*STEP],[3,2*STEP],[4,before+STEP]]);
});

test('expired entries are collected with one version step',()=>{
  const handle=new FakeHandle(),store=container(handle);store.create(PATH);
  store.commit(PATH,[],[set([1],[1],1000),set([2],[2],5000),set([3],[3])]);
  const version=store.databases.get(PATH).version;
  assert.equal(store.collect(PATH,999),undefined);
  assert.equal(store.databases.get(PATH).version,version);
  assert.equal(store.collect(PATH,1000),undefined);
  assert.equal(store.databases.get(PATH).version,version+1);
  assert.equal(snapshot(container(new FakeHandle(handle.durable))),snapshot(store));
  assert.deepEqual(store.read(PATH,[{start:bytes(0),end:bytes(9),limit:9,reverse:false}])[0].map(e=>e.key[0]),[2,3]);
});

test('a crash at any write, flush or truncate leaves either the old or the new state',()=>{
  // The operations of one session, each a durable step.
  const steps=[(s)=>s.create(PATH),(s)=>s.commit(PATH,[],[set([1],[1])]),(s)=>s.create(OTHER),(s)=>s.commit(PATH,[],[set([1],[2]),set([2],[2])])];
  const states=[];
  {const store=container(new FakeHandle());states.push(snapshot(store));for(const step of steps){step(store);states.push(snapshot(store));}}
  let crashes=0;
  for(let crashAt=0;;crashAt++){
    const handle=new FakeHandle();handle.crashAt=crashAt;
    const store=container(handle);let done=0,crashed=false;
    try{for(const step of steps){const result=step(store);if(result?.error||result?.name)throw new Crash();done++;}}catch{crashed=true;}
    if(!crashed)break;
    crashes++;
    // Unflushed bytes may or may not reach the disk.
    for(const image of [handle.durable,handle.data]){
      const reloaded=container(new FakeHandle(image));
      assert.equal(reloaded.failure,undefined,`crash ${crashAt}`);
      assert([states[done],states[done+1]].includes(snapshot(reloaded)),`crash ${crashAt}: ${snapshot(reloaded)}`);
    }
  }
  assert(crashes>=steps.length*2,`${crashes} crash points`);
});

test('a crash anywhere in a commit that compacts the log leaves the old or the new state',()=>{
  // One key rewritten with large values: the log outgrows the live bytes.
  const value=new Array(8*1024).fill(3);
  const grow=(store)=>store.commit(PATH,[],[set([1],value)]);
  const base=new FakeHandle(),store=container(base);store.create(PATH);
  // Commit until the next commit compacts (the file then shrinks).
  const compacts=()=>{const probe=new FakeHandle(base.durable);grow(container(probe));return probe.data.length<base.durable.length;};
  while(!compacts())grow(store);
  const before=snapshot(store),image=base.durable.slice();
  const next=(()=>{const clean=container(new FakeHandle(image));grow(clean);return snapshot(clean);})();
  let crashes=0;
  for(let crashAt=0;;crashAt++){
    const handle=new FakeHandle(image);handle.crashAt=crashAt;
    const outcome=grow(container(handle));
    if(handle.calls<=crashAt)break;
    crashes++;
    for(const image of [handle.durable,handle.data]){
      const reloaded=container(new FakeHandle(image));
      assert.equal(reloaded.failure,undefined,`crash ${crashAt}`);
      // A crash after the commit point (a reported versionstamp) keeps it.
      const states=outcome.versionstamp?[next]:[before,next];
      assert(states.includes(snapshot(reloaded)),`crash ${crashAt}`);
    }
  }
  // The record, its flush, the header and its flush, then the compaction's
  // checkpoint, header, move, header, truncate and flushes.
  assert(crashes>=8,`${crashes} crash points`);
});

test('a full quota fails a commit cleanly and a later commit succeeds',()=>{
  const handle=new FakeHandle(),store=container(handle);store.create(PATH);
  const before=snapshot(store);
  handle.quota=handle.data.length;
  const outcome=store.commit(PATH,[],[set([1],[1])]);
  assert.deepEqual(outcome,{error:{name:K.errors.typeError,message:FAILURES.full(PATH,K.quotaError)}});
  assert.equal(snapshot(store),before);
  assert.equal(store.unusable(PATH),undefined);
  handle.quota=undefined;
  assert.deepEqual(store.commit(PATH,[],[set([1],[1])]).versionstamp,versionstamp(STEP));
  assert.equal(snapshot(container(new FakeHandle(handle.durable))),snapshot(store));
});

test('a failed header write leaves the container unusable until a reload',()=>{
  const handle=new FakeHandle(),store=container(handle);store.create(PATH);
  // The record write and its flush succeed; the header write is torn.
  handle.crashAt=handle.calls+2;
  const outcome=store.commit(PATH,[],[set([1],[1])]);
  assert.equal(outcome.error.message,FAILURES.uncertain(PATH));
  assert.deepEqual(store.unusable(PATH),{name:K.errors.typeError,message:FAILURES.uncertain(PATH)});
  assert.equal(container(new FakeHandle(handle.data)).failure,undefined);
});

test('a corrupt container refuses every database with the corruption named',()=>{
  const handle=new FakeHandle(),store=container(handle);store.create(PATH);store.commit(PATH,[],[set([1],[1])]);
  const damaged=handle.durable.slice();damaged[damaged.length-1]^=0xff;
  const broken=container(new FakeHandle(damaged));
  assert.equal(broken.corrupt,true);
  assert.equal(broken.unusable(PATH).message,FAILURES.corrupt(PATH,'a record checksum does not match'));
  for(const [image,detail] of [[bytes(1,2,3),'the header is truncated'],[new Uint8Array(2*CONTAINER.slotBytes).fill(1),'no valid header']]){
    assert.equal(container(new FakeHandle(image)).unusable(OTHER).message,FAILURES.corrupt(OTHER,detail));
  }
  // An empty file is a new container.
  assert.equal(container(new FakeHandle()).unusable(PATH),undefined);
  assert.equal(crc32(new TextEncoder().encode('123456789')),0xcbf43926);
});

test('the log is compacted into one checkpoint and keeps its state',()=>{
  const handle=new FakeHandle(),store=container(handle);store.create(PATH);
  const value=new Array(512).fill(7);
  for(let i=0;i<400;i++)store.commit(PATH,[],[set([i%4],value)]);
  assert(handle.data.length<CONTAINER.compactMinBytes*2,`${handle.data.length} bytes`);
  assert.equal(snapshot(container(new FakeHandle(handle.durable))),snapshot(store));
  assert.equal(store.unusable(PATH),undefined);
});

/** A minimal OPFS: one file per path, whose sync handle is exclusive. */
function fakeStorage({fail,held=new Set(),lockName=K.lockedErrors[0]}={}){
  const files=new Map();
  const directory=(prefix)=>({
    async getDirectoryHandle(name){return directory(`${prefix}/${name}`);},
    async getFileHandle(name){const path=`${prefix}/${name}`;return {async createSyncAccessHandle(){
      if(held.has(path)){const error=new Error('held');error.name=lockName;throw error;}
      held.add(path);const handle=files.get(path)??new FakeHandle();files.set(path,handle);handle.path=path;return handle;
    }};},
  });
  return {files,held,async getDirectory(){if(fail)throw fail;return directory('');}};
}
const IDENTITY=registry.permissions.artifactPrefix+'ab'.repeat(32), SECOND=registry.permissions.artifactPrefix+'cd'.repeat(32);

test('one container per artifact, held exclusively; a second holder is locked out',async()=>{
  const storage=fakeStorage();
  const first=await openKvStorage(storage,IDENTITY);
  assert(first.container instanceof KvContainer);
  assert.deepEqual([...storage.held],[`/${CONTAINER.directory}/${CONTAINER.artifactPrefix}${'ab'.repeat(32)}/${CONTAINER.file}`]);
  assert.deepEqual(await openKvStorage(storage,IDENTITY),{locked:true});
  // WebKit refuses a held file with its own error name.
  for(const lockName of K.lockedErrors){
    const other=fakeStorage({lockName});await openKvStorage(other,IDENTITY);
    assert.deepEqual(await openKvStorage(other,IDENTITY),{locked:true},lockName);
  }
  // Another artifact (a new version included) has its own, empty container.
  const second=await openKvStorage(storage,SECOND);
  assert(second.container instanceof KvContainer&&second.container.databases.size===0);
  const error=new Error('denied');error.name='SecurityError';
  assert.deepEqual(await openKvStorage(fakeStorage({fail:error}),IDENTITY),{failure:'SecurityError: denied'});
  await assert.rejects(openKvStorage(storage,'md5:00'),TypeError);
  assert.equal(syncHandlesSupported({}),false);
  class Handle{createSyncAccessHandle(){}}
  assert.equal(syncHandlesSupported({FileSystemFileHandle:Handle,navigator:{storage:{getDirectory(){}}}}),true);
  assert.equal(syncHandlesSupported({FileSystemFileHandle:class{},navigator:{storage:{getDirectory(){}}}}),false);
});

test('virtual paths and patterns follow the native file-root rules',()=>{
  assert.deepEqual(parseVirtualPath('/data/app.kv'),{root:'data',components:['app.kv']});
  for(const bad of ['data/app.kv','/Data/app.kv','/data//app.kv','/data/./app.kv','/data/../x','/data/a\\b','/data/a\u0085','/9data/x','/'+'d'.repeat(65)+'/x','/data/'+'x'.repeat(4096)])assert.equal(parseVirtualPath(bad),undefined,bad);
  assert(virtualMatches('/data/*.kv','/data/app.kv'));
  assert(!virtualMatches('/data/*.kv','/data/a/app.kv'));
  assert(virtualMatches('/data/**','/data/a/b.kv'));
  assert(virtualMatches('/data/a?.kv','/data/ab.kv')&&!virtualMatches('/data/a?.kv','/data/a/.kv'));
  assert(!virtualMatches('/data/app.kv','/data/app.kv2'));
  assert(virtualMatches('/data/app.kv','/data/app.kv'));
  assert(!virtualMatches('*.kv','/data/app.kv'));
  assert(glob('','')&&!glob('a',''));
  const manifest={fsRoots:['data'],capabilities:[{id:K.capabilities.read,patterns:['/data/*.kv']},{id:K.capabilities.write,patterns:['/data/app.kv']}]};
  const open=createKvPolicy(manifest,undefined);
  assert(open.covers(K.capabilities.read,PATH)&&open.covers(K.capabilities.write,PATH));
  assert(open.covers(K.capabilities.read,OTHER)&&!open.covers(K.capabilities.write,OTHER));
  assert(!open.covers(K.capabilities.read,'/cache/app.kv'));
  // A ceiling narrows the request; an empty or missing ceiling grant denies.
  const ceiling=createKvPolicy(manifest,{[K.capabilities.read]:{patterns:['/data/app.kv']}});
  assert(ceiling.covers(K.capabilities.read,PATH)&&!ceiling.covers(K.capabilities.read,OTHER)&&!ceiling.covers(K.capabilities.write,PATH));
  assert(!createKvPolicy(manifest,{}).covers(K.capabilities.read,PATH));
  // An undeclared root denies even when a pattern names it.
  assert(!createKvPolicy({...manifest,fsRoots:[]},undefined).covers(K.capabilities.read,PATH));
});

const CODEC=new WebAssembly.Module(readFileSync(new URL('./kv-codec.wasm',import.meta.url)));
// A guest module with no static sections (a single-page static region).
const EMPTY_GUEST=new WebAssembly.Module(Uint8Array.of(0,0x61,0x73,0x6d,1,0,0,0));
const RW_GRANTS=[{id:K.capabilities.read,patterns:['/data/**']},{id:K.capabilities.write,patterns:['/data/**']}];
/** The `kv_*` imports over `container`, with a broker that grants every
 * covered operation and reports `writeGranted` for `kv.write`. Strings are
 * written to guest memory as the guest passes them (UTF-16LE). */
function kvHost({container,grants=RW_GRANTS,writeGranted=true}){
  const memory=new WebAssembly.Memory({initial:1}),O=K.operations;
  const codec=createCodec({codec:CODEC,guest:EMPTY_GUEST,memory:()=>memory,exports:()=>({}),deadlinePassed:()=>false,chargeFuel:()=>{},version:{global:'',value:0}});
  const broker={operation:(id,path,covered)=>covered,granted:(id)=>id!==K.capabilities.write||writeGranted};
  const host=createKvHost({memory:()=>memory,codec,broker,policy:createKvPolicy({fsRoots:['data'],capabilities:grants},undefined),
    storage:{container},emit:()=>{},writeResult:(value)=>value,chargeBytes:()=>{},chargeFuel:()=>{},turnDeadline:()=>Infinity});
  const pass=(text)=>{
    const units=Uint16Array.from(text,(char)=>char.charCodeAt(0));
    new Uint8Array(memory.buffer).set(new Uint8Array(units.buffer));
    return [0,units.byteLength];
  };
  const outcome=(result)=>result===K.failed?host[O.text]():result;
  return {
    open:(path)=>outcome(host[O.open](...pass(path))),
    read:(id)=>outcome(host[O.read](...pass([K.descriptorVersion,id,0].join(K.separator)))),
  };
}
const failureText=(name,message)=>`${name}${K.separator}${message}`;

test('a database below a directory of its root is refused by name, as the container has none',()=>{
  const root='/data';
  const host=kvHost({container:container(new FakeHandle()),grants:RW_GRANTS.map(grant=>({...grant,patterns:[...grant.patterns,root]}))});
  const nested='/data/sub/app.kv';
  assert.equal(host.open(nested),failureText(K.errors.typeError,FAILURES.unavailable(nested,K.noDirectories)));
  assert.equal(host.open('/data/a/b/c.kv'),failureText(K.errors.typeError,FAILURES.unavailable('/data/a/b/c.kv',K.noDirectories)));
  // A root-level database opens; the root itself names no file.
  assert.equal(host.open(PATH),1);
  assert.equal(host.open(root),failureText(K.errors.typeError,FAILURES.noFile(root)));
});

test('a database opens writable when kv.write is covered, whatever the broker decides, as native',()=>{
  const store=container(new FakeHandle());
  // kv.write covered but not granted: the missing database is created.
  assert.equal(kvHost({container:store,writeGranted:false}).open(PATH),1);
  assert(store.has(PATH));
  // kv.write not covered: a missing database is not found.
  const readOnly=kvHost({container:store,grants:[RW_GRANTS[0]]});
  assert.equal(readOnly.open(OTHER),failureText(K.errors.notFound,FAILURES.notFound(OTHER)));
});

test('a container left uncertain by an unfinished write refuses reads too',()=>{
  const handle=new FakeHandle(),store=container(handle),host=kvHost({container:store});
  const id=host.open(PATH);
  assert.equal(host.read(id),0);
  handle.crashAt=handle.calls+2;
  assert.equal(store.commit(PATH,[],[set([1],[1])]).error.message,FAILURES.uncertain(PATH));
  assert.equal(host.read(id),failureText(K.errors.typeError,FAILURES.uncertain(PATH)));
  assert.equal(host.open(PATH),failureText(K.errors.typeError,FAILURES.uncertain(PATH)));
});

test('the checked-in codec module has exactly the registry interface',async()=>{
  const module=new WebAssembly.Module(readFileSync(new URL('./kv-codec.wasm',import.meta.url)));
  const C=K.codec;
  const imports=WebAssembly.Module.imports(module);
  assert(imports.every(item=>item.module===C.importModule&&Object.values(C.imports).includes(item.name)&&item.kind==='function'),JSON.stringify(imports));
  const exports=new Set(WebAssembly.Module.exports(module).map(item=>item.name));
  for(const name of Object.values(C.exports))assert(exports.has(name),name);
  // Configured for a single-page module (no static sections).
  const guest=new WebAssembly.Module(Uint8Array.of(0,0x61,0x73,0x6d,1,0,0,0));
  const memory=new WebAssembly.Memory({initial:1});
  const codec=createCodec({codec:module,guest,memory:()=>memory,exports:()=>({}),deadlinePassed:()=>false,chargeFuel:()=>{},version:{global:'',value:0}});
  assert.equal(typeof codec[C.exports.stage],'function');
});

// The Worker with a fake guest module importing `kv_open`: the KV storage
// decision before instantiation.
async function startWorker({grants,scope}){
  const code=(await Promise.all(['scheduler.js','timers.js','network-policy.js','network.js','worker.js'].map(name=>readFile(new URL(name,import.meta.url),'utf8')))).join('\n');
  const [permissionBroker,eventSources,viewPolicy,viewWorker,environment,environmentAbi,kvStore]=await Promise.all(['./permission-broker.mjs','./event-sources.mjs','./view-policy.mjs','./view-worker.mjs','./environment.mjs','./environment-abi.mjs','./kv-store.mjs'].map(name=>import(name)));
  const messages=[],descriptor={profile:'checked-f64-utf16-v38',schemaVersion:1,fairFuel:true,turnBudgets:true,
    budgets:{fuelPerTurn:1000000,hostCallsPerTurn:1024,outputBytes:1048576,deadlinePerTurnMs:1000},
    imports:[{module:'varta',name:K.operations.open}],hostModule:'varta',memoryExport:'memory',pendingExport:'pending',startExport:'_start',
    original:{identity:IDENTITY},maxStringBytes:65536,decodeChunkBytes:65536,resources:[]};
  let instantiated=false;
  const self={...scope,postMessage(message){messages.push(message);if(message.type==='turn_end')self.onmessage({data:{type:'turn_ack',id:message.id}});}};
  class Tank{constructor(fuel){this.remaining=fuel;this.global={value:BigInt(-fuel)};}charge(){}beginTurn(fuel){this.remaining=fuel;}}
  const wasm={compile:async()=>({}),Module:{imports:()=>[{module:'varta',name:K.operations.open,kind:'function'},{module:registry.meter.module,name:registry.meter.counter,kind:'global'}],customSections:()=>{const bytes=new Uint8Array(4);new DataView(bytes.buffer).setUint32(0,registry.meter.version,true);return[bytes.buffer];}},
    instantiate:async()=>{instantiated=true;const version=registry.profiles.find(profile=>profile.name===descriptor.profile).exceptionVersion;return{exports:{memory:new WebAssembly.Memory({initial:1}),[version.global]:{value:version.value},_start(){}}};}};
  const context=vm.createContext({registry,permissionBroker,eventSources,viewPolicy,viewWorker,environment,environmentAbi,kvStore,sessionChannel,self,navigator:scope.navigator,WebAssembly:wasm,FuelTank:Tank,bundleSchemaVersion:1,
    reasonReader:()=>()=>'',browserTransport:()=>{throw new Error('unexpected transport');},TextEncoder,TextDecoder,URL,Uint8Array,Uint32Array,DataView,performance,setTimeout,clearTimeout,Date,crypto});
  vm.runInContext(code,context);
  await self.onmessage({data:{descriptor,image:new Uint8Array(),kvCodec:new Uint8Array(),manifest:new TextEncoder().encode(JSON.stringify({fsRoots:['data'],capabilities:grants})),mode:'run'}});
  return {result:messages.at(-1),instantiated};
}
const KV_GRANTS=[{id:K.capabilities.read,patterns:['/data/*.kv']}];

test('a Worker without sync access handles refuses a granted KV program by name before instantiation',async()=>{
  const {result,instantiated}=await startWorker({grants:KV_GRANTS,scope:{}});
  assert.equal(result.status,'failed');
  assert.equal(result.error,K.storageUnavailable+K.capabilities.read);
  assert.equal(instantiated,false);
  // Without a covered grant nothing is acquired or refused: every open is
  // then denied as a host without a grant denies it.
  let opened=0;
  class Handle{createSyncAccessHandle(){}}
  const scope={FileSystemFileHandle:Handle,navigator:{storage:{getDirectory(){opened++;throw new Error('unexpected storage');}}}};
  const plain=await startWorker({grants:[],scope:{}});
  assert.equal(plain.result.status,'ok',plain.result.error);
  assert.equal(plain.instantiated,true);
  const granted=await startWorker({grants:KV_GRANTS,scope});
  assert.equal(granted.result.status,'ok',granted.result.error);
  assert.equal(opened,1);
});
