import {registry} from '../../../varta-common/browser/registry.mjs';
// The official browser host's v38 KV storage (`kv_*`; docs/embedding.md, KV
// storage (v38)), on the agreed design (docs/development-plan.md, Browser KV
// storage):
//
// - Values: the shared Rust value layer (`varta-kv-codec`) compiled to wasm32
//   (`kv-codec.wasm`), the code the native runtime runs: staging, the V8
//   codec, Deno's key codec, descriptors, cursors, versionstamp text, fair
//   fuel and the decode into the module's schema, built through the guest's
//   own allocator. This module only moves bytes between it, the guest and
//   the storage below.
// - Storage: one Varta KV container file per artifact in the origin private
//   file system (OPFS), under `varta-kv/sha256-<digest>/`, held through one
//   `FileSystemSyncAccessHandle` the dedicated Worker acquires before the
//   guest is instantiated. Its reads, writes and flushes are synchronous, so
//   every `kv_*` import stays a synchronous host call and a commit is
//   reported only once it is durable. Another tab or Worker holding the
//   handle makes every open fail as native does on a database another writer
//   locks.
// - Authority: `kv.read` and `kv.write` path patterns of the package and the
//   ceiling, matched like the native virtual file roots, decided by the
//   broker before any storage is touched. Without a covered grant the host
//   behaves as one without KV storage: every open is denied.
const K=registry.kv, C=K.codec, encoder=new TextEncoder(), decoder=new TextDecoder('utf-8',{fatal:true});
// The bytes of one UTF-16 code unit: a path's byte length is a multiple.
const UTF16_UNIT_BYTES=Uint16Array.BYTES_PER_ELEMENT;
const U32_BYTES=Uint32Array.BYTES_PER_ELEMENT, U64_BYTES=BigUint64Array.BYTES_PER_ELEMENT, F64_BYTES=Float64Array.BYTES_PER_ELEMENT;
const U64_MASK=(1n<<64n)-1n;

// ---------------------------------------------------------------------------
// The Varta KV container, format version 1.
//
// [header slot A][header slot B][log records ...]
//
// A header slot is MAGIC, the format version, a sequence number, the log's
// start and end offsets and a CRC-32 of those fields. The valid slot with the
// highest sequence is current; a write alternates slots, so a torn header
// write leaves the other slot current. Between start and end the log holds
// framed records (payload length, CRC-32 of the payload, payload): a
// checkpoint of every database, the creation of a database, or one write
// (a commit's resolved puts and deletes, or an expiry collection) with the
// database's new data version.
//
// A write appends its record past the current end, flushes, then writes the
// other header slot with the new end and flushes again: COMMITTED only after
// that last flush. A crash before it leaves the old end current (the old
// state); after it, the new state. Records past the end are ignored and
// overwritten. When the log outgrows the live data it is compacted into one
// checkpoint, which never changes the logical state.
export const CONTAINER=Object.freeze({
  directory:'varta-kv',
  artifactPrefix:'sha256-',
  file:'kv.varta',
  magic:'VARTAKV\0',
  formatVersion:K.containerFormat,
  slotBytes:64,
  slots:2,
  // Record kinds and write operations.
  checkpoint:1, create:2, write:3,
  put:1, remove:2,
  // A value without expiration (Deno's `expiration_ms` of -1).
  noExpiration:-1,
  // Compact once the log holds more than this many times the live bytes,
  // and at least this many bytes.
  compactFactor:4,
  compactMinBytes:64*1024,
});
const HEADER_BYTES=CONTAINER.slotBytes*CONTAINER.slots;
// Header slot fields: magic, version, sequence, start, end, CRC.
const SLOT=Object.freeze({version:8,sequence:16,start:24,end:32,crc:40});
const RECORD_HEADER_BYTES=2*U32_BYTES;
// Data versions advance by a random step of 1 to this bound at each commit
// (`denokv_sqlite`'s `gen_range(1..10)`) and by one at a collection.
const MAX_VERSION_STEP=9;
const VERSIONSTAMP_BYTES=10;

/** The failure texts (`registry.kv.texts`, each a recoverable TypeError
 * but `notFound`), with the path and the failure filled in. Function
 * replacements keep a `$` in a path literal. */
const fill=(template,path,failure='',operation='')=>template.replace(K.pathPlaceholder,()=>path).replace(K.failurePlaceholder,()=>failure).replace(K.operationPlaceholder,()=>operation);
export const FAILURES=Object.freeze(Object.fromEntries(Object.entries(K.texts).map(([name,template])=>[name,(path,failure)=>fill(template,path,failure)])));
const nonU64=(operation)=>fill(K.texts.nonU64,'','',operation);

const CRC_TABLE=(()=>{
  // The CRC-32 (IEEE 802.3, reflected) table.
  const POLYNOMIAL=0xedb88320, table=new Uint32Array(256);
  for(let n=0;n<table.length;n++){let c=n;for(let k=0;k<8;k++)c=c&1?POLYNOMIAL^(c>>>1):c>>>1;table[n]=c>>>0;}
  return table;
})();
export function crc32(bytes){
  let crc=0xffffffff;
  for(const byte of bytes)crc=CRC_TABLE[(crc^byte)&0xff]^(crc>>>8);
  return (crc^0xffffffff)>>>0;
}

