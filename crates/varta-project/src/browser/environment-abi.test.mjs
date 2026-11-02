import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createEnvironmentAbi} from './environment-abi.mjs';
function fixture() {
  const mem=new WebAssembly.Memory({initial:2});const resultLength={value:99},resultStatus={value:99},charges=[],logs=[],reads=[];
  const read=name=>{reads.push(name);return name==='ALLOW'?{status:'ok',value:'Привіт 😀'}:name==='MISSING'?{status:'missing'}:{status:'denied'};};
  const abi=createEnvironmentAbi({memory:()=>mem,resultLength,resultStatus,maxInputBytes:65536,statuses:{ok:0,denied:1,invalid:2,io:3},access:{'env.get':{read},'env.has':{read:name=>name==='MISSING'?{status:'ok',value:false}:{status:'ok',value:true}}},chargeBytes:n=>charges.push(n),writeResult:text=>{resultLength.value=text.length*2;return 65536;},emit:text=>logs.push(text)});
  const put=text=>{const view=new DataView(mem.buffer);for(let i=0;i<text.length;i++)view.setUint16(32+i*2,text.charCodeAt(i),true);return text.length*2;};
  return{abi,put,resultLength,resultStatus,charges,logs,reads,mem};
}
test('UTF-16 imports reset state, charge input and return checked results',()=>{
  const f=fixture();assert.equal(f.abi.env_get16(32,f.put('ALLOW')),65536);assert.equal(f.resultStatus.value,0);assert.equal(f.resultLength.value,'Привіт 😀'.length*2);assert.deepEqual(f.charges,[10]);
  assert.equal(f.abi.env_has16(32,f.put('MISSING')),0);assert.equal(f.resultStatus.value,0);assert.equal(f.resultLength.value,0);
});
test('invalid inputs fail before authority lookup, denial and missing use distinct statuses',()=>{
  const f=fixture();assert.equal(f.abi.env_get16(32,1),0);assert.equal(f.resultStatus.value,2);assert.deepEqual(f.reads,[]);assert.deepEqual(f.charges,[1]);
  assert.equal(f.abi.env_get16(32,f.put('SECRET')),0);assert.equal(f.resultStatus.value,1);assert.equal(f.resultLength.value,0);
  assert.equal(f.abi.env_get16(32,f.put('MISSING')),0);assert.equal(f.resultStatus.value,3);assert(f.logs.at(-1).includes('environment variable not found'));
  assert.equal(f.abi.env_get16(-1,2),0);assert.equal(f.resultStatus.value,2);assert.equal(f.reads.length,2);
  assert.equal(f.abi.env_get16(32,f.put('\ud800')),0);assert.equal(f.resultStatus.value,2);assert.equal(f.reads.length,2);
  assert.equal(f.abi.env_get16(32,f.put('BAD\0NAME')),0);assert.equal(f.resultStatus.value,2);assert.equal(f.reads.length,2);
});
test('result memory growth failure becomes an IO status without exposing stale bytes',()=>{
  const mem=new WebAssembly.Memory({initial:1,maximum:1}),length={value:7},status={value:0},logs=[];
  const abi=createEnvironmentAbi({memory:()=>mem,resultLength:length,resultStatus:status,access:{'env.get':{read:()=>({status:'ok',value:'x'})}},chargeBytes:()=>{},writeResult:(_,fail)=>fail(1),emit:value=>logs.push(value)});
  assert.equal(abi.env_get16(0,0),0);assert.equal(length.value,0);assert.equal(status.value,3);
  assert.deepEqual(logs,['<host I/O error: failed to grow memory by `1`>']);
});
