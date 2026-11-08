// Repeated account resets must release every completed/cancelled transfer,
// discard old replies and leave ordinary traffic alive.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';
import {registry} from '../../../varta-common/browser/registry.mjs';
const source=(await Promise.all(['scheduler.js','network.js'].map(name=>readFile(new URL(name,import.meta.url),'utf8')))).join('\n');
const C=registry.network.completion;
const CYCLES=4096;
const SCENARIOS=6;
const descriptor=[registry.network.version,'GET','https://api.test/read','0',registry.network.noBody,''].join(registry.network.separator);
function deferred() {let resolve;const promise=new Promise(done=>resolve=done);return {promise,resolve};}

test('session soak reuses slots across deferred, head, body, late reply and repeated reset phases',async()=>{
  const context=vm.createContext({registry,URL,TextEncoder,Uint8Array,setTimeout,clearTimeout});
  vm.runInContext(source+'\nglobalThis.make=createNetwork;globalThis.scheduler=createScheduler;',context);
  const policy={method:()=>true,allows:()=>true,visibleHeaders:()=>new Set()};
  const active=new Set();let current,started=0;
  const network=context.make(context.scheduler(registry.profiles.at(-1),String),{...policy,session:policy},()=>{},value=>value,value=>value,()=>{
    const head=deferred(),body=deferred(),number=++started;active.add(number);
    current={head,body};return {head:head.promise,pull:()=>body.promise,cancel(){active.delete(number);}};
  },{allows:()=>true});
  try {
    const ordinary=network.implementations.net_request_start_v2(descriptor,0);network.pump();const normal=current;
    const counts=Array(SCENARIOS).fill(0);
    for(let cycle=0;cycle<CYCLES;cycle++) {
      const scenario=cycle%SCENARIOS;counts[scenario]++;
      const before=started,id=network.implementations.session_request_start(descriptor,0);
      if(scenario===0) {
        network.implementations.session_reset();network.pump();assert.equal(started,before);
      } else {
        network.pump();const transfer=current;
        if(scenario===1) {
          network.implementations.session_reset();
          transfer.head.resolve({kind:C.head,text:'obsolete account'});await Promise.resolve();
        } else {
          transfer.head.resolve({kind:C.head,text:'head'});await Promise.resolve();assert.equal(network.deliver(),true);
          assert.equal(network.implementations.net_request_take(id),C.head);
          if(scenario===2) {
            network.implementations.session_reset();network.implementations.net_request_abort(id);
            assert.equal(active.size,1);continue;
          }
          if(scenario===3) {
            network.implementations.session_reset();network.implementations.net_body_read(id);
            assert.equal(network.implementations.net_request_take(id),C.aborted);
            assert.equal(active.size,1);continue;
          }
          network.implementations.net_body_read(id);network.pump();
          if(scenario===4) {
            transfer.body.resolve({kind:C.chunk,text:'obsolete body'});await Promise.resolve();
          }
          network.implementations.session_reset();network.implementations.session_reset();
          if(scenario===5) {transfer.body.resolve({kind:C.chunk,text:'late body'});await Promise.resolve();}
        }
      }
      assert.equal(network.deliver(),true);
      assert.equal(network.implementations.net_request_take(id),C.aborted);
      assert.equal(network.implementations.net_request_text(id),'');
      assert.equal(network.deliver(),false);
      assert.equal(active.size,1);
    }
    assert(counts.every(count=>count>registry.network.maxPending));
    normal.head.resolve({kind:C.head,text:'ordinary traffic'});await Promise.resolve();assert.equal(network.deliver(),true);
    assert.equal(network.implementations.net_request_take(ordinary),C.head);
    assert.equal(network.implementations.net_request_text(ordinary),'ordinary traffic');
    network.implementations.net_request_abort(ordinary);assert.equal(active.size,0);
  } finally {network.close();}
  assert.equal(active.size,0);
});