/** Lexicographic byte order, SQLite's order of blob keys. */
export function compareBytes(a,b){
  const n=Math.min(a.length,b.length);
  for(let i=0;i<n;i++)if(a[i]!==b[i])return a[i]-b[i];
  return a.length-b.length;
}
const sameBytes=(a,b)=>a.length===b.length&&compareBytes(a,b)===0;

/** Little-endian field writer. */
class Out {
  constructor(){this.parts=[];this.length=0;}
  push(bytes){this.parts.push(bytes);this.length+=bytes.length;return this;}
  u8(value){return this.push(Uint8Array.of(value));}
  u32(value){const b=new Uint8Array(U32_BYTES);new DataView(b.buffer).setUint32(0,value,true);return this.push(b);}
  u64(value){const b=new Uint8Array(U64_BYTES);new DataView(b.buffer).setBigUint64(0,BigInt(value),true);return this.push(b);}
  f64(value){const b=new Uint8Array(F64_BYTES);new DataView(b.buffer).setFloat64(0,value,true);return this.push(b);}
  bytes(bytes){return this.u32(bytes.length).push(bytes);}
  done(){const out=new Uint8Array(this.length);let at=0;for(const part of this.parts){out.set(part,at);at+=part.length;}return out;}
}
/** Little-endian field reader; throws past the end. */
class In {
  constructor(bytes){this.data=bytes;this.view=new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength);this.at=0;}
  need(count){if(this.at+count>this.data.length)throw new RangeError('truncated field');const at=this.at;this.at+=count;return at;}
  u8(){return this.data[this.need(1)];}
  u32(){return this.view.getUint32(this.need(U32_BYTES),true);}
  u64(){return this.view.getBigUint64(this.need(U64_BYTES),true);}
  safe(){const value=this.u64();if(value>BigInt(Number.MAX_SAFE_INTEGER))throw new RangeError('version out of range');return Number(value);}
  f64(){return this.view.getFloat64(this.need(F64_BYTES),true);}
  bytes(){return this.fixed(this.u32());}
  fixed(count){const at=this.need(count);return this.data.slice(at,at+count);}
  get done(){return this.at===this.data.length;}
}

/** The versionstamp of data version `version`: eight big-endian bytes and
 * two zero bytes (`denokv_sqlite`'s `version_to_versionstamp`). */
export function versionstamp(version){
  const bytes=new Uint8Array(VERSIONSTAMP_BYTES);
  new DataView(bytes.buffer).setBigUint64(0,BigInt(version),false);
  return bytes;
}

function storageError(error){return `${error?.name??'Error'}: ${error?.message??String(error)}`;}

/** The databases of one artifact in one container file, over a
 * `FileSystemSyncAccessHandle` (or any object with its synchronous
 * `read`, `write`, `truncate`, `getSize`, `flush` and `close`). */
