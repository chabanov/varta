import {test} from 'node:test';
import assert from 'node:assert/strict';
import {registry} from '../../../varta-common/browser/registry.mjs';
import {snapshotDecisionFile,createPermissionBroker,parsePermissionDescriptor} from './permission-broker.mjs';
const P=registry.permissions,artifact=P.artifactPrefix+'a'.repeat(P.artifactHex),other=P.artifactPrefix+'b'.repeat(P.artifactHex);
const file=(decision,identity=artifact)=>({applications:[{artifact:identity,decisions:[{id:'time.timer',decision}]}]});
const descriptor=(name,resource)=>[P.descriptor.version,name,resource===undefined?P.descriptor.none:P.descriptor.resource,resource??''].join(P.descriptor.separator);
const broker=(options={})=>createPermissionBroker({manifest:{capabilities:[{id:'time.timer',enabled:true,...options.grant}]},artifact,clock:()=>0,...options});
test('all decision identities and records validate before a frozen snapshot is used',()=>{
  const input=file('allow'),copy=snapshotDecisionFile(input);input.applications[0].decisions[0].decision='deny';assert.equal(copy.applications[0].decisions[0].decision,'allow');assert(Object.isFrozen(copy.applications[0].decisions));
  for(const invalid of [null,{}, {...file('allow'),schemaVersion:null},{...file('allow'),schemaVersion:2},file('yes'),file('allow',artifact.toUpperCase()),{applications:[...file('allow').applications,...file('deny').applications]}, {applications:[{artifact:other,decisions:[{id:'time.fake',decision:'allow'}]}]}, {applications:[{artifact,decisions:[{id:'time.timer',decision:'allow'},{id:'time.timer',decision:'deny'}]}]}])assert.throws(()=>snapshotDecisionFile(invalid));
});
test('optional/required decisions match native ordering without widening ceilings',()=>{
  assert.equal(broker().query(descriptor('time.timer')),P.states.granted);
  assert.equal(broker({grant:{required:false}}).query(descriptor('time.timer')),P.states.denied);
  assert.equal(broker({grant:{required:false},decisions:file('allow')}).query(descriptor('time.timer')),P.states.granted);
  assert.equal(broker({grant:{required:false},decisions:file('allow',other)}).query(descriptor('time.timer')),P.states.denied);
  assert.throws(()=>broker({decisions:file('deny')}),/stored decision/);
  const denied=broker({ceiling:{},decisions:file('deny')});assert.equal(denied.query(descriptor('time.timer')),P.states.denied);
  assert.equal(broker({ceiling:{},decisions:file('allow')}).operation('time.timer','',true),false);
  const allowed=broker({decisions:file('allow'),ceiling:{'time.timer':true}});assert.equal(allowed.operation('time.timer','',true),true);
  assert.equal(allowed.snapshot().audit.at(-1).source,'stored');
});
test('descriptors distinguish unknown names, boolean resources and malformed structure',()=>{
  const b=broker();assert.equal(b.query(descriptor('time.fake')),P.descriptor.invalid);assert.equal(b.query(descriptor('time.timer','')),P.descriptor.invalid);
  assert.equal(b.query(descriptor('clipboard.write')),P.states.denied);
  assert.deepEqual(parsePermissionDescriptor(descriptor('net.request','https://a/\nrest')),{name:'net.request',resource:'https://a/\nrest'});
  for(const text of ['bad',descriptor('time.timer').replace(P.descriptor.version,'bad'),[P.descriptor.version,'time.timer',P.descriptor.none,'extra'].join(P.descriptor.separator)])assert.throws(()=>b.query(text));
});
test('concrete scopes, audit truncation/retention and snapshots stay bounded and owned',()=>{
  const b=createPermissionBroker({manifest:{capabilities:[{id:'net.request',patterns:['https://a/public/**']}]},artifact,clock:()=>1,covers:(_,value)=>value==='https://a/public/x'});
  assert.equal(b.query(descriptor('net.request','https://a/public/x')),P.states.granted);assert.equal(b.query(descriptor('net.request','https://a/private/x')),P.states.denied);
  const resource='\uFEFF'+'😀'.repeat(200);for(let i=0;i<P.limits.audit+5;i++)b.operation('net.request',resource,false);
  const snapshot=b.snapshot();assert.equal(snapshot.audit.length,P.limits.audit);assert.equal(snapshot.auditDropped,6);assert(snapshot.audit.at(-1).resource.startsWith('\uFEFF'));assert(Buffer.byteLength(snapshot.audit.at(-1).resource)<=P.limits.auditResourceBytes);assert(snapshot.audit.at(-1).resource.isWellFormed());
  snapshot.audit[0].outcome='granted';assert.equal(b.snapshot().audit[0].outcome,'denied');
});
test('browser platform states only narrow queries and operations; changes remain audited',()=>{
  const b=createPermissionBroker({manifest:{capabilities:[{id:'clipboard.write',enabled:true}]},artifact,platform:{'clipboard.write':'prompt'},clock:()=>0});
  assert.equal(b.query(descriptor('clipboard.write')),P.states.prompt);
  assert.equal(b.setPlatform('clipboard.write','denied'),true);assert.equal(b.query(descriptor('clipboard.write')),P.states.denied);assert.equal(b.operation('clipboard.write','',true),false);
  assert.equal(b.setPlatform('clipboard.write','granted'),true);assert.equal(b.query(descriptor('clipboard.write')),P.states.granted);assert.equal(b.setPlatform('clipboard.write','granted'),false);
  assert.equal(b.snapshot().audit.at(-1).outcome,'granted');assert.throws(()=>b.setPlatform('clipboard.write','invalid'));
  const optional=createPermissionBroker({manifest:{capabilities:[{id:'clipboard.write',enabled:true,required:false}]},artifact,platform:{'clipboard.write':'granted'},clock:()=>0});assert.equal(optional.query(descriptor('clipboard.write')),P.states.denied);
});
test('decision snapshots require own JSON fields and ignore inherited optional version',()=>{
  assert.throws(()=>snapshotDecisionFile(Object.create({applications:[]})));
  assert.throws(()=>snapshotDecisionFile({applications:[Object.create({artifact,decisions:[]})]}));
  assert.throws(()=>snapshotDecisionFile({applications:[{artifact,decisions:[Object.create({id:'time.timer',decision:'allow'})]}]}));
  const value=Object.assign(Object.create({schemaVersion:999}),{applications:[]});assert.equal(snapshotDecisionFile(value).schemaVersion,P.schemaVersion);
});
test('revocation is stored/audited and only narrows requested grants including required ones',()=>{
  const b=broker({decisions:file('allow')});assert.equal(b.revoke('time.fake'),false);assert.equal(b.revoke('clipboard.write'),false);
  assert.equal(b.revoke('time.timer'),true);assert.equal(b.query(descriptor('time.timer')),P.states.denied);assert.equal(b.operation('time.timer','',true),false);
  assert.equal(b.snapshot().decisions['time.timer'],'deny');assert.deepEqual(b.snapshot().audit.at(-2),{sequence:2,at_ms:0,source:'stored',operation:'time.timer',resource:'',outcome:'denied'});
  assert.equal(broker({v1:false}).revoke('time.timer'),false);
  const outside=broker({ceiling:{}});assert.equal(outside.revoke('time.timer'),true);assert.equal(outside.query(descriptor('time.timer')),P.states.denied);
});
test('delayed Worker observations preserve the original decision without restoring authority',()=>{
  const records=[],worker=broker({onRecord:record=>records.push(record)}),parent=broker();
  worker.operation('time.timer','',true);parent.revoke('time.timer');parent.recordOperation(records[0]);
  assert.equal(parent.snapshot().audit.at(-1).outcome,'granted');assert.equal(parent.snapshot().audit.at(-1).source,'policy');
  assert.equal(parent.query(descriptor('time.timer')),P.states.denied);assert.equal(parent.operation('time.timer','',true),false);
  for(const record of [{...records[0],covered:false},{...records[0],source:'prompt'},{...records[0],outcome:'prompt'},{...records[0],id:'time.fake'}])assert.throws(()=>parent.recordOperation(record));
  assert.throws(()=>broker({ceiling:{}}).recordOperation(records[0]));
});
test('optional prompting withholds effects; dismissals are bounded and stale answers cannot undo revocation',()=>{
  const b=broker({grant:{required:false,purpose:'Explain'},prompt:true});assert.equal(b.query(descriptor('time.timer')),P.states.prompt);assert.equal(b.operation('time.timer','',true),false);
  assert.deepEqual(b.promptInfo('time.timer'),{artifact,capability:'time.timer',scope:[],purpose:'Explain',required:false});
  for(let i=1;i<=P.limits.dismissals;i++){const result=b.resolve('time.timer','dismiss');assert.equal(result.state,i===P.limits.dismissals?'denied':'prompt');}
  assert.equal(b.snapshot().decisions['time.timer'],'deny');assert.equal(b.resolve('time.timer','allow').changed,false);
  const allowed=broker({grant:{required:false},prompt:true});assert.equal(allowed.resolve('time.timer','allow').state,'granted');assert.equal(allowed.operation('time.timer','',true),true);
  const revoked=broker({grant:{required:false},prompt:true});revoked.revoke('time.timer');assert.equal(revoked.resolve('time.timer','allow').state,'denied');assert.equal(revoked.snapshot().decisions['time.timer'],'deny');
  assert.throws(()=>revoked.resolve('time.timer','invalid'));assert.throws(()=>broker({prompt:true}),/before instantiation/);
});
test('first-run required choices are sequential before initialization and retain prompt audit provenance',async()=>{
  const asked=[],saved=[],observed=[];
  const b=broker({prompt:true,deferInit:true});assert.equal(b.granted('time.timer'),false);
  await b.initialize(async info=>{asked.push(info);assert.equal(b.granted('time.timer'),false);return'allow';},async(...args)=>saved.push(args),()=>observed.push(b.snapshot().audit.at(-1)));
  assert.equal(asked[0].required,true);assert.deepEqual(saved,[['time.timer','allow']]);assert.equal(b.granted('time.timer'),true);assert.equal(observed[0].source,'prompt');assert.equal(b.snapshot().decisions['time.timer'],'allow');
  await assert.rejects(b.initialize(()=>{}));
  for(const answer of ['deny','dismiss']){const denied=broker({prompt:true,deferInit:true});await assert.rejects(denied.initialize(async()=>answer),/refused at first run/);assert.equal(denied.snapshot().decisions['time.timer'],answer==='deny'?'deny':undefined);assert.equal(denied.snapshot().audit[0].source,'prompt');}
  const outside=broker({prompt:true,deferInit:true,ceiling:{},decisions:file('deny')});await outside.initialize(()=>{throw new Error('outside ceiling prompted');});assert.equal(outside.granted('time.timer'),false);
});
test('per-activation answers audit the full resource without storing a decision or changing state',()=>{
  const b=createPermissionBroker({manifest:{capabilities:[{id:'ui.open_url',patterns:[registry.view.capabilities.anyHttps]}]},artifact,clock:()=>0});
  for(const answer of ['allow','deny','dismiss','allow'])assert.equal(b.activation('ui.open_url','https://api.test/full?#',answer,true),answer==='allow');
  assert.deepEqual(b.snapshot().decisions,{});assert.equal(b.granted('ui.open_url'),true);assert(b.snapshot().audit.slice(1).every(row=>row.source==='prompt'&&row.resource==='https://api.test/full?#'));
  b.revoke('ui.open_url');assert.equal(b.activation('ui.open_url','https://api.test/full?#','allow',true),false);assert.equal(b.snapshot().decisions['ui.open_url'],'deny');
});
