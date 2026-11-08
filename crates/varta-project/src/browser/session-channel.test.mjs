import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';
import {snapshotSessionBinding,createSessionChannel} from './session-channel.mjs';
import {registry} from '../../../varta-common/browser/registry.mjs';
const bytes=new Uint8Array(await readFile(new URL('session-policy.wasm',import.meta.url)));
const binding={apiOrigin:'https://api.test',clientOrigin:'https://studio.test'};
const options={bytes,binding,pageOrigin:binding.clientOrigin,explicitCeiling:true};
test('protected session Rust policy binds exact API and page origins and refuses guest credentials',async()=>{
 const channel=await createSessionChannel(options);
 const plan=(extra={})=>({url:'https://api.test/api/me',method:'GET',headers:[],...extra});
 assert(channel.allows(plan()));
 for(const url of ['https://api.test.evil/api/me','https://user@api.test/api/me','https://api.test:444/api/me','http://api.test/api/me','https://api.test/api/me#token']) assert(!channel.allows(plan({url})),url);
 for(const name of ['Cookie','Authorization','Origin','X-Varta-Session','Sec-Fetch-Site'])assert(!channel.allows(plan({headers:[[name,'hidden']]})),name);
 assert(channel.allows(plan({method:'POST',headers:[['Content-Type','application/json'],['Idempotency-Key','public-key']]})));
 for(const extra of [{pageOrigin:'https://preview.test'},{binding:{...binding,apiOrigin:'http://api.test'}},{binding:{...binding,apiOrigin:'https://api.test:443'}}])await assert.rejects(createSessionChannel({...options,...extra}),/binding/);
 assert.equal((await createSessionChannel({...options,explicitCeiling:false})).allows(plan()),false);
 assert.equal((await createSessionChannel({...options,binding:undefined})).allows(plan()),false);
 const original={...binding};const copy=snapshotSessionBinding(original);original.apiOrigin='https://evil.test';assert.equal(copy.apiOrigin,binding.apiOrigin);
});
test('protected session grants cannot borrow net.request authority and reset discards late session replies only',async()=>{
 const source=(await Promise.all(['scheduler.js','network-policy.js','network.js'].map(name=>readFile(new URL(name,import.meta.url),'utf8')))).join('\n');
 const context=vm.createContext({registry,URL,TextEncoder,Uint8Array,setTimeout,clearTimeout});vm.runInContext(source+'\nglobalThis.make=createNetwork;globalThis.policy=networkPolicy;globalThis.scheduler=createScheduler;',context);
 const grant=id=>({id,patterns:['https://api.test/api/**'],methods:['GET']});const manifest={capabilities:[grant('net.session'),grant('net.request')]};
 const scope={patterns:['https://api.test/api/**'],methods:['GET']};const ceiling={'net.request':scope,'net.session':scope};
 const plain=context.policy(manifest,ceiling);const session=context.policy(manifest,ceiling,true,undefined,'net.session');
 assert.equal(context.policy({capabilities:[grant('net.request')]},ceiling,true,undefined,'net.session').allows('https://api.test/api/me','GET'),false);
 const channel=await createSessionChannel(options),starts=[],resolvers=[],cancelled=[];
 const scheduler=context.scheduler(registry.profiles.at(-1),String);
 const network=context.make(scheduler,{...plain,session},()=>{},s=>s,s=>s,plan=>{const index=starts.push(plan)-1;return{head:new Promise(resolve=>resolvers[index]=resolve),cancel(){cancelled.push(index);},pull:async()=>({kind:registry.network.completion.end,text:''})};},channel);
 const descriptor=[registry.network.version,'GET','https://api.test/api/me','0','0',''].join('\n');
 const protectedId=network.implementations.session_request_start(descriptor,0);const ordinaryId=network.implementations.net_request_start_v2(descriptor,0);network.pump();assert.equal(starts.length,2);
 network.implementations.session_reset();assert.deepEqual(cancelled,[0]);
 resolvers[0]({kind:registry.network.completion.head,text:'old account'});resolvers[1]({kind:registry.network.completion.head,text:'ordinary'});await Promise.resolve();
 assert.equal(network.deliver(),true);assert.equal(network.implementations.net_request_take(protectedId),registry.network.completion.aborted);assert.equal(network.deliver(),true);assert.equal(network.implementations.net_request_take(ordinaryId),registry.network.completion.head);assert.equal(network.implementations.net_request_text(ordinaryId),'ordinary');assert.equal(network.deliver(),false);
 // A reset between Response head and body must reject through the completion
 // protocol, rather than trap when the guest later asks to read that body.
 const idle=network.implementations.session_request_start(descriptor,0);network.pump();
 resolvers[2]({kind:registry.network.completion.head,text:'head'});await Promise.resolve();network.deliver();
 assert.equal(network.implementations.net_request_take(idle),registry.network.completion.head);
 network.implementations.session_reset();assert.doesNotThrow(()=>network.implementations.net_body_read(idle));
 assert.equal(network.implementations.net_request_take(idle),registry.network.completion.aborted);
 assert.equal(network.deliver(),false);
 // Reset can retain an aborted body completion for a future read. A guest
 // cancellation of that body must instead release its bounded slot at once.
 for(let i=0;i<registry.network.maxPending+1;i++) {
  const at=starts.length,id=network.implementations.session_request_start(descriptor,0);network.pump();
  resolvers[at]({kind:registry.network.completion.head,text:'head'});await Promise.resolve();assert.equal(network.deliver(),true);
  assert.equal(network.implementations.net_request_take(id),registry.network.completion.head);
  network.implementations.session_reset();network.implementations.net_request_abort(id);
 }
 network.close();
});
