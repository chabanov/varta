// Sustained protected-channel acceptance through both official Varta HTTPS hosts.
// A trusted local certificate is supplied by the operator. No trust bypass,
// provider calls, OTP or real credentials are used by this synthetic probe.
import assert from 'node:assert/strict';
import {mkdtemp,readFile,writeFile,mkdir,rm} from 'node:fs/promises';
import {spawn,execFileSync} from 'node:child_process';
import {createServer} from 'node:net';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {once} from 'node:events';
import {createHash} from 'node:crypto';
import {withBrowserPage} from './browser-probe.mjs';

const args=process.argv.slice(2);
const option=(name,fallback)=>{const at=args.indexOf(name);return at<0?fallback:args[at+1];};
const executable=resolve(option('--varta','target/debug/varta'));
const certificate=option('--certificate'),key=option('--key');
assert(certificate&&key,'--certificate and --key must name operator-trusted local TLS files');
const seconds=Number(option('--seconds','90')),cycles=Number(option('--cycles','256'));
assert(Number.isInteger(seconds)&&seconds>=1&&seconds<=300);
assert(Number.isInteger(cycles)&&cycles>=32&&cycles<=4096);
const MAX_LOG_BYTES=64*1024,SAMPLE_MS=5000,WARMUP_CYCLES=64;
const MAX_WARM_GROWTH_KIB=64*1024;
const root=await mkdtemp(join(tmpdir(),'varta-session-soak-'));
const children=[];const samples=[];let lastState,lastHosts=[];
async function port(){const server=createServer();server.listen(0,'127.0.0.1');await once(server,'listening');const result=server.address().port;await new Promise(done=>server.close(done));return result;}
function start(argv){
 const child=spawn(executable,argv,{stdio:['ignore','pipe','pipe']});children.push(child);let output='';
 const collect=data=>{output=(output+data.toString()).slice(-MAX_LOG_BYTES);};child.stdout.on('data',collect);child.stderr.on('data',collect);
 return {child,output:()=>output};
}
async function ready(process,marker){const until=Date.now()+90000;while(!process.output().includes(marker)){if(process.child.exitCode!==null)throw new Error(process.output());assert(Date.now()<until,'host readiness timeout');await new Promise(done=>setTimeout(done,25));}}
const save=(path,data)=>writeFile(path,JSON.stringify(data));
try {
 const api=`https://127.0.0.1:${await port()}`,studio=`https://127.0.0.1:${await port()}`;
 const tls={certificate:resolve(certificate),key:resolve(key)};
 await mkdir(join(root,'server'));await mkdir(join(root,'client'));
 const fixture=await readFile(new URL('../crates/varta-runtime/src/server_tests/sessions.rs',import.meta.url),'utf8');
 const match=/const SOURCE: &str = r#"([\s\S]*?)"#;/.exec(fixture);assert(match,'session fixture source boundary changed');
 let source=match[1].replace('port:0',`port:${new URL(api).port}`);
 assert(source.includes(' const valid=secret==='),'session fixture authentication boundary changed');
 source=source.replace(" const valid=secret===", " if(request.target==='/wait'){await new Promise<void>((resolve)=>{setTimeout(resolve,1000);});}\n const valid=secret===");
 await writeFile(join(root,'server/main.ts'),source);
 const serverConfig=join(root,'server/varta.config.json');await save(serverConfig,{entry:'main.ts',target:'native',out:'dist/server',languageProfile:'checked-f64-utf16-v40',capabilities:{session:{read:true,write:true,clear:true},time:{timer:true},net:{listen:[api]}}});
 const serverPolicy=join(root,'server-policy.json');await save(serverPolicy,{schemaVersion:1,abiVersion:36,capabilities:[{id:'net.listen',patterns:[api]},{id:'time.timer',enabled:true},...['read','write','clear'].map(op=>({id:'session.'+op,enabled:true}))],http:{tls,session:{origin:api,clientOrigin:studio,maxAgeSeconds:3600,idleSeconds:900,maxSessions:4,sameSite:'strict'}}});
 const code=`
async function check(path:string,method:string,status:number):Promise<void>{const response=await varta.session.fetch('${api}'+path,{method});if(response.status!==status)throw new Error('status mismatch');await response.text();}
export async function main():Promise<void>{
 const until=Date.now()+${seconds*1000};let cycle=0;
 while(cycle<${cycles}||Date.now()<until){
  await check('/set','POST',201);
  if(cycle%4===0){await check('/read','GET',200);}
  else if(cycle%4===1){const response=await varta.session.fetch('${api}/read');varta.session.reset();const body=response.body;if(body!==null)await body.cancel();}
  else if(cycle%4===2){const response=await varta.session.fetch('${api}/read');varta.session.reset();let aborted=false;try{await response.text();}catch(error){aborted=error instanceof DOMException&&error.name==='AbortError';}if(!aborted)throw new Error('body was not aborted');}
  else{const controller=new AbortController();const pending=varta.session.fetch('${api}/wait',{signal:controller.signal});await new Promise<void>((resolve)=>{setTimeout(resolve,25);});controller.abort();let aborted=false;try{await pending;}catch(error){aborted=error instanceof DOMException&&error.name==='AbortError';}if(!aborted)throw new Error('pending request was not aborted');varta.session.reset();}
  await check('/read','GET',200);
  if(cycle%32===0){const plain=await fetch('${api}/read');if(plain.status!==403)throw new Error('ordinary channel changed');await plain.text();console.log('soak-step',cycle);}
  await check('/clear','POST',200);await check('/read','GET',401);
  cycle+=1;await new Promise<void>((resolve)=>{setTimeout(resolve,20);});
 }
 console.log('soak-passed',cycle);
}`;
 await writeFile(join(root,'client/main.ts'),code);
 const clientConfig=join(root,'client/varta.config.json');await save(clientConfig,{entry:'main.ts',target:'wasm',out:'dist/app',languageProfile:'checked-f64-utf16-v40',capabilities:{time:{now:true,timer:true},net:{session:{urls:[api+'/**'],methods:['GET','POST']},request:{urls:[api+'/read'],methods:['GET']}}}});
 const clientPolicy=join(root,'client-policy.json');await save(clientPolicy,{schemaVersion:1,abiVersion:36,capabilities:[{id:'time.now',enabled:true},{id:'time.timer',enabled:true},{id:'net.session',patterns:[api+'/**'],methods:['GET','POST']},{id:'net.request',patterns:[api+'/read'],methods:['GET']}],http:{tls},browserSession:{apiOrigin:api,clientOrigin:studio}});
 // Pin synthetic test consent to these exact portable bytes, beneath the host ceiling.
 execFileSync(executable,['build','--config',clientConfig],{stdio:['ignore','pipe','pipe']});
 const identity='sha256:'+createHash('sha256').update(await readFile(join(root,'client/dist/app.wasm'))).digest('hex');
 const decisions=join(root,'decisions.json');await save(decisions,{schemaVersion:1,applications:[{artifact:identity,decisions:['time.now','time.timer','net.session','net.request'].map(id=>({id,decision:'allow'}))}]});
 const server=start(['dev','--config',serverConfig,'--policy',serverPolicy]);await ready(server,'varta listening:');
 const browser=start(['dev','--target','browser','--listen',new URL(studio).host,'--config',clientConfig,'--policy',clientPolicy,'--decisions',decisions,'--app']);await ready(browser,'varta dev: browser '+studio+'/');
 lastHosts=[server,browser];const started=Date.now();let final,lastSample=0;
 await withBrowserPage('about:blank',async call=>{
  await call('Page.addScriptToEvaluateOnNewDocument',{source:`globalThis.__soakErrors=[];addEventListener('error',e=>__soakErrors.push({message:e.message,source:e.filename,resource:e.target?.src}),true);addEventListener('unhandledrejection',e=>__soakErrors.push({rejection:String(e.reason)}));addEventListener('securitypolicyviolation',e=>__soakErrors.push({policy:e.violatedDirective,blocked:e.blockedURI}));`});
  const navigation=await call('Page.navigate',{url:studio+'/'});assert(!navigation.errorText,navigation.errorText);
  const until=Date.now()+seconds*1000+120000;
  while(true){
   const answer=await call('Runtime.evaluate',{expression:"({status:document.querySelector('#varta-output')?.dataset.status,output:document.querySelector('#varta-output')?.textContent,secure:isSecureContext,origin:location.origin,errors:globalThis.__soakErrors,scripts:[...document.scripts].map(s=>({src:s.src,type:s.type,dev:s.hasAttribute('data-varta-dev')})),resources:performance.getEntriesByType('resource').map(e=>({name:e.name,size:e.transferSize,duration:e.duration})),body:document.body?.innerText})",returnByValue:true});
   assert(!answer.exceptionDetails);const state=answer.result.value;lastState=state;if(state.origin!==studio){assert(Date.now()-started<15000,'HTTPS navigation did not complete');await new Promise(done=>setTimeout(done,100));continue;}assert(state.secure,'trusted HTTPS page required');
   assert.notEqual(state.status,'failed',state.output);
   if(Date.now()-lastSample>=SAMPLE_MS){
    lastSample=Date.now();const matches=[...state.output.matchAll(/soak-step (\d+)/g)];const cycle=matches.length?Number(matches.at(-1)[1]):0;
    const rss=Number(execFileSync('ps',['-o','rss=','-p',String(server.child.pid)],{encoding:'utf8'}).trim());samples.push({elapsedMs:Date.now()-started,cycle,nativeRssKiB:rss});
   }
   if(state.status==='ok'){assert.match(state.output,/soak-passed \d+/);final=Number(/soak-passed (\d+)/.exec(state.output)[1]);break;}
   assert(state.status||Date.now()-started<15000,'browser bootstrap did not report readiness');assert(Date.now()<until,'browser soak deadline');await new Promise(done=>setTimeout(done,250));
  }
 });
 const warm=samples.filter(s=>s.cycle>=WARMUP_CYCLES);assert(seconds<30||warm.length>=2,'enough warm native RSS samples required');
 const growth=warm.length>=2?warm.at(-1).nativeRssKiB-warm[0].nativeRssKiB:null;assert(growth===null||growth<=MAX_WARM_GROWTH_KIB,`warm native RSS grew by ${growth} KiB`);
 console.log(JSON.stringify({status:'passed',scope:'official Varta HTTPS hosts; isolated Chrome; synthetic session credentials only',cycles:final,minimumDurationSeconds:seconds,elapsedMs:Date.now()-started,nativeWarmGrowthKiB:growth,maxWarmGrowthKiB:MAX_WARM_GROWTH_KIB,samples,trustBypass:false,realProviderCalls:0},null,2));
}catch(error){
 console.error(JSON.stringify({status:'failed',message:error.message,state:lastState,samples,hosts:lastHosts.map(host=>host.output().slice(-8192))},null,2));throw error;
}finally{
 for(const child of children.reverse())if(child.exitCode===null&&child.signalCode===null){child.kill('SIGTERM');await once(child,'exit');}
 await rm(root,{recursive:true,force:true});
}
