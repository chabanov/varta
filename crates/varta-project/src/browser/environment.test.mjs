import {test} from 'node:test';
import assert from 'node:assert/strict';
import {snapshotEnvironment,createEnvironmentPolicy,createEnvironmentAccess} from './environment.mjs';
import {createPermissionBroker} from './permission-broker.mjs';
import {registry} from '../../../varta-common/browser/registry.mjs';

test('environment values are explicit snapshots with Unicode and no ambient fallback', () => {
  const input = [['NAME','Привіт 😀'],['empty','']];
  const snapshot = snapshotEnvironment(input);
  input[0][1] = 'changed'; input.push(['OTHER','new']);
  assert.equal(snapshot.get('NAME'),'Привіт 😀');
  assert.equal(snapshot.get('empty'),''); assert(snapshot.has('empty'));
  assert.equal(snapshot.get('OTHER'),undefined); assert(!snapshot.has('PATH'));
  const copy=snapshot.entries();copy[0][1]='modified';assert.equal(snapshot.get('NAME'),'Привіт 😀');
  assert(Object.isFrozen(snapshot));
});

test('environment scopes intersect, match Unicode scalars and enumerate exact intersections only', () => {
  const requested=['APP_*','EXACT','😀?']; const ceiling=['APP_PUBLIC','EXACT','😀?'];
  const policy=createEnvironmentPolicy(requested,ceiling); requested.push('LATER'); ceiling.push('LATER');
  assert(policy.allows('APP_PUBLIC')); assert(!policy.allows('APP_SECRET'));
  assert(policy.allows('😀🦀')); assert(!policy.allows('LATER'));
  assert.deepEqual(policy.exactNames(),['EXACT']);
  assert(!createEnvironmentPolicy(['*'],[]).allows('ANY'));
  assert(createEnvironmentPolicy(['*']).allows('a/b'));
  assert(!policy.allows('BAD=NAME')); assert(!policy.allows('\ud800'));
  assert.deepEqual(createEnvironmentPolicy(['\u{10000}','\ue000','BAD=NAME','']).exactNames(),['\ue000','\u{10000}']);
  assert.throws(()=>createEnvironmentPolicy(['x'],null),TypeError);
});

test('broker audits each lookup and revocation prevents access before reading values', () => {
  const artifact=registry.permissions.artifactPrefix+'a'.repeat(registry.permissions.artifactHex);
  const records=[];
  const broker=createPermissionBroker({manifest:{capabilities:[{id:'env.get',patterns:['PUBLIC','SECRET']}]},ceiling:{'env.get':{patterns:['PUBLIC']}},artifact,v1:true,clock:()=>0,onRecord:r=>records.push(r)});
  const policy=createEnvironmentPolicy(['PUBLIC','SECRET'],['PUBLIC']);
  const snapshot=snapshotEnvironment([['PUBLIC','Привіт 😀'],['SECRET','private']]);
  const access=createEnvironmentAccess(snapshot,policy,broker,'env.get');
  assert.deepEqual(access.read('PUBLIC'),{status:'ok',value:'Привіт 😀'});
  assert.deepEqual(access.read('SECRET'),{status:'denied'});
  assert(broker.revoke('env.get'));
  assert.deepEqual(access.read('PUBLIC'),{status:'denied'});
  assert.deepEqual(records.map(r=>r.outcome),['granted','denied','denied']);
  const emptyBroker=createPermissionBroker({manifest:{capabilities:[{id:'env.get',patterns:['MISSING']}]},artifact,v1:true,clock:()=>0});
  assert.deepEqual(createEnvironmentAccess(snapshot,createEnvironmentPolicy(['MISSING']),emptyBroker,'env.get').read('MISSING'),{status:'missing'});
  const hasBroker=createPermissionBroker({manifest:{capabilities:[{id:'env.has',patterns:['MISSING']}]},artifact,v1:true,clock:()=>0});
  assert.deepEqual(createEnvironmentAccess(snapshot,createEnvironmentPolicy(['MISSING']),hasBroker,'env.has').read('MISSING'),{status:'ok',value:false});
});

test('environment rejects malformed, duplicate, invalid Unicode and oversized input', () => {
  for (const entries of [null, {}, [['','x']], [['a=b','x']], [['x','\0']], [['x','\ud800']], [['\udfff','x']], [['a','x'],['a','y']], Array.from({length:257},(_,i)=>[String(i),'x']), [['x','a'.repeat(524289)]], [['é'.repeat(129),'x']]]) {
    assert.throws(()=>snapshotEnvironment(entries),TypeError);
  }
});
