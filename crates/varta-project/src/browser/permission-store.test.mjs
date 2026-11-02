import {test} from 'node:test';
import assert from 'node:assert/strict';
import {registry} from '../../../varta-common/browser/registry.mjs';
import {updateDecisionFile,createDecisionStore} from './permission-store.mjs';
const P=registry.permissions,artifact='sha256:'+'a'.repeat(64),other='sha256:'+'b'.repeat(64);
test('updates preserve other decisions/artifacts without aliases and replace one choice',()=>{
  const first=updateDecisionFile(undefined,artifact,'time.timer','allow');
  const second=updateDecisionFile(first,other,'clipboard.write','allow');
  const third=updateDecisionFile(second,artifact,'time.timer','deny');
  assert.equal(first.applications[0].decisions[0].decision,'allow');
  assert.deepEqual(third.applications,[{artifact,decisions:[{id:'time.timer',decision:'deny'}]},{artifact:other,decisions:[{id:'clipboard.write',decision:'allow'}]}]);
  assert(Object.isFrozen(third.applications[1].decisions[0]));
});
test('invalid choices and exhausted identities cannot silently evict existing choices',()=>{
  for(const [hash,id,decision] of [[artifact,'time.fake','allow'],[artifact,'time.timer','session'],['bad','time.timer','deny']])assert.throws(()=>updateDecisionFile(undefined,hash,id,decision));
  const full={applications:Array.from({length:P.limits.applications},(_,i)=>({artifact:'sha256:'+i.toString(16).padStart(64,'0'),decisions:[{id:'time.timer',decision:'allow'}]}))};
  assert.throws(()=>updateDecisionFile(full,artifact,'time.timer','deny'));
  assert.equal(updateDecisionFile(full,full.applications[0].artifact,'time.timer','deny').applications.length,P.limits.applications);
});
test('unavailable stores and aborts fail before a database can be opened',async()=>{
  assert.throws(()=>createDecisionStore({indexedDB:undefined}),/unavailable/);
  let opened=0;const store=createDecisionStore({indexedDB:{open(){opened++;throw new Error('open failed');}}});
  await assert.rejects(store.set(artifact,'time.fake','allow'),/invalid/);assert.equal(opened,0);
  const controller=new AbortController();controller.abort();await assert.rejects(store.read({signal:controller.signal}),{name:'AbortError'});assert.equal(opened,0);
  await assert.rejects(store.read(),/open failed/);assert.equal(opened,1);
});
