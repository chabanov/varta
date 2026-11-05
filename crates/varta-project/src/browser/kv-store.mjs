import {registry} from '../../../varta-common/browser/registry.mjs';
// The official browser host's v38 KV imports (`kv_*`; docs/embedding.md,
// KV storage (v38)). The browser host offers no KV storage yet: the imports
// are synchronous host calls and a commit may be reported only once it is
// durable, which no asynchronous browser store (IndexedDB) can give a
// running guest call. It therefore implements the contract of a host without
// KV storage and touches no storage at all:
//
// - `kv_open` reads the path exactly as the native host does (a fatal
//   failure for an invalid range, an odd length, a lone surrogate, a NUL or
//   an oversized path; its UTF-16 bytes are charged first), lets the broker
//   audit the denied `kv.read` operation, logs the denial and leaves
//   `NotAllowedError\n<capability denied: kv.read> (<path>)` in `kv_text`:
//   the denial of a host without a grant.
// - No database is ever open, so no database id, staged value or read entry
//   exists: `kv_read`, `kv_commit`, `kv_stage`, `kv_entry`, `kv_key` and
//   `kv_value` are fatal, as the native host is for an unknown id, stage id
//   or entry. The engine prelude reaches none of them without an open
//   database.
// - `kv_text` returns the last failure text through the shared result
//   writer; `kv_close` of an id that was never opened does nothing.
// The bytes of one UTF-16 code unit: a path's byte length is a multiple.
const UTF16_UNIT_BYTES=Uint16Array.BYTES_PER_ELEMENT;
export function createKvHost({memory, broker, emit, writeResult, chargeBytes}) {
  const K=registry.kv, encoder=new TextEncoder();
  let text='';
  const fatal=(what)=>new Error(`kv: ${what}`);
  // The native descriptor reader (`host::input` with the UTF-16 resource
  // rules): range and length first, then the byte charge, then the text.
  const path=(ptr,len)=>{
    if(!Number.isInteger(ptr)||ptr<0||!Number.isInteger(len)||len<0||len>K.maxDescriptorBytes)throw fatal('invalid database path');
    chargeBytes(len);
    const buffer=memory().buffer;
    if(len%UTF16_UNIT_BYTES||ptr+len>buffer.byteLength)throw fatal('invalid database path');
    let name;
    try{name=new TextDecoder('utf-16le',{fatal:true,ignoreBOM:true}).decode(new Uint8Array(buffer,ptr,len));}
    catch{throw fatal('invalid database path');}
    if(name.includes('\0')||encoder.encode(name).length>K.maxDescriptorBytes)throw fatal('invalid database path');
    return name;
  };
  const open=(ptr,len)=>{
    const name=path(ptr,len);
    broker.operation(K.capabilities.read,name,false);
    // A function replacement: a `$` in the path stays literal.
    const denial=K.denialText.replace(K.pathPlaceholder,()=>name);
    emit(denial);
    text=K.notAllowed+K.separator+denial;
    return K.failed;
  };
  const closed=()=>{throw fatal('no KV database is open');};
  const noEntry=()=>{throw fatal('entry index');};
  const O=K.operations;
  return Object.freeze({
    [O.open]:open,
    [O.read]:closed,[O.commit]:closed,[O.stage]:closed,
    [O.entry]:noEntry,[O.key]:noEntry,[O.value]:noEntry,
    [O.text]:()=>writeResult(text),
    [O.close]:()=>{},
  });
}
