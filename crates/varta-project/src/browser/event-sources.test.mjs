import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';
import {registry} from '../../../varta-common/browser/registry.mjs';
import {createEventSources,snapshotHostEvent} from './event-sources.mjs';
const context=vm.createContext({registry});vm.runInContext(await readFile(new URL('scheduler.js',import.meta.url),'utf8')+'\nglobalThis.scheduler=createScheduler;',context);
const profile=registry.profiles.find(p=>p.name==='checked-f64-utf16-v38'),E=registry.permissions.events;
const setup=()=>{const scheduler=context.scheduler(profile,()=>''),events=createEventSources({scheduler,readText:text=>text,writeResult:text=>text});return{scheduler,events,ops:events.implementations};};
test('ready permission answers wake once, release their source and expose owned text once',()=>{
  const {scheduler,events,ops}=setup(),id=events.answer('granted'),serial=scheduler.implementations.promise_react(0,1);
  ops.event_watch(serial,id);assert.equal(scheduler.reactionCount(serial),-1);assert.equal(ops.event_take(id),E.event);assert.equal(events.size,0);assert.equal(ops.event_text(id),'granted');assert.throws(()=>ops.event_text(id));assert.throws(()=>ops.event_take(id));assert(events.answer('denied')>id);
});
test('source capacity, names, close and timer serial ownership follow native rules',()=>{
  const {scheduler,events,ops}=setup();assert.equal(ops.event_source_open('9bad'),E.invalid);
  const id=ops.event_source_open('ui.click');assert.equal(ops.event_source_open('ui.click'),E.invalid);
  const serial=scheduler.implementations.promise_react(0,1);scheduler.setTimerOwner(()=>true);assert.throws(()=>ops.event_watch(serial,id),/serial/);scheduler.setTimerOwner(()=>false);ops.event_watch(serial,id);assert.throws(()=>ops.event_watch(serial,id));
  ops.event_source_close(id);assert.equal(ops.event_take(id),E.closed);assert.equal(ops.event_text(id),'');
  for(let i=0;i<E.sources;i++)events.answer('denied');assert.throws(()=>events.answer('granted'),/queue/);events.close();assert.equal(events.size,0);
});
test('prompt answers enqueue FIFO tasks without waking or running guest continuations early',()=>{
  const {scheduler,events,ops}=setup(),a=events.answer(undefined),b=events.answer(undefined),seen=[];
  ops.event_watch(scheduler.implementations.promise_react(0,1n),a);ops.event_watch(scheduler.implementations.promise_react(0,2n),b);
  const drain=()=>scheduler.drain({exports:{[registry.scheduler.resumeExport](token){seen.push(token);return 1;}}},()=>{});
  events.complete(a,'granted',10);events.complete(b,'denied',11);assert.equal(events.arrival(),10);drain();assert.deepEqual(seen,[]);
  assert.throws(()=>events.complete(a,'denied',12));assert.equal(events.deliver(),true);drain();assert.deepEqual(seen,[1n]);assert.equal(ops.event_take(a),E.event);assert.equal(ops.event_text(a),'granted');
  assert.equal(events.arrival(),11);events.deliver();drain();assert.deepEqual(seen,[1n,2n]);ops.event_take(b);assert.equal(ops.event_text(b),'denied');assert.equal(events.deliver(),false);
  assert.equal(events.complete(a,'denied',12),false);assert.throws(()=>events.complete(b,'granted',NaN));events.close();
});
test('named events preserve FIFO, coalesce only marked records and retain original arrival/gesture',()=>{
  const{events,ops}=setup(),id=ops.event_source_open('ui');
  assert.equal(events.push({source:'ui',record:'a'},id,1),'queued');
  assert.equal(events.push({source:'ui',record:'old',coalesce:true},id,2),'queued');
  assert.equal(events.push({source:'ui',record:'new',coalesce:true,gesture:true},id,3),'coalesced');
  assert.equal(events.push({source:'ui',record:'b'},id,4),'queued');
  const seen=[];while(events.arrival()!==undefined){const at=events.arrival();events.deliver();ops.event_take(id);seen.push([ops.event_text(id),at,events.gesture]);}
  assert.deepEqual(seen,[['a',1,false],['new',2,true],['b',4,false]]);
});
test('closed source generations discard queued records and reject deliveries after reopen',()=>{
  const{events,ops}=setup(),old=ops.event_source_open('ui');events.push({source:'ui',record:'stale'},old,1);ops.event_source_close(old);ops.event_take(old);
  const current=ops.event_source_open('ui');assert(current>old);assert.throws(()=>events.push({source:'ui',record:'late'},old,2),/no open host/);assert.equal(events.arrival(),undefined);
  assert.deepEqual(events.catalog(),[{name:'ui',id:current}]);events.push({source:'ui',record:'live'},current,3);events.deliver();ops.event_take(current);assert.equal(ops.event_text(current),'live');
});
test('queue capacity drops only coalescible overflow and allows replacement at capacity',()=>{
  const{events,ops}=setup(),id=ops.event_source_open('ui'),other=ops.event_source_open('other');
  events.push({source:'ui',record:'first',coalesce:true},id,0);
  for(let i=1;i<E.tasks;i++)events.push({source:'ui',record:'x'},id,i);
  assert.equal(events.push({source:'ui',record:'replacement',coalesce:true},id,E.tasks),'coalesced');
  assert.equal(events.push({source:'other',record:'drop',coalesce:true},other,E.tasks),'dropped');assert.throws(()=>events.push({source:'ui',record:'fatal'},id,E.tasks),/queue/);
  events.deliver();ops.event_take(id);assert.equal(ops.event_text(id),'replacement');
});
test('host records are valid bounded UTF-8 snapshots and listener errors do not hide size failures',()=>{
  const input={source:'ui',record:'\0\uFEFF😀'},snapshot=snapshotHostEvent(input);input.record='mutated';assert.equal(snapshot.record,'\0\uFEFF😀');assert(Object.isFrozen(snapshot));
  for(const invalid of [{source:'ui',record:'\uD800'},{source:'ui',record:'x',gesture:1}])assert.throws(()=>snapshotHostEvent(invalid));
  const{events}=setup();assert.throws(()=>events.push({source:'missing',record:'😀'.repeat(E.recordBytes)},undefined,1),/exceeds/);assert.throws(()=>snapshotHostEvent({source:'missing',record:'😀'.repeat(E.recordBytes)}),/exceeds/);
  assert.throws(()=>events.push({source:'missing',record:'x'},undefined,1),/no open host/);
});