export class KvContainer {
  #handle; #random; #sequence=0; #start=HEADER_BYTES; #end=HEADER_BYTES; #live=0;
  constructor(handle,{random=()=>crypto.getRandomValues(new Uint32Array(1))[0]}={}){
    this.#handle=handle;this.#random=random;this.databases=new Map();this.failure=undefined;this.uncertain=false;
    try{this.#load();}
    catch(error){this.databases.clear();this.failure=error instanceof CorruptContainer?error.message:storageError(error);this.corrupt=error instanceof CorruptContainer;}
  }
  close(){try{this.#handle.close();}catch{}}
  #read(at,length){
    const bytes=new Uint8Array(length);
    const read=this.#handle.read(bytes,{at});
    if(read!==length)throw new CorruptContainer('the file ends early');
    return bytes;
  }
  #load(){
    const size=this.#handle.getSize();
    if(size===0)return;
    if(size<HEADER_BYTES)throw new CorruptContainer('the header is truncated');
    const header=this.#read(0,HEADER_BYTES);
    let current;
    for(let slot=0;slot<CONTAINER.slots;slot++){
      const fields=readSlot(header.subarray(slot*CONTAINER.slotBytes,(slot+1)*CONTAINER.slotBytes),size);
      if(fields&&(!current||fields.sequence>current.sequence))current=fields;
    }
    // The first header write (sequence 1) goes to the second slot and the
    // first slot stays zero until the next one: no valid slot and a zero
    // first slot is a first commit that never completed, an empty
    // container. Any other invalid header is corrupt.
    if(!current&&header.subarray(0,CONTAINER.slotBytes).every(byte=>byte===0))return;
    if(!current)throw new CorruptContainer('no valid header');
    this.#sequence=current.sequence;this.#start=current.start;this.#end=current.end;
    const log=this.#read(this.#start,this.#end-this.#start);
    let at=0;
    while(at<log.length){
      if(at+RECORD_HEADER_BYTES>log.length)throw new CorruptContainer('a record is truncated');
      const view=new DataView(log.buffer,log.byteOffset+at);
      const length=view.getUint32(0,true),crc=view.getUint32(U32_BYTES,true);
      const start=at+RECORD_HEADER_BYTES;
      if(start+length>log.length)throw new CorruptContainer('a record is truncated');
      const payload=log.subarray(start,start+length);
      if(crc32(payload)!==crc)throw new CorruptContainer('a record checksum does not match');
      try{this.#replay(new In(payload));}catch(error){throw error instanceof CorruptContainer?error:new CorruptContainer('a record is malformed');}
      at=start+length;
    }
    this.#live=this.#measure();
  }
  #replay(input){
    const kind=input.u8();
    if(kind===CONTAINER.checkpoint){
      this.databases.clear();
      const count=input.u32();
      for(let i=0;i<count;i++){
        const path=decoder.decode(input.bytes()),version=input.safe(),entries=[],n=input.u32();
        for(let j=0;j<n;j++){
          const key=input.bytes(),encoding=input.u8(),value=input.bytes(),entryVersion=input.safe(),expiration=input.f64();
          if(entries.length&&compareBytes(entries.at(-1).key,key)>=0)throw new CorruptContainer('checkpoint keys are out of order');
          entries.push({key,encoding,value,version:entryVersion,expiration});
        }
        if(this.databases.has(path))throw new CorruptContainer('a database is listed twice');
        this.databases.set(path,{version,entries});
      }
    }else if(kind===CONTAINER.create){
      const path=decoder.decode(input.bytes());
      if(this.databases.has(path))throw new CorruptContainer('a database is created twice');
      this.databases.set(path,{version:0,entries:[]});
    }else if(kind===CONTAINER.write){
      const path=decoder.decode(input.bytes()),database=this.databases.get(path);
      if(!database)throw new CorruptContainer('a write names no database');
      const version=input.safe(),count=input.u32();
      if(version<database.version)throw new CorruptContainer('a data version goes back');
      database.version=version;
      for(let i=0;i<count;i++){
        const op=input.u8(),key=input.bytes();
        if(op===CONTAINER.put){const encoding=input.u8(),value=input.bytes(),expiration=input.f64();put(database,{key,encoding,value,version,expiration});}
        else if(op===CONTAINER.remove)remove(database,key);
        else throw new CorruptContainer('an unknown write operation');
      }
    }else throw new CorruptContainer('an unknown record');
    if(!input.done)throw new CorruptContainer('a record has trailing bytes');
  }
  #measure(){
    let bytes=0;
    for(const [path,database] of this.databases){bytes+=encoder.encode(path).length;for(const entry of database.entries)bytes+=entry.key.length+entry.value.length;}
    return bytes;
  }
  /** Append records and make them current: the commit point. */
  #append(payloads){
    const frames=new Out();
    for(const payload of payloads)frames.u32(payload.length).u32(crc32(payload)).push(payload);
    const bytes=frames.done(),end=this.#end+bytes.length;
    // A failure here leaves the old end current: nothing was applied.
    if(this.#handle.write(bytes,{at:this.#end})!==bytes.length)throw new Error('short write');
    this.#handle.flush();
    this.#header(this.#start,end);
    this.#end=end;
  }
  /** Write the other header slot for `[start, end)` and flush. A failure
   * here leaves the file in the old or the new state, unknown which. */
  #header(start,end){
    const sequence=this.#sequence+1,slot=sequence%CONTAINER.slots;
    const bytes=writeSlot(sequence,start,end);
    try{
      if(this.#handle.write(bytes,{at:slot*CONTAINER.slotBytes})!==bytes.length)throw new Error('short write');
      this.#handle.flush();
    }catch(error){this.uncertain=true;throw error;}
    this.#sequence=sequence;
  }
  #compact(){
    const log=this.#end-this.#start;
    if(log<=CONTAINER.compactMinBytes||log<=CONTAINER.compactFactor*this.#live)return;
    const checkpoint=new Out().u8(CONTAINER.checkpoint).u32(this.databases.size);
    for(const [path,database] of this.databases){
      checkpoint.bytes(encoder.encode(path)).u64(database.version).u32(database.entries.length);
      for(const entry of database.entries)checkpoint.bytes(entry.key).u8(entry.encoding).bytes(entry.value).u64(entry.version).f64(entry.expiration);
    }
    const payload=checkpoint.done();
    const frame=new Out().u32(payload.length).u32(crc32(payload)).push(payload).done();
    // The checkpoint past the end, made current; then moved to the front
    // when it fits before itself, made current there, and the file cut.
    const at=this.#end;
    if(this.#handle.write(frame,{at})!==frame.length)throw new Error('short write');
    this.#handle.flush();
    this.#header(at,at+frame.length);
    this.#start=at;this.#end=at+frame.length;
    if(HEADER_BYTES+frame.length>at)return;
    if(this.#handle.write(frame,{at:HEADER_BYTES})!==frame.length)throw new Error('short write');
    this.#handle.flush();
    this.#header(HEADER_BYTES,HEADER_BYTES+frame.length);
    this.#start=HEADER_BYTES;this.#end=HEADER_BYTES+frame.length;
    this.#handle.truncate(this.#end);
    this.#handle.flush();
  }
  /** Run a durable change; compaction afterwards never changes its
   * outcome (a failed compaction leaves the same logical state). */
  #durable(payloads,apply){
    this.#append(payloads);
    apply();
    this.#live=this.#measure();
    try{this.#compact();}catch{this.uncertain=true;}
  }
  /** The failure of a write that did not reach its commit point. */
  #failed(path,error){
    if(this.uncertain)return typeError(FAILURES.uncertain(path));
    return typeError(error?.name===K.quotaError?FAILURES.full(path,error.name):FAILURES.unavailable(path,storageError(error)));
  }
  /** Why `path` cannot be used now, as a KV failure, or undefined. */
  unusable(path){
    if(this.failure!==undefined)return typeError(this.corrupt?FAILURES.corrupt(path,this.failure):FAILURES.unavailable(path,this.failure));
    if(this.uncertain)return typeError(FAILURES.uncertain(path));
    return undefined;
  }
  has(path){return this.databases.has(path);}
  /** Create the empty database `path`, durably. */
  create(path){
    const payload=new Out().u8(CONTAINER.create).bytes(encoder.encode(path)).done();
    try{this.#durable([payload],()=>this.databases.set(path,{version:0,entries:[]}));}
    catch(error){return this.#failed(path,error);}
    return undefined;
  }
  /** Delete the entries of `path` expired at `now` (Unix ms), as
   * `denokv_sqlite` collects at a writable open: one version step. */
  collect(path,now){
    const database=this.databases.get(path);
    const expired=database.entries.filter(entry=>entry.expiration>=0&&entry.expiration<=now);
    if(!expired.length)return undefined;
    const version=database.version+1;
    const record=new Out().u8(CONTAINER.write).bytes(encoder.encode(path)).u64(version).u32(expired.length);
    for(const entry of expired)record.u8(CONTAINER.remove).bytes(entry.key);
    try{this.#durable([record.done()],()=>{database.version=version;for(const entry of expired)remove(database,entry.key);});}
    catch(error){return this.#failed(path,error);}
    return undefined;
  }
  /** The entries of each range, as `denokv_sqlite` scans them. */
  read(path,ranges){
    const {entries}=this.databases.get(path);
    return ranges.map(({start,end,limit,reverse})=>{
      const first=lowerBound(entries,start),last=lowerBound(entries,end);
      const selected=last>first?entries.slice(first,last):[];
      if(reverse)selected.reverse();
      return selected.slice(0,limit);
    });
  }
  /** Commit checks and mutations to `path`: `{versionstamp}` once durable,
   * `{checkFailed}`, or `{error}`. Like `denokv_sqlite` 0.14.0, a mutation
   * failing after earlier ones keeps those applied under the new version. */
  commit(path,checks,mutations){
    const database=this.databases.get(path);
    for(const check of checks){
      const entry=find(database,check.key);
      const current=entry?versionstamp(entry.version):null;
      if(current===null?check.versionstamp!==null:check.versionstamp===null||!sameBytes(current,check.versionstamp))return {checkFailed:true};
    }
    const version=database.version+1+this.#random()%MAX_VERSION_STEP;
    const working=new Map(),operations=[];
    const value=key=>{const id=keyId(key);return working.has(id)?working.get(id):find(database,key)??null;};
    let failure;
    for(const mutation of mutations){
      if(mutation.kind===K.mutations.set){
        const entry={key:mutation.key,encoding:mutation.encoding,value:mutation.value,version,expiration:mutation.expiration??CONTAINER.noExpiration};
        working.set(keyId(mutation.key),entry);operations.push(entry);
      }else if(mutation.kind===K.mutations.remove){
        working.set(keyId(mutation.key),null);operations.push({key:mutation.key,removed:true});
      }else{
        const old=value(mutation.key),operation=K.mutationNames[mutation.kind];
        if(old&&old.encoding!==K.encodings.u64){failure=typeError(nonU64(operation));break;}
        const current=old?new DataView(old.value.buffer,old.value.byteOffset).getBigUint64(0,true):undefined;
        const result=current===undefined?mutation.number:operation===K.mutationNames[K.mutations.sum]?(current+mutation.number)&U64_MASK:operation===K.mutationNames[K.mutations.min]?(current<mutation.number?current:mutation.number):(current>mutation.number?current:mutation.number);
        const bytes=new Uint8Array(U64_BYTES);new DataView(bytes.buffer).setBigUint64(0,result,true);
        const entry={key:mutation.key,encoding:K.encodings.u64,value:bytes,version,expiration:CONTAINER.noExpiration};
        working.set(keyId(mutation.key),entry);operations.push(entry);
      }
    }
    const record=new Out().u8(CONTAINER.write).bytes(encoder.encode(path)).u64(version).u32(operations.length);
    for(const op of operations){
      if(op.removed)record.u8(CONTAINER.remove).bytes(op.key);
      else record.u8(CONTAINER.put).bytes(op.key).u8(op.encoding).bytes(op.value).f64(op.expiration);
    }
    try{this.#durable([record.done()],()=>{database.version=version;for(const op of operations){if(op.removed)remove(database,op.key);else put(database,op);}});}
    catch(error){return {error:this.#failed(path,error)};}
    return failure?{error:failure}:{versionstamp:versionstamp(version)};
  }
}
class CorruptContainer extends Error {}
const typeError=(message)=>({name:K.errors.typeError,message});
const keyId=(key)=>Array.from(key,byte=>byte.toString(16).padStart(2,'0')).join('');

function readSlot(bytes,size){
  const view=new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength);
  for(let i=0;i<CONTAINER.magic.length;i++)if(bytes[i]!==CONTAINER.magic.charCodeAt(i))return undefined;
  if(crc32(bytes.subarray(0,SLOT.crc))!==view.getUint32(SLOT.crc,true))return undefined;
  if(view.getUint32(SLOT.version,true)!==CONTAINER.formatVersion)return undefined;
  const number=(at)=>{const value=view.getBigUint64(at,true);return value>BigInt(Number.MAX_SAFE_INTEGER)?undefined:Number(value);};
  const sequence=number(SLOT.sequence),start=number(SLOT.start),end=number(SLOT.end);
  if(sequence===undefined||start===undefined||end===undefined||start<HEADER_BYTES||end<start||end>size)return undefined;
  return {sequence,start,end};
}
function writeSlot(sequence,start,end){
  const bytes=new Uint8Array(CONTAINER.slotBytes),view=new DataView(bytes.buffer);
  for(let i=0;i<CONTAINER.magic.length;i++)bytes[i]=CONTAINER.magic.charCodeAt(i);
  view.setUint32(SLOT.version,CONTAINER.formatVersion,true);
  view.setBigUint64(SLOT.sequence,BigInt(sequence),true);
  view.setBigUint64(SLOT.start,BigInt(start),true);
  view.setBigUint64(SLOT.end,BigInt(end),true);
  view.setUint32(SLOT.crc,crc32(bytes.subarray(0,SLOT.crc)),true);
  return bytes;
}
function lowerBound(entries,key){
  let low=0,high=entries.length;
  while(low<high){const mid=(low+high)>>>1;if(compareBytes(entries[mid].key,key)<0)low=mid+1;else high=mid;}
  return low;
}
function find(database,key){const at=lowerBound(database.entries,key);const entry=database.entries[at];return entry&&sameBytes(entry.key,key)?entry:undefined;}
function put(database,entry){
  const at=lowerBound(database.entries,entry.key),current=database.entries[at];
  const stored={key:entry.key,encoding:entry.encoding,value:entry.value,version:entry.version,expiration:entry.expiration};
  if(current&&sameBytes(current.key,entry.key))database.entries[at]=stored;else database.entries.splice(at,0,stored);
}
function remove(database,key){
  const at=lowerBound(database.entries,key),current=database.entries[at];
  if(current&&sameBytes(current.key,key))database.entries.splice(at,1);
}

/** Whether this Worker can hold OPFS sync access handles at all. A host
 * without them refuses a granted KV program at mount. */
export function syncHandlesSupported(scope=globalThis){
  return typeof scope.FileSystemFileHandle==='function'&&typeof scope.FileSystemFileHandle.prototype.createSyncAccessHandle==='function'&&typeof scope.navigator?.storage?.getDirectory==='function';
}

/** Acquire the container of artifact `identity` (`sha256:<hex>`):
 * `{container}`, `{locked:true}` when another tab or Worker holds it, or
 * `{failure}` when the origin's storage is unavailable. */
export async function openKvStorage(storage,identity){
  const prefix=registry.permissions.artifactPrefix;
  if(typeof identity!=='string'||!identity.startsWith(prefix))throw new TypeError('invalid artifact identity');
  try{
    const root=await storage.getDirectory();
    const area=await root.getDirectoryHandle(CONTAINER.directory,{create:true});
    const directory=await area.getDirectoryHandle(CONTAINER.artifactPrefix+identity.slice(prefix.length),{create:true});
    const file=await directory.getFileHandle(CONTAINER.file,{create:true});
    let handle;
    try{handle=await file.createSyncAccessHandle();}
    catch(error){if(K.lockedErrors.includes(error?.name))return {locked:true};throw error;}
    return {container:new KvContainer(handle)};
  }catch(error){return {failure:storageError(error)};}
}

// ---------------------------------------------------------------------------
// Authority: v38 virtual file roots (`varta_common::files`) and the native
// `virtual_matches` of `kv.*` patterns.

const isControl=(code)=>code<=0x1f||(code>=0x7f&&code<=0x9f);
function validRootName(name){
  return encoder.encode(name).length<=K.paths.maxRootNameBytes&&/^[a-z][a-z0-9_-]*$/.test(name);
}
/** A v38 guest path's root and components, or undefined (a denial). */
export function parseVirtualPath(input){
  if(encoder.encode(input).length>K.paths.maxPathBytes)return undefined;
  for(const char of input){const code=char.codePointAt(0);if(isControl(code)||char==='\\')return undefined;}
  if(!input.startsWith(K.paths.separator))return undefined;
  const [root,...components]=input.slice(K.paths.separator.length).split(K.paths.separator);
  if(!validRootName(root)||components.some(c=>c===''||c==='.'||c==='..'))return undefined;
  return {root,components};
}
/** The native component-aware glob: `*` stays in one directory, `**`
 * crosses it, `?` is one character; bounded work fails closed. */
export function glob(pattern,text){
  if(encoder.encode(pattern).length*encoder.encode(text).length>K.paths.maxMatchWork)return false;
  const p=Array.from(pattern),t=Array.from(text),sep=K.paths.separator;
  let row=new Array(t.length+1).fill(false);row[0]=true;
  for(let i=0;i<p.length;i++){
    const recursive=p[i]==='*'&&p[i+1]==='*',next=new Array(t.length+1).fill(false);
    if(p[i]==='*'){
      next[0]=row[0];
      for(let j=0;j<t.length;j++)next[j+1]=row[j+1]||next[j]&&(recursive||t[j]!==sep);
      if(recursive)i++;
    }else for(let j=0;j<t.length;j++)next[j+1]=row[j]&&(p[i]===t[j]||p[i]==='?'&&t[j]!==sep);
    row=next;
  }
  return row[t.length];
}
export function virtualMatches(pattern,path){
  const boundary=[...'*?'].map(c=>pattern.indexOf(c)).filter(i=>i>=0).reduce((a,b)=>Math.min(a,b),pattern.length);
  const slash=pattern.slice(0,boundary).lastIndexOf(K.paths.separator);
  if(slash<0)return false;
  const prefix=pattern.slice(0,slash);
  if(!path.startsWith(prefix+K.paths.separator))return false;
  return glob(pattern.slice(slash+1),path.slice(prefix.length+K.paths.separator.length));
}
/** Coverage of `kv.read`/`kv.write` on a virtual path: a valid path under a
 * declared root that a request pattern and, when a ceiling exists, a
 * ceiling pattern match. The broker decides on top of it. */
export function createKvPolicy(manifest,ceiling){
  const patterns=(grants,id)=>grants.filter(grant=>grant.id===id).flatMap(grant=>grant.patterns??[]);
  const roots=new Set(Array.isArray(manifest.fsRoots)?manifest.fsRoots:[]);
  return Object.freeze({covers(id,path){
    const parsed=parseVirtualPath(path);
    if(!parsed||!roots.has(parsed.root))return false;
    const granted=list=>list.some(pattern=>virtualMatches(pattern,path));
    return granted(patterns(manifest.capabilities,id))&&(ceiling===undefined||granted(ceiling[id]?.patterns??[]));
  }});
}

// ---------------------------------------------------------------------------
// The `kv_*` imports.

/** The KV imports of one instance. `codec` is the instantiated codec
 * module's exports (bound to this guest by `codecImports`), `storage` the
 * acquisition result (`undefined` without a covered grant). */
export function createKvHost({memory,codec,broker,policy,storage,emit,writeResult,chargeBytes,chargeFuel,turnDeadline,now=()=>Date.now()}) {
  const O=K.operations;
  let text='',nextId=0,staged=[],stagedBytes=0,entries=[];
  const databases=new Map();
  const fatal=(what)=>new Error(`kv: ${what}`);
  const fail=(error)=>{text=`${error.name}${K.separator}${error.message}`;staged=[];stagedBytes=0;return K.failed;};
  // The native descriptor reader (`host::input` with the UTF-16 resource
  // rules): range and length first, then the byte charge, then the text.
  const read=(ptr,len,what)=>{
    if(!Number.isInteger(ptr)||ptr<0||!Number.isInteger(len)||len<0||len>K.maxDescriptorBytes)throw fatal(what);
    chargeBytes(len);
    const buffer=memory().buffer;
    if(len%UTF16_UNIT_BYTES||ptr+len>buffer.byteLength)throw fatal(what);
    let value;
    try{value=new TextDecoder('utf-16le',{fatal:true,ignoreBOM:true}).decode(new Uint8Array(buffer,ptr,len));}
    catch{throw fatal(what);}
    if(value.includes('\0')||encoder.encode(value).length>K.maxDescriptorBytes)throw fatal(what);
    return value;
  };
  // One codec call: input bytes in, a status and output bytes out. A
  // fatal status traps with the codec's message, a deadline with the
  // host's deadline error (`checked`).
  const raw=(name,input,...args)=>{
    const at=codec[C.exports.input](input.length);
    new Uint8Array(codec[C.exports.memory].buffer,at,input.length).set(input);
    const status=codec[name](...args);
    const output=new Uint8Array(codec[C.exports.memory].buffer,codec[C.exports.output](),codec[C.exports.outputLength]()).slice();
    return {status,output};
  };
  const checked=(result)=>{
    if(result.status===C.statuses.fatal)throw new Error(decoder.decode(result.output));
    if(result.status===C.statuses.deadline)throw new Error(K.deadlineError);
    return result;
  };
  const call=(name,input,...args)=>checked(raw(name,input,...args));
  const table=()=>{const out=new Out().u32(staged.length);for(const record of staged)out.push(record);return out.done();};
  const denied=(id,path)=>{
    const message=K.denialText.replace(K.capabilityPlaceholder,()=>id).replace(K.pathPlaceholder,()=>path);
    emit(message);return {name:K.errors.notAllowed,message};
  };
  const granted=(id,path)=>broker.operation(id,path,policy.covers(id,path));
  const database=(id)=>{const open=databases.get(id);if(!open)throw fatal('unknown database id');return open;};
  const open=(ptr,len)=>{
    const path=read(ptr,len,'invalid database path');
    if(!granted(K.capabilities.read,path))return fail(denied(K.capabilities.read,path));
    const writable=policy.covers(K.capabilities.write,path)&&broker.granted(K.capabilities.write);
    if(databases.size===K.maxOpenDatabases)return fail(typeError(K.texts.tooManyDatabases));
    // V38 fair fuel: the open, after the grant and before the storage.
    chargeFuel(K.fuel.operation);
    const name=path.split(K.paths.separator).at(-1);
    if(path.split(K.paths.separator).length<=2)return fail(typeError(FAILURES.noFile(path)));
    if(name.startsWith(K.reservedNamePrefix)||K.sidecarSuffixes.some(suffix=>name.endsWith(suffix)))return fail(typeError(FAILURES.invalidName(path)));
    if(!storage)return fail(typeError(FAILURES.unavailable(path,K.noStorage)));
    if(storage.locked)return fail(typeError(FAILURES.locked(path)));
    if(storage.failure!==undefined)return fail(typeError(FAILURES.unavailable(path,storage.failure)));
    const container=storage.container,unusable=container.unusable(path);
    if(unusable)return fail(unusable);
    if(!container.has(path)){
      if(!writable)return fail({name:K.errors.notFound,message:FAILURES.notFound(path)});
      const created=container.create(path);if(created)return fail(created);
    }
    if(writable){const collected=container.collect(path,now());if(collected)return fail(collected);}
    const id=++nextId;databases.set(id,{path,writable});
    return id;
  };
  const stage=(mode,kind,lo,hi)=>{
    const packed=(BigInt(hi>>>0)<<32n)|BigInt(lo>>>0);
    const result=raw(C.exports.stage,new Uint8Array(0),mode,kind,BigInt.asIntN(64,packed));
    // As native: the stage limit is checked before the walk's outcome.
    if(staged.length===K.maxStaged)throw fatal('too many staged values');
    const {status,output}=checked(result);
    if(status===C.statuses.failed)return fail(failure(output));
    const input=new In(output),fuel=input.u64(),size=input.u32();
    // V38 fair fuel: the encoded key and value bytes, before they are kept.
    chargeFuel(fuel);
    stagedBytes+=size;
    if(stagedBytes>K.maxStagedBytes)return fail({name:K.errors.typeError,message:K.stagedTooLarge});
    staged.push(output.slice(input.at));
    return staged.length-1;
  };
  const failure=(output)=>{
    const value=decoder.decode(output),at=value.indexOf(K.separator);
    return {name:value.slice(0,at),message:value.slice(at+K.separator.length)};
  };
  const readOp=(ptr,len)=>{
    const descriptor=read(ptr,len,'invalid descriptor');
    const {status,output}=call(C.exports.parseRead,concat(table(),encoder.encode(descriptor)));
    if(status===C.statuses.failed)return fail(failure(output));
    staged=[];stagedBytes=0;
    const input=new In(output),id=input.u32(),count=input.u32(),ranges=[];
    for(let i=0;i<count;i++)ranges.push({start:input.bytes(),end:input.bytes(),limit:input.u32(),reverse:input.u8()===1,prefix:input.bytes()});
    const open=database(id);
    if(!granted(K.capabilities.read,open.path))return fail(denied(K.capabilities.read,open.path));
    // V38 fair fuel: the read, after the grant and before the storage.
    chargeFuel(K.fuel.operation);
    const outputs=storage.container.read(open.path,ranges);
    const request=new Out().u32(ranges.length);
    for(const range of ranges)request.bytes(range.prefix);
    const flat=outputs.flatMap((found,range)=>found.map(entry=>({range,entry})));
    request.u32(flat.length);
    for(const {range,entry} of flat)request.u32(range).bytes(entry.key).u32(entry.value.length).push(versionstamp(entry.version));
    const result=new In(call(C.exports.readEntries,request.done()).output);
    // The entries returned, before the guest sees their count.
    chargeFuel(result.u64());
    entries=flat.map(({range,entry})=>({range,key:entry.key,encoding:entry.encoding,value:entry.value,cursor:decoder.decode(result.bytes()),versionstamp:decoder.decode(result.bytes())}));
    return entries.length;
  };
  const commit=(ptr,len)=>{
    const descriptor=read(ptr,len,'invalid descriptor');
    const {status,output}=call(C.exports.parseCommit,concat(table(),encoder.encode(descriptor)),now());
    if(status===C.statuses.failed)return fail(failure(output));
    staged=[];stagedBytes=0;
    const input=new In(output),id=input.u32(),fuel=input.u64(),checks=[],mutations=[];
    for(let i=input.u32();i>0;i--){const key=input.bytes();checks.push({key,versionstamp:input.u8()===C.present?input.fixed(VERSIONSTAMP_BYTES):null});}
    for(let i=input.u32();i>0;i--){
      const kind=input.u8(),key=input.bytes(),mutation={kind,key};
      if(kind===K.mutations.set){mutation.encoding=input.u8();mutation.value=input.bytes();}
      else if(kind!==K.mutations.remove)mutation.number=input.u64();
      if(input.u8()===C.present)mutation.expiration=input.f64();
      mutations.push(mutation);
    }
    const open=database(id);
    if(!granted(K.capabilities.write,open.path))return fail(denied(K.capabilities.write,open.path));
    // V38 fair fuel: the commit and the bytes it writes, before it is applied.
    chargeFuel(fuel);
    if(!open.writable)return fail(typeError(FAILURES.readOnly(open.path)));
    const unusable=storage.container.unusable(open.path);
    if(unusable)return fail(unusable);
    if(performance.now()>=turnDeadline()-K.commitReserveMs)return fail(typeError(FAILURES.tooLate(open.path)));
    const outcome=storage.container.commit(open.path,checks,mutations);
    if(outcome.checkFailed){text='';return K.checkFailed;}
    if(outcome.error)return fail(outcome.error);
    text=Array.from(outcome.versionstamp,byte=>byte.toString(16).padStart(2,'0')).join('');
    return K.committed;
  };
  const entryAt=(index)=>{
    if(!Number.isInteger(index)||index<0||index>=entries.length)return undefined;
    return entries[index];
  };
  const entry=(index,field)=>{
    const found=entryAt(index);
    if(!found)throw fatal('entry index');
    const F=K.entryFields;
    if(!Number.isInteger(field)||field<0||!Object.values(F).includes(field))throw fatal('entry field');
    let value;
    if(field===F.versionstamp)value=found.versionstamp;
    else if(field===F.encoding)value=String(found.encoding);
    else if(field===F.u64){if(found.encoding!==K.encodings.u64)throw fatal('entry is not a KvU64');value=new DataView(found.value.buffer,found.value.byteOffset).getBigUint64(0,true).toString();}
    else if(field===F.bytes){if(found.encoding!==K.encodings.bytes)throw fatal('entry is not bytes');value=String.fromCharCode(...found.value);}
    else if(field===F.cursor)value=found.cursor;
    else value=String(found.range);
    return writeResult(value);
  };
  const decode=(key)=>(index,ptr,len)=>{
    const found=entryAt(index);
    const input=!found?Uint8Array.of(C.entries.none):key?concat(Uint8Array.of(C.entries.key),found.key):concat(Uint8Array.of(C.entries.value,found.encoding),found.value);
    const {output}=call(C.exports.decode,input,key?1:0,ptr,len);
    return BigInt.asIntN(64,new In(output).u64());
  };
  const close=(id)=>{if(Number.isInteger(id)&&id>=1)databases.delete(id);};
  return Object.freeze({
    [O.open]:open,[O.read]:readOp,[O.commit]:commit,[O.stage]:stage,
    [O.entry]:entry,[O.key]:decode(true),[O.value]:decode(false),
    [O.text]:()=>writeResult(text),[O.close]:close,
  });
}
function concat(a,b){const out=new Uint8Array(a.length+b.length);out.set(a,0);out.set(b,a.length);return out;}

/** The codec module's imports for one guest: guest memory, its allocation
 * and retention helpers, the exception channel, the turn deadline and the
 * fuel tank. Every guest range is checked here again. */
export function codecImports({memory,exports,codecMemory,deadlinePassed,chargeFuel,channel}){
  const guest=()=>new Uint8Array(memory().buffer);
  const own=()=>new Uint8Array(codecMemory().buffer);
  const inside=(bytes,at,len)=>Number.isInteger(at)&&Number.isInteger(len)&&at>=0&&len>=0&&at+len<=bytes.length;
  const I=C.imports;
  return {[C.importModule]:{
    [I.memorySize]:()=>guest().length,
    [I.memoryRead]:(at,into,len)=>{
      [at,into,len]=[at>>>0,into>>>0,len>>>0];
      const from=guest(),to=own();
      if(!inside(from,at,len)||!inside(to,into,len))return 0;
      to.set(from.subarray(at,at+len),into);return 1;
    },
    [I.memoryWrite]:(at,from,len)=>{
      [at,from,len]=[at>>>0,from>>>0,len>>>0];
      const to=guest(),source=own();
      if(!inside(to,at,len)||!inside(source,from,len))return 0;
      to.set(source.subarray(from,from+len),at);return 1;
    },
    [I.memoryZero]:(at,len)=>{
      [at,len]=[at>>>0,len>>>0];
      const to=guest();if(!inside(to,at,len))return 0;
      to.fill(0,at,at+len);return 1;
    },
    [I.allocate]:(len)=>exports()[K.guest.allocate](len|0)>>>0,
    [I.retain]:(handle)=>BigInt.asIntN(64,BigInt(exports()[K.guest.retain](handle))),
    [I.deadline]:()=>deadlinePassed()?1:0,
    [I.charge]:(fuel)=>chargeFuel(BigInt.asUintN(64,fuel)),
    [I.channelReady]:()=>channel.ready()?1:0,
    [I.channelThrow]:(cell)=>channel.raise(BigInt.asUintN(64,cell))?1:0,
    [I.resultLength]:(high)=>{const global=exports()[K.guest.resultLength];if(!(global instanceof WebAssembly.Global))return 0;global.value=high|0;return 1;},
  }};
}

/** Instantiate the compiled codec module `codec` for the guest module
 * `guest` and its instance (`memory`, `exports`), configured with the
 * guest's static region from its own custom sections, as native reads it. */
export function createCodec({codec,guest,memory,exports,deadlinePassed,chargeFuel,version}){
  let instance;
  instance=new WebAssembly.Instance(codec,codecImports({memory,exports,codecMemory:()=>instance[C.exports.memory],deadlinePassed,chargeFuel,
    channel:exceptionChannel({memory,exports,version})})).exports;
  const section=(name)=>{const [bytes]=WebAssembly.Module.customSections(guest,name);return bytes?new Uint8Array(bytes):new Uint8Array(0);};
  const input=new Out().bytes(section(C.sections.static)).bytes(section(C.sections.intrinsics)).done();
  new Uint8Array(instance[C.exports.memory].buffer,instance[C.exports.input](input.length),input.length).set(input);
  if(instance[C.exports.configure]()!==C.statuses.ok)throw new Error('kv: invalid static region');
  return instance;
}

/** The native `exceptions::ready` and the channel publication of a v38
 * module (exception channel `profile.exceptionVersion`). */
export function exceptionChannel({memory,exports,version}){
  const V=registry.values,global=(name)=>{const value=exports()[name];return value instanceof WebAssembly.Global?value:undefined;};
  return Object.freeze({
    ready(){
      const selector=global(version.global),pending=global(K.guest.pending),value=global(V.exceptionValue);
      if(!selector||selector.value!==version.value||!pending||pending.value!==0||!value||value.value!==0n)return false;
      const root=new Uint8Array(memory().buffer,V.exceptionRoot,V.exceptionRootBytes);
      return root.every(byte=>byte===0);
    },
    raise(cell){
      const value=global(V.exceptionValue),pending=global(K.guest.pending),length=global(K.guest.resultLength);
      if(!value||!pending||!length)return false;
      length.value=0;
      new DataView(memory().buffer).setBigUint64(V.exceptionRoot,cell,true);
      value.value=BigInt.asIntN(64,cell);pending.value=1;
      return true;
    },
  });
}
