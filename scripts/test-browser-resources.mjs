// Verified resource bytes must precede DOM creation and every guest effect.
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { resolve, sep, join } from 'node:path';
import assert from 'node:assert/strict';
import { withBrowserPage } from './browser-probe.mjs';
const root=resolve(process.argv[2]),requests=[];
let tamper;
const server=createServer(async(request,response)=>{
  try{
    const name=new URL(request.url,'http://loopback').pathname;requests.push(name);
    const path=resolve(root,`.${name}`);if(!path.startsWith(root+sep))throw new Error('outside fixtures');
    let bytes=await readFile(path);
    if(name.endsWith('/varta-resource-0.png')&&tamper){
      if(tamper==='hash'){bytes=Buffer.from(bytes);bytes[42]^=1;}
      else bytes=Buffer.concat([bytes,Buffer.from([0])]);
    }
    response.setHeader('Cache-Control','no-store');response.setHeader('Content-Type',path.endsWith('.js')?'text/javascript':path.endsWith('.html')?'text/html':'application/octet-stream');response.end(bytes);
  }catch{response.writeHead(404).end();}
});
await new Promise(done=>server.listen(0,'127.0.0.1',done));
const base=`http://127.0.0.1:${server.address().port}`;
async function output(call,expected){
  let result;const end=Date.now()+10000;
  do{
    const response=await call('Runtime.evaluate',{expression:`(()=>{const e=document.querySelector('#varta-output');return{status:e.dataset.status,text:e.textContent,usage:e.dataset.usage?JSON.parse(e.dataset.usage):null};})()`,returnByValue:true});
    assert(!response.exceptionDetails,JSON.stringify(response.exceptionDetails));result=response.result.value;
    if(result.status!==expected)await new Promise(done=>setTimeout(done,20));
  }while(result.status!==expected&&Date.now()<end);
  assert.equal(result.status,expected,JSON.stringify(result));return result;
}
try{
  await withBrowserPage(`${base}/view-image/index.html`,async(call)=>{
    const result=await output(call,'ready'),golden=JSON.parse(await readFile(join(root,'view-image.golden.json'),'utf8'));
    assert.equal(result.text,golden.logs.join('\n'));assert.deepEqual(result.usage,golden.usage);
    const {root:document}=await call('DOM.getDocument',{depth:-1,pierce:true});
    const find=(node)=>node.nodeName==='IMG'?node:[...(node.children??[]),...(node.shadowRoots??[])].map(find).find(Boolean);
    const image=find(document);assert(image);const remote=await call('DOM.resolveNode',{nodeId:image.nodeId});
    const decoded=await call('Runtime.callFunctionOn',{objectId:remote.object.objectId,functionDeclaration:'async function(){await this.decode();const canvas=document.createElement("canvas");canvas.width=canvas.height=1;const context=canvas.getContext("2d");context.drawImage(this,0,0);return{src:this.src,width:this.naturalWidth,height:this.naturalHeight,pixel:[...context.getImageData(0,0,1,1).data]};}',awaitPromise:true,returnByValue:true});
    assert(!decoded.exceptionDetails,JSON.stringify(decoded.exceptionDetails));assert.match(decoded.result.value.src,/^blob:/);assert.equal(decoded.result.value.width,1);assert.equal(decoded.result.value.height,1);assert.deepEqual(decoded.result.value.pixel,[255,0,0,255]);
    const lifecycle=await call('Runtime.evaluate',{expression:`(async()=>{const{mount}=await import(${JSON.stringify(base+'/view-image/varta-host.js')});const create=URL.createObjectURL,revoke=URL.revokeObjectURL,created=[],revoked=[];URL.createObjectURL=function(blob){const url=create.call(this,blob);created.push(url);return url;};URL.revokeObjectURL=function(url){revoked.push(url);return revoke.call(this,url);};try{const container=document.createElement('div');document.body.append(container);const app=mount({container});await app.ready;app.close();await app.finished;return{created,revoked};}finally{URL.createObjectURL=create;URL.revokeObjectURL=revoke;}})()`,awaitPromise:true,returnByValue:true});
    assert(!lifecycle.exceptionDetails,JSON.stringify(lifecycle.exceptionDetails));assert.equal(lifecycle.result.value.created.length,2);assert.deepEqual(lifecycle.result.value.revoked.sort(),lifecycle.result.value.created.sort());
  });
  for(const kind of ['hash','length']){
    tamper=kind;
    await withBrowserPage(`${base}/view-image/index.html`,async(call)=>{
      const result=await output(call,'failed');assert(!result.text.includes('varta log:'));assert.match(result.text,kind==='hash'?/integrity mismatch/:/length mismatch/);
      const {root}=await call('DOM.getDocument',{depth:-1,pierce:true});const hasImage=(node)=>node.nodeName==='IMG'||[...(node.children??[]),...(node.shadowRoots??[])].some(hasImage);assert.equal(hasImage(root),false);
    });
  }
  tamper=undefined;
  for(const name of ['view-image-broken','view-image-missing'])await withBrowserPage(`${base}/${name}/index.html`,async(call)=>{
    const result=await output(call,'failed');assert(!result.text.includes('varta log: image ready'));assert(!result.text.includes('varta log: must not run'));
    if(name.endsWith('missing'))assert.match(result.text,/unpackaged view asset/);
    else assert(!result.text.includes('varta log:'),'invalid decoded content must fail before the first guest log');
  });
  assert(!requests.some(path=>path.includes('/icons/')),'logical resource names must never become request URLs');
  console.log('Verified PNG resources passed: native usage, real decoded pixels, owned Blob cleanup, tampering, decode failure and missing-asset rejection before guest effects');
}finally{await new Promise(done=>server.close(done));}
