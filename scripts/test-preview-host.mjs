// Development-only native host + owned Chrome acceptance. No product JS backend.
import assert from "node:assert/strict";
import {mkdtemp,writeFile,readFile,readdir,rm,mkdir} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join,resolve} from "node:path";
import {spawn} from "node:child_process";
import {createHash,randomBytes} from "node:crypto";
import {createServer} from "node:http";
import {withBrowserPage} from "./browser-probe.mjs";

const executable=resolve(process.argv[2]??"target/debug/varta");
const output=resolve(process.argv[3]??"target/preview-browser");
const root=await mkdtemp(join(tmpdir(),"varta-preview-chrome-"));
const secret=randomBytes(32).toString("hex"), secrets=[secret];
const delay=ms=>new Promise(r=>setTimeout(r,ms));
let host,control,publicOrigin,hits=0;
const studio=createServer((_q,r)=>{hits++;r.writeHead(200);r.end("privileged fixture");});
await new Promise(r=>studio.listen(0,"127.0.0.1",r));
const studioOrigin="http://127.0.0.1:"+studio.address().port;
const alive=c=>c&&c.exitCode===null&&c.signalCode===null;
async function stop(child){
  if(!alive(child))return;
  child.kill("SIGTERM");
  for(let i=0;i<200&&alive(child);i++)await delay(20);
  if(alive(child)){child.kill("SIGKILL");await new Promise(r=>child.once("exit",r));throw new Error("owned process did not stop gracefully");}
}
async function command(args,cwd){
  const c=spawn(executable,args,{cwd,env:{PATH:process.env.PATH??""},stdio:["ignore","pipe","pipe"]});
  let text="";c.stdout.on("data",b=>text+=b);c.stderr.on("data",b=>text+=b);
  const code=await new Promise((r,j)=>{c.on("error",j);c.on("exit",r);});
  assert.equal(code,0,text);
}
async function api(method,path,body={},key){
  const headers={authorization:"Bearer "+secret,"content-type":"application/json"};
  if(key)headers["idempotency-key"]=key;
  const response=await fetch(control+path,{method,headers,body:method==="GET"?undefined:JSON.stringify(body)});
  const value=await response.json();assert(response.ok,JSON.stringify({status:response.status,value}));return value;
}
async function build(source,name="main.tsx"){
  const dir=await mkdtemp(join(root,"project-"));
  await writeFile(join(dir,name),source);
  await writeFile(join(dir,"varta.config.json"),JSON.stringify({entry:name,target:"wasm",out:"dist/app",languageProfile:"checked-f64-utf16-v40",capabilities:{}}));
  await command(["build","--target","browser"],dir);
  const files=[];
  for(const path of (await readdir(join(dir,"dist/web"))).sort()){
    const bytes=await readFile(join(dir,"dist/web",path));files.push({path,bytes,sha256:createHash("sha256").update(bytes).digest("hex")});
  }
  return files;
}
async function upload(files,info,key){
  const h=createHash("sha256").update("varta-toolchain-artifact-v1\0");
  const le=n=>{const b=Buffer.alloc(8);b.writeBigUInt64LE(BigInt(n));return b;};
  for(const f of files){h.update(le(Buffer.byteLength(f.path))).update(f.path).update(le(f.bytes.length)).update(f.bytes);}
  const artifactSha256=h.digest("hex");
  const prepare=await api("POST","/v1/bundles",{schemaVersion:1,hostSha256:info.hostSha256,artifactSha256,files:files.map(f=>({path:f.path,sha256:f.sha256,bytes:f.bytes.length}))},key);
  for(let f=0;f<files.length;f++)for(let at=0;at<files[f].bytes.length;at+=8192){
    const response=await fetch(control+`/v1/bundles/${prepare.id}/files/${f}/chunks/${at/8192}`,{method:"PUT",headers:{authorization:"Bearer "+secret,"content-type":"application/octet-stream"},body:files[f].bytes.subarray(at,at+8192)});
    assert.equal(response.status,204);
  }
  await api("POST",`/v1/bundles/${prepare.id}/seal`);
  for(let i=0;;i++){const status=await api("GET",`/v1/bundles/${prepare.id}`);if(status.state==="ready")return artifactSha256;assert.equal(status.state,"sealing");assert(i<500);await delay(20);}
}
async function run(info,bundleSha256,key,ttlSeconds=120){
  return api("POST","/v1/runs",{schemaVersion:1,hostSha256:info.hostSha256,bundleSha256,target:"browser",policyId:"deny-all-v1",ttlSeconds},key);
}
async function evaluate(call,expression){
  const value=await call("Runtime.evaluate",{expression,awaitPromise:true,returnByValue:true});
  assert(!value.exceptionDetails,"browser evaluation failed");return value.result.value;
}
async function until(call,predicate){
  let last;
  for(let i=0;i<300;i++){const value=await evaluate(call,"({state:document.querySelector('#varta-output')?.dataset.previewState,text:document.querySelector('#varta-output')?.textContent,path:location.pathname,fragment:location.hash,secure:isSecureContext})");last=value;if(predicate(value))return value;await delay(20);}
  throw new Error("preview browser state timeout: "+JSON.stringify(last));
}
async function launch(call,id){
  const ticket=await api("POST",`/v1/runs/${id}/tickets`);secrets.push(ticket.launchUrl.split("#")[1]);
  await call("Page.navigate",{url:ticket.launchUrl});
}
try{
  await mkdir(output,{recursive:true});
  const ceiling={schemaVersion:1,controlListen:"127.0.0.1:0",publicListen:"[::1]:0",protectedOrigins:[studioOrigin],maxImports:4,maxRuns:4,maxRunSeconds:300,importSeconds:60,retentionSeconds:300,validationDeadlineMs:15000,cpuSeconds:10,addressSpaceBytes:4294967296,budgets:{}};
  await writeFile(join(root,"ceiling.json"),JSON.stringify(ceiling));
  host=spawn(executable,["preview-host","--ceiling",join(root,"ceiling.json")],{env:{VARTA_PREVIEW_TOKEN:secret},stdio:["ignore","pipe","pipe"]});
  let stdout="",stderr="";host.stdout.on("data",b=>stdout+=b);host.stderr.on("data",b=>stderr+=b);
  for(let i=0;;i++){const match=/varta preview: (http:\/\/\S+) public=(http:\/\/\S+) host=([a-f0-9]{64})/.exec(stdout);if(match){[,control,publicOrigin]=match;break;}assert(alive(host),stderr);assert(i<800,"preview readiness timeout");await delay(25);}
  const info=await api("GET","/v1/info");assert.equal(info.publicOrigin,publicOrigin);
  const files=await build(`let count=0;let label=0;
function increment(event:ViewEvent):void{count++;View.setText(label,String(count));View.commit();}
export async function main():Promise<void>{
 let denied=false;try{await fetch("${studioOrigin}/leak");}catch(error){denied=true;}if(!denied){throw new Error("unexpected authority");}
 console.log("preview main");View.setTitle("Preview isolated");label=View.node();View.createText(label,"0");
 const root=<main><h1>Preview isolated</h1><button id="increment" onClick={increment}>Increment</button></main>;
 View.insert(root,label,0);View.mount(root);
}`);
  const bundle=await upload(files,info,"browser-valid-bundle-01");
  const first=await run(info,bundle,"browser-valid-start-01");
  assert.equal(first.guestState,"pending");
  await withBrowserPage("about:blank",async call=>{
    await call("Network.enable");
    await call("Network.setCookie",{name:"studio_session_fixture",value:"never-preview",url:studioOrigin,httpOnly:true,sameSite:"Strict"});
    await call("Emulation.setDeviceMetricsOverride",{width:800,height:600,deviceScaleFactor:1,mobile:false});
    await launch(call,first.id);
    const ready=await until(call,v=>v.state==="ready");assert.equal(ready.fragment,"");assert(ready.secure);assert.equal(hits,0);
    assert.equal(await evaluate(call,"document.cookie"),"");
    const cookies=await call("Network.getCookies",{urls:[publicOrigin+`/runs/${first.id}/`]});
    assert(cookies.cookies.some(c=>c.name==="varta_preview"&&c.httpOnly));assert(!cookies.cookies.some(c=>c.name==="studio_session_fixture"));
    const tree=await call("Accessibility.getFullAXTree");const button=tree.nodes.find(n=>n.role?.value==="button"&&n.name?.value==="Increment");assert(button);
    const box=await call("DOM.getBoxModel",{backendNodeId:button.backendDOMNodeId});const q=box.model.border;const x=(q[0]+q[2]+q[4]+q[6])/4,y=(q[1]+q[3]+q[5]+q[7])/4;
    await call("Input.dispatchMouseEvent",{type:"mousePressed",x,y,button:"left",clickCount:1});
    await call("Input.dispatchMouseEvent",{type:"mouseReleased",x,y,button:"left",clickCount:1});
    for(let i=0;;i++){const ax=await call("Accessibility.getFullAXTree");if(ax.nodes.some(n=>n.role?.value==="StaticText"&&n.name?.value==="1"))break;assert(i<100,"click did not update");await delay(20);}
    const screenshot=await call("Page.captureScreenshot",{format:"png"});await writeFile(join(output,"preview.png"),Buffer.from(screenshot.data,"base64"));
    await api("POST",`/v1/runs/${first.id}/stop`);
    await until(call,v=>v.state==="stopped");
    const targets=await call("Target.getTargets");assert(!targets.targetInfos.some(t=>t.type==="worker"&&t.url.startsWith("blob:"+publicOrigin)),"worker outlived stop");
    assert.equal((await api("GET",`/v1/runs/${first.id}`)).state,"stopped");
  });
  const trap=await build("export function main():void{throw new Error('expected preview trap');}","main.ts");
  const trapBundle=await upload(trap,info,"browser-trap-bundle-001");
  const trapped=await run(info,trapBundle,"browser-trap-start-0001");
  await withBrowserPage("about:blank",async call=>{
    await launch(call,trapped.id);await until(call,v=>v.state==="stopped"||v.state==="failed");
    for(let i=0;;i++){const s=await api("GET",`/v1/runs/${trapped.id}`);if(s.state==="failed"){assert.equal(s.guestState,"failed");break;}assert(i<100);await delay(20);}
  });
  const report={passed:true,hostSha256:info.hostSha256,sourceExecutionInHost:false,scopes:["real TSX first frame","trusted click updates state","hostname/HttpOnly/Studio-cookie isolation","no privileged fetch","one-use fragment removed","stop closes Worker","guest trap reported"],privilegedRequests:hits,ownedChromeCleanup:true};
  await writeFile(join(output,"report.json"),JSON.stringify(report,null,2)+"\n");console.log(JSON.stringify(report));
}catch(error){
  let text=String(error.stack??error);for(const s of secrets)text=text.replaceAll(s,"[redacted]");console.error(text.replace(/[a-f0-9]{64}/g,"[digest-or-secret]"));process.exitCode=1;
}finally{
  await stop(host);await new Promise(r=>studio.close(r));await rm(root,{recursive:true,force:true});
}
