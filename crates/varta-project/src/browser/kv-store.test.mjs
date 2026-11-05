import {test} from 'node:test';
import assert from 'node:assert/strict';
import {registry} from '../../../varta-common/browser/registry.mjs';
import {createKvHost} from './kv-store.mjs';

const K=registry.kv, O=K.operations;
// Where the fixture writes guest text: past the zero page, inside memory.
const TEXT_BASE=32;
const UTF16_BYTES=Uint16Array.BYTES_PER_ELEMENT;

function fixture() {
  const mem=new WebAssembly.Memory({initial:2}),charges=[],logs=[],audit=[],results=[];
  const host=createKvHost({memory:()=>mem,
    broker:{operation:(id,resource,covered)=>{audit.push([id,resource,covered]);return false;}},
    emit:text=>logs.push(text),writeResult:text=>{results.push(text);return TEXT_BASE;},chargeBytes:n=>charges.push(n)});
  const put=text=>{const view=new DataView(mem.buffer);for(let i=0;i<text.length;i++)view.setUint16(TEXT_BASE+i*UTF16_BYTES,text.charCodeAt(i),true);return text.length*UTF16_BYTES;};
  return{host,put,mem,charges,logs,audit,results};
}
const denial=path=>K.denialText.replace(K.pathPlaceholder,()=>path);

test('every open is denied like a host without a grant: audited, logged and recoverable',()=>{
  const f=fixture();
  for(const path of ['/data/app.kv','/data/$&.kv','/data/Привіт 😀.kv']){
    assert.equal(f.host[O.open](TEXT_BASE,f.put(path)),K.failed);
    assert.deepEqual(f.audit.at(-1),[K.capabilities.read,path,false]);
    assert.equal(f.logs.at(-1),denial(path));
    assert.equal(f.host[O.text](),TEXT_BASE);
    assert.equal(f.results.at(-1),`${K.notAllowed}${K.separator}<capability denied: ${K.capabilities.read}> (${path})`);
  }
  assert.deepEqual(f.charges,['/data/app.kv','/data/$&.kv','/data/Привіт 😀.kv'].map(path=>path.length*UTF16_BYTES));
  // A text before any failure is empty, like native's initial state.
  const fresh=fixture();fresh.host[O.text]();assert.deepEqual(fresh.results,['']);
});

test('an invalid path is fatal before the broker decides or anything is logged',()=>{
  const f=fixture();
  // Each case writes its text when it runs: the cases share one buffer.
  const invalid=[()=>[-1,2],()=>[TEXT_BASE,-2],()=>[TEXT_BASE,1],()=>[TEXT_BASE,K.maxDescriptorBytes+UTF16_BYTES],()=>[f.mem.buffer.byteLength,UTF16_BYTES],
    ()=>[TEXT_BASE,f.put('\ud800')],()=>[TEXT_BASE,f.put('/data/a\0.kv')],()=>[0.5,2]];
  for(const input of invalid){const [ptr,len]=input();assert.throws(()=>f.host[O.open](ptr,len),/^Error: kv: invalid database path$/,`${ptr} ${len}`);}
  // A path whose UTF-8 form is above the limit, although its UTF-16 bytes fit.
  const wide='ࠀ'.repeat(K.maxDescriptorBytes/UTF16_BYTES);
  assert.throws(()=>f.host[O.open](TEXT_BASE,f.put(wide)),/invalid database path/);
  assert.deepEqual(f.audit,[]);assert.deepEqual(f.logs,[]);
  // The UTF-16 bytes are charged once the range and the limit are known to
  // hold, before the text is checked (native order).
  assert.deepEqual(f.charges,[1,UTF16_BYTES,UTF16_BYTES,UTF16_BYTES*'/data/a\0.kv'.length,K.maxDescriptorBytes]);
});

test('without an open database every id, stage and entry import is fatal and close does nothing',()=>{
  const f=fixture();
  for(const name of [O.read,O.commit])assert.throws(()=>f.host[name](TEXT_BASE,0),/no KV database is open/);
  assert.throws(()=>f.host[O.stage](0,0,0,0),/no KV database is open/);
  assert.throws(()=>f.host[O.entry](0,0),/entry index/);
  for(const name of [O.key,O.value])assert.throws(()=>f.host[name](0,TEXT_BASE,0),/entry index/);
  assert.equal(f.host[O.close](1),undefined);
  assert.deepEqual([f.audit,f.logs,f.charges,f.results],[[],[],[],[]]);
  assert.deepEqual(Object.keys(f.host).sort(),Object.values(O).sort());
});
