// Exercise the actual CLI/local host. No production output or ambient OS effects.
import {mkdtemp,writeFile,readFile,rm,access} from 'node:fs/promises';
import {join,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {spawn} from 'node:child_process';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';
import {withBrowserPage} from './browser-probe.mjs';
const executable=resolve(process.argv[2]??'target/debug/varta'),root=await mkdtemp(join(tmpdir(),'varta-browser-dev-chrome-'));
const config={entry:'main.ts',target:'wasm',out:'dist/app',languageProfile:'checked-f64-utf16-v38',fs:{roots:{unused:'missing'}}};
let server;
async function start(args=[]){
  const child=spawn(executable,['dev','--target','browser',...args],{cwd:root,stdio:['ignore','pipe','pipe']});let stdout='',stderr='',failed;
  child.stdout.on('data',data=>stdout+=data);child.stderr.on('data',data=>stderr+=data);child.on('error',error=>failed=error);
  server={child,get stdout(){return stdout;},get stderr(){return stderr;}};
  for(let i=0;i<400;i++){
    if(failed)throw failed;const match=/varta dev: browser (http:\/\/[^\s]+)/.exec(stdout);if(match){server.url=match[1];const html=await(await fetch(server.url)).text();const path=/data-varta-dev src="([^"]+)"/.exec(html)?.[1];assert(path?.startsWith('/__varta/'));server.bundleUrl=new URL('./',new URL(path,server.url));return server;}
    if(child.exitCode!==null)throw new Error(`dev exited ${child.exitCode}: ${stdout}${stderr}`);await new Promise(done=>setTimeout(done,25));
  }
  throw new Error(`dev did not start: ${stdout}${stderr}`);
}
async function stop(){
  if(!server)return;const current=server;server=undefined;
  if(current.child.exitCode===null&&current.child.signalCode===null){current.child.kill('SIGINT');await Promise.race([new Promise(done=>current.child.once('exit',done)),new Promise(done=>setTimeout(done,3000))]);}
  if(current.child.exitCode===null&&current.child.signalCode===null){current.child.kill('SIGKILL');await new Promise(done=>current.child.once('exit',done));throw new Error('dev did not stop gracefully');}
  assert.equal(current.child.exitCode,0,current.stderr);assert(!current.stdout.includes('varta log:'),'guest must not execute on the server');
  if(current.url)await assert.rejects(fetch(current.url));
}
const evaluate=async(call,expression)=>{const result=await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});assert(!result.exceptionDetails,JSON.stringify(result.exceptionDetails));return result.result.value;};
const state=call=>evaluate(call,"(()=>{const output=document.querySelector('#varta-output');return{status:output?.dataset.status,text:output?.textContent,budgets:document.querySelector('#varta-budgets')?.textContent,usage:output?.dataset.usage?JSON.parse(output.dataset.usage):undefined};})()");
const wait=async(call,predicate)=>{for(let i=0;i<250;i++){const value=await state(call);if(predicate(value))return value;if(value.status==='failed')throw new Error(JSON.stringify(value));await new Promise(done=>setTimeout(done,20));}throw new Error(JSON.stringify(await state(call)));};
try{
  await writeFile(join(root,'varta.config.json'),JSON.stringify(config));await writeFile(join(root,'main.ts'),'export function main():void{console.log("browser dev");}');
  await start(['--budget-report']);const manifest=await(await fetch(new URL('app.manifest.json',server.bundleUrl))).json();
  const loader=await(await fetch(new URL('varta-host.js',server.bundleUrl))).text(),descriptor=JSON.parse(/^const descriptor = (.+);\n/.exec(loader)[1]);
  await withBrowserPage(server.url,async(call)=>{
    const value=await wait(call,v=>v.status==='ok');assert.equal(value.text,'varta log: browser dev');assert(value.budgets.includes('fuelUsed:'));assert(value.usage.fuelUsed>0);
    const validation=await evaluate(call,`(async()=>{const {run}=await import(document.querySelector('script[data-varta-dev]').src);const original=fetch;let calls=0;globalThis.fetch=(...args)=>{calls++;return original(...args);};try{await run({environmentInputs:[['bad=key','value']]});return {accepted:true,calls};}catch(error){return {accepted:false,calls,type:error.name};}finally{globalThis.fetch=original;}})()`);
    assert.deepEqual(validation,{accepted:false,calls:0,type:'TypeError'});
    const scopes=await evaluate(call,`(async()=>{const {run}=await import(document.querySelector('script[data-varta-dev]').src);const ok=await run({permissionCeiling:{'env.get':{patterns:['PUBLIC_*']}},environmentInputs:[['PUBLIC_X','value']]});try{await run({permissionCeiling:{'env.get':{patterns:[42]}}});return {status:ok.status,invalid:false};}catch(error){return {status:ok.status,invalid:error.name==='TypeError'};}})()`);
    assert.deepEqual(scopes,{status:'ok',invalid:true});
    const original=server.bundleUrl;
    await evaluate(call,'globalThis.vartaOldRealm=true');
    await writeFile(join(root,'main.ts'),'import {value} from "./missing.ts"; export function main():void{console.log(value);}');
    for(let i=0;;i++){if(await evaluate(call,'Boolean(document.querySelector("#varta-dev-errors")?.textContent)'))break;if(i>150)throw new Error('missing rebuild diagnostic');await new Promise(done=>setTimeout(done,30));}
    const failed=await(await fetch(new URL('/__varta/status',server.url))).json();assert.equal(failed.revision,original.pathname.split('/')[2]);assert(failed.error);
    await writeFile(join(root,'missing.ts'),'export const value:string="rebuilt from missing dependency";');
    await wait(call,v=>v.status==='ok'&&v.text==='varta log: rebuilt from missing dependency');
    assert.equal(await evaluate(call,'typeof globalThis.vartaOldRealm'),'undefined','reload must create a fresh page realm');
    assert.equal((await fetch(new URL('varta-host.js',original))).status,200,'one preceding generation remains');
    await writeFile(join(root,'main.ts'),'export function main():void{console.log("third generation");}');
    await wait(call,v=>v.status==='ok'&&v.text==='varta log: third generation');
    assert.equal((await fetch(new URL('varta-host.js',original))).status,404,'expired generation must not resolve to latest bytes');
    await writeFile(join(root,'varta.config.json'),'{');
    for(let i=0;;i++){if(await evaluate(call,'Boolean(document.querySelector("#varta-dev-errors")?.textContent)'))break;if(i>150)throw new Error('missing config diagnostic');await new Promise(done=>setTimeout(done,30));}
    await evaluate(call,'globalThis.vartaSameRealm=true');await writeFile(join(root,'varta.config.json'),JSON.stringify(config));
    for(let i=0;;i++){if(await evaluate(call,'!document.querySelector("#varta-dev-errors")'))break;if(i>150)throw new Error('recovery did not clear diagnostic');await new Promise(done=>setTimeout(done,30));}
    assert.equal(await evaluate(call,'globalThis.vartaSameRealm'),true,'identical rebuilt bytes clear error without reloading');
  });
  assert.equal((await fetch(new URL('main.ts',server.url))).status,404);await assert.rejects(access(join(root,'dist')));await stop();

  const timer='export function main():void{try{setTimeout(()=>{console.log("timer");},1);}catch(error){console.log((error as Error).name);}}';
  config.capabilities={time:{timer:true}};await writeFile(join(root,'varta.config.json'),JSON.stringify(config));await writeFile(join(root,'main.ts'),timer);
  const policy={schemaVersion:manifest.schemaVersion,abiVersion:manifest.abiVersion,policy:manifest.policy,capabilities:[],budgets:{memoryBytes:descriptor.budgets.memoryBytes/2}};
  await writeFile(join(root,'host-policy.json'),JSON.stringify(policy));await start(['--policy','host-policy.json']);
  await withBrowserPage(server.url,async(call)=>{const value=await wait(call,v=>v.status==='ok');assert(value.text.includes('NotAllowedError'));assert(!value.text.includes('varta log: timer'));});
  const boundedLoader=await(await fetch(new URL('varta-host.js',server.bundleUrl))).text(),bounded=JSON.parse(/^const descriptor = (.+);\n/.exec(boundedLoader)[1]);assert.equal(bounded.budgets.memoryBytes,policy.budgets.memoryBytes);await stop();

  config.permissions={'time.timer':{required:false}};await writeFile(join(root,'varta.config.json'),JSON.stringify(config));await start();const bytes=await(await fetch(new URL('app.wasm',server.bundleUrl))).arrayBuffer(),artifact='sha256:'+createHash('sha256').update(new Uint8Array(bytes)).digest('hex');await stop();
  const other='sha256:'+'b'.repeat(64);await writeFile(join(root,'choices.json'),JSON.stringify({applications:[{artifact,decisions:[{id:'time.timer',decision:'allow'}]},{artifact:other,decisions:[{id:'time.timer',decision:'deny'}]}]}));for(const application of [false,true]){
    await start(['--decisions','choices.json',...(application?['--app']:[])]);
    await withBrowserPage(server.url,async(call)=>{const value=await wait(call,v=>v.status==='ok');assert.equal(value.text,'varta log: timer');});
    const selectedLoader=await(await fetch(new URL('varta-host.js',server.bundleUrl))).text();assert(!selectedLoader.includes(other),'other application choices must not leave the CLI host');await stop();
  }

  config.capabilities={};config.permissions={};await writeFile(join(root,'varta.config.json'),JSON.stringify(config));await writeFile(join(root,'main.ts'),'export function main():void{const root=View.node();View.createElement(root,1);const button=View.node();View.createElement(button,20);const text=View.node();View.createText(text,"0");View.insert(button,text,0);View.insert(root,button,0);let count=0;View.on(button,1,(event:ViewEvent):void=>{count++;View.setText(text,String(count));console.log(count);},0,0,0,0);View.mount(root);}');await start();
  await withBrowserPage(server.url,async(call)=>{await wait(call,v=>v.status==='ready');const{root:documentRoot}=await call('DOM.getDocument',{depth:-1,pierce:true});const find=node=>node.nodeName==='BUTTON'?node:[...(node.children??[]),...(node.shadowRoots??[])].map(find).find(Boolean);const button=find(documentRoot);assert(button);const{model}=await call('DOM.getBoxModel',{nodeId:button.nodeId}),q=model.content,x=(q[0]+q[2])/2,y=(q[1]+q[5])/2;await call('Input.dispatchMouseEvent',{type:'mousePressed',x,y,button:'left',clickCount:1});await call('Input.dispatchMouseEvent',{type:'mouseReleased',x,y,button:'left',clickCount:1});const value=await wait(call,v=>v.usage?.turns===2);assert.equal(value.text,'varta log: 1');
    await evaluate(call,`(()=>{const terminate=Worker.prototype.terminate;Worker.prototype.terminate=function(...args){sessionStorage.setItem('vartaWorkerStops',String(Number(sessionStorage.getItem('vartaWorkerStops')??0)+1));return terminate.apply(this,args);};})()`);
    await writeFile(join(root,'main.ts'),'export function main():void{console.log("view replaced");}');
    for(let i=0;;i++){let current;try{current=await state(call);}catch(error){if(!/context|navigat/i.test(error.message))throw error;}if(current?.status==='ok'&&current.text==='varta log: view replaced')break;if(i>250)throw new Error('live view was not replaced');await new Promise(done=>setTimeout(done,20));}
    assert.equal(await evaluate(call,`sessionStorage.getItem('vartaWorkerStops')`),'1','old live Worker must terminate before replacement');
  });await stop();
  await assert.rejects(access(join(root,'dist')));console.log('Browser dev CLI passed: real console/view execution, CSP/SRI, runtime policy and private memory ceiling, scoped decisions, no native roots/output/source serving, live rebuild/reload, missing-dependency/config recovery, retained generations and graceful stop');
}finally{if(server)await stop();await rm(root,{recursive:true,force:true});}
