// Real HTTPS browser/Wasmtime acceptance. All keys are synthetic; temporary
// certificates are trusted only inside the isolated Chrome probe through CDP.
import assert from 'node:assert/strict';
import {mkdtemp,readFile,writeFile,mkdir,rm} from 'node:fs/promises';
import {spawn,execFileSync} from 'node:child_process';
import {createServer as httpsServer} from 'node:https';
import {createServer as tcpServer} from 'node:net';
import {tmpdir} from 'node:os';
import {join,resolve,extname} from 'node:path';
import {once} from 'node:events';
import {withBrowserPage} from './browser-probe.mjs';
import {registry} from '../crates/varta-common/browser/registry.mjs';
const executable=resolve(process.argv[2]??'target/debug/varta');
const root=await mkdtemp(join(tmpdir(),'varta-session-'));
let server,web;
const run=(args)=>execFileSync(executable,args,{encoding:'utf8',stdio:['ignore','pipe','pipe']});
try {
 const certConfig=join(root,'openssl.cnf');await writeFile(certConfig,'[req]\ndistinguished_name=dn\nx509_extensions=ext\nprompt=no\n[dn]\nCN=127.0.0.1\n[ext]\nsubjectAltName=IP:127.0.0.1\n');
 execFileSync('openssl',['req','-x509','-newkey','rsa:2048','-nodes','-days','1','-keyout',join(root,'key.pem'),'-out',join(root,'cert.pem'),'-config',certConfig],{stdio:'ignore'});
 const credentials={key:await readFile(join(root,'key.pem')),cert:await readFile(join(root,'cert.pem'))};
 let webFolder,redirectHits=0;
 web=httpsServer(credentials,async(req,res)=>{
  try {
   const path=new URL(req.url,'https://127.0.0.1').pathname;
   if(path==='/trap'){redirectHits++;res.writeHead(200);res.end('unexpected');return;}
   if(path==='/blank.html'){res.writeHead(200,{'content-type':'text/html'});res.end('<!doctype html><title>session acceptance</title>');return;}
   if(!path.startsWith('/web/')||path.includes('..'))throw new Error('not found');
   const file=await readFile(join(webFolder,path.slice('/web/'.length)));
   res.writeHead(200,{'content-type':{'.wasm':'application/wasm','.js':'application/javascript','.mjs':'application/javascript','.json':'application/json'}[extname(path)]??'application/octet-stream'});res.end(file);
  }catch{res.writeHead(404);res.end();}
 });web.listen(0,'127.0.0.1');await once(web,'listening');
 const studio=`https://127.0.0.1:${web.address().port}`;
 const reservation=tcpServer();reservation.listen(0,'127.0.0.1');await once(reservation,'listening');const port=reservation.address().port;await new Promise(done=>reservation.close(done));
 const api=`https://127.0.0.1:${port}`;
 const source=await readFile(new URL('../crates/varta-runtime/src/server_tests/sessions.rs',import.meta.url),'utf8');
 let handler=/const SOURCE: &str = r#"([\s\S]*?)"#;/.exec(source)[1].replace("port:0",`port:${port}`);
 handler=handler.replace(" let secret:string|null=null;try{secret=varta.http.session.get();}catch(error){return {status:403,body:'channel-required',headers:[]};}",` if(request.target==='/redirect'){return {status:302,body:'',headers:[{name:'location',value:'${studio}/trap'}]};}\n let secret:string|null=null;try{secret=varta.http.session.get();}catch(error){return {status:403,body:'channel-required',headers:[]};}`);
 await mkdir(join(root,'server'));await writeFile(join(root,'server','main.ts'),handler);
 const grants={session:{read:true,write:true,clear:true},net:{listen:[api]}};
 const serverConfig=join(root,'server','varta.config.json');await writeFile(serverConfig,JSON.stringify({entry:'main.ts',target:'native',out:'dist/server',languageProfile:'checked-f64-utf16-v40',capabilities:grants}));
 const policy=join(root,'policy.json');await writeFile(policy,JSON.stringify({schemaVersion:1,abiVersion:36,capabilities:[{id:'net.listen',patterns:[api]},...['read','write','clear'].map(op=>({id:'session.'+op,enabled:true}))],http:{tls:{certificate:join(root,'cert.pem'),key:join(root,'key.pem')},session:{origin:api,clientOrigin:studio,maxAgeSeconds:3600,idleSeconds:900,maxSessions:4,sameSite:'strict'}}}));
 let serverOutput='';server=spawn(executable,['dev','--config',serverConfig,'--policy',policy],{stdio:['ignore','pipe','pipe']});server.stdout.on('data',v=>serverOutput+=v);server.stderr.on('data',v=>serverOutput+=v);
 const until=Date.now()+90000;while(!serverOutput.includes('varta listening:')){if(server.exitCode!==null)throw new Error(serverOutput);assert(Date.now()<until,'server readiness');await new Promise(r=>setTimeout(r,25));}
 await mkdir(join(root,'client'));
 const client=`export async function main():Promise<void>{
 const api='${api}';
 const created=await varta.session.fetch(api+'/set',{method:'POST'});console.log('created',created.status);
 const first=await varta.session.fetch(api+'/read');console.log('read',first.status);const text=await first.text();if(text.includes('cookie')||text.includes('private-key')||text.includes('x-varta-session'))throw new Error('credential leaked');
 const unread=await varta.session.fetch(api+'/read');varta.session.reset();
 let bodyAborted=false;try{await unread.text();}catch(error){bodyAborted=error instanceof DOMException&&error.name==='AbortError';}console.log('idle-body-aborted',bodyAborted);
 for(let i=0;i<${registry.network.maxPending+1};i++) {
  const unused=await varta.session.fetch(api+'/read');varta.session.reset();const body=unused.body;if(body!==null)await body.cancel();
 }
 console.log('reset-cancel-slots',true);
 const broken=new ReadableStream<Uint8Array>({start:(controller:ReadableStreamDefaultController<Uint8Array>):void=>{controller.error(undefined);}});
 let preserved=false;try{await broken.getReader().read();}catch(error){preserved=error===undefined;}console.log('undefined-error-preserved',preserved);
 console.log('ordinary',(await fetch(api+'/read')).status);
 console.log('failed-set',(await varta.session.fetch(api+'/bad-set',{method:'POST'})).status);
 console.log('still-valid',(await varta.session.fetch(api+'/read')).status);
 let redirected=false;try{await varta.session.fetch(api+'/redirect');}catch(error){redirected=error instanceof TypeError;}console.log('redirect-refused',redirected);
 let headerDenied=false;try{await varta.session.fetch(api+'/read',{headers:{Authorization:'Bearer guest-owned'}});}catch(error){headerDenied=error instanceof DOMException&&error.name==='NotAllowedError';}console.log('header-denied',headerDenied);
 varta.session.reset();console.log('logout',(await varta.session.fetch(api+'/clear',{method:'POST'})).status);
 console.log('anonymous',(await varta.session.fetch(api+'/read')).status);
}`;
 await writeFile(join(root,'client','main.ts'),client);
 const clientConfig=join(root,'client','varta.config.json');await writeFile(clientConfig,JSON.stringify({entry:'main.ts',target:'wasm',out:'dist/app.wasm',languageProfile:'checked-f64-utf16-v40',capabilities:{net:{request:{urls:[api+'/**'],methods:['GET']},session:{urls:[api+'/**'],methods:['GET','POST']}}}}));
 run(['build','--target','browser','--config',clientConfig]);webFolder=join(root,'client','dist','web');
 const binding={apiOrigin:api,clientOrigin:studio,allowInsecureLoopback:false};const ceiling={'net.request':{patterns:[api+'/**'],methods:['GET']},'net.session':{patterns:[api+'/**'],methods:['GET','POST']}};
 await withBrowserPage('about:blank',async call=>{
  await call('Security.setIgnoreCertificateErrors',{ignore:true});await call('Page.navigate',{url:studio+'/blank.html'});
  const evaluate=async expression=>{
   const answer=await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});if(answer.exceptionDetails)throw new Error(JSON.stringify(answer.exceptionDetails));return answer.result.value;
  };
  for(let i=0;i<100;i++){if(await evaluate('location.origin')===studio)break;await new Promise(r=>setTimeout(r,20));}
  const result=await evaluate(`(async()=>{const host=await import('${studio}/web/varta-host.js');const result=await host.run({sessionBinding:${JSON.stringify(binding)},permissionCeiling:${JSON.stringify(ceiling)}});return {...result,cookies:document.cookie};})()`);
  assert.equal(result.status,'ok',JSON.stringify(result));
  const lines=result.logs.filter(v=>!v.includes('capability denied'));assert.deepEqual(lines,[
   'varta log: created 201','varta log: read 200','varta log: idle-body-aborted true','varta log: reset-cancel-slots true','varta log: undefined-error-preserved true','varta log: ordinary 403','varta log: failed-set 500','varta log: still-valid 200','varta log: redirect-refused true','varta log: header-denied true','varta log: logout 200','varta log: anonymous 401',
  ]);assert.equal(result.cookies,'');assert.equal(redirectHits,0);assert(!result.logs.some(v=>v.includes('synthetic-private-key')));
  const before=(serverOutput.match(/varta HTTP request /g)??[]).length;
  for(const options of [{sessionBinding:{...binding,clientOrigin:'https://preview.test'},permissionCeiling:ceiling},{permissionCeiling:ceiling},{sessionBinding:binding,permissionCeiling:{'net.request':ceiling['net.request']}}]){
   const denied=await evaluate(`(async()=>{const host=await import('${studio}/web/varta-host.js');try{const result=await host.run(${JSON.stringify(options)});return result.status==='failed';}catch(error){return true;}})()`);assert.equal(denied,true);
  }
  assert.equal((serverOutput.match(/varta HTTP request /g)??[]).length,before);
 });
 console.log('PASS protected sessions: real HTTPS, HttpOnly cookie, cross-origin CORS/preflight, isolated vault, ordinary-fetch omission, failed rotation, redirect/header refusal, logout, binding/ceiling denial and no late transport effects');
}finally{
 if(server&&server.exitCode===null){server.kill('SIGTERM');await once(server,'exit');}
 if(web){web.closeAllConnections();await new Promise(done=>web.close(done));}
 await rm(root,{recursive:true,force:true});
}
