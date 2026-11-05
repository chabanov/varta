// Real decoded faces, isolated simultaneous applications and deterministic widths.
import {createServer} from 'node:http';
import {readFile} from 'node:fs/promises';
import {resolve,join,sep} from 'node:path';
import assert from 'node:assert/strict';
import {withBrowserPage} from './browser-probe.mjs';
const root=resolve(process.argv[2]),requests=[];
let tamper;
const server=createServer(async(request,response)=>{
  try{
    const route=new URL(request.url,'http://loopback').pathname;requests.push(route);
    if(route==='/__embed'){
      response.setHeader('Content-Type','text/html');
      response.end(`<!doctype html><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; worker-src blob:; img-src blob:; trusted-types varta-worker 'allow-duplicates'; require-trusted-types-for 'script'"><title>Host</title>`);return;
    }
    const path=resolve(root,`.${route}`);if(!path.startsWith(root+sep))throw new Error('outside fixtures');
    let bytes=await readFile(path);
    if(tamper&&route.includes('/varta-resource-0.')){
      if(tamper==='hash'){bytes=Buffer.from(bytes);bytes[bytes.length-1]^=1;}
      else bytes=Buffer.concat([bytes,Buffer.from([0])]);
    }
    response.setHeader('Content-Type',path.endsWith('.js')?'text/javascript':path.endsWith('.html')?'text/html':'application/octet-stream');response.setHeader('Cache-Control','no-store');response.end(bytes);
  }catch{response.writeHead(404).end();}
});
await new Promise(done=>server.listen(0,'127.0.0.1',done));
const base=`http://127.0.0.1:${server.address().port}`;
async function inspect(call){
  const {root}=await call('DOM.getDocument',{depth:-1,pierce:true}),found=[];
  const visit=(node)=>{
    for(let i=0;i<(node.attributes?.length??0);i+=2)if(node.attributes[i]==='id'&&['direct','via','quoted'].includes(node.attributes[i+1]))found.push(node);
    for(const child of [...(node.children??[]),...(node.shadowRoots??[])])visit(child);
  };visit(root);
  const results=[];
  for(const node of found){
    const remote=await call('DOM.resolveNode',{nodeId:node.nodeId});
    const result=await call('Runtime.callFunctionOn',{objectId:remote.object.objectId,functionDeclaration:`function(){const style=getComputedStyle(this),canvas=document.createElement('canvas'),context=canvas.getContext('2d'),range=document.createRange();context.font=style.fontSize+' '+style.fontFamily;range.selectNodeContents(this);return{id:this.id,family:style.fontFamily,color:style.color,width:context.measureText('AAAA').width,layout:range.getBoundingClientRect().width};}`,returnByValue:true});
    assert(!result.exceptionDetails,JSON.stringify(result.exceptionDetails));results.push(result.result.value);
  }
  return results;
}
try{
  for(const format of ['ttf','otf','woff','woff2'])await withBrowserPage(`${base}/view-font-${format}/index.html`,async(call)=>{
    let result;const end=Date.now()+10000;
    do{const r=await call('Runtime.evaluate',{expression:`(()=>{const e=document.querySelector('#varta-output');return{status:e.dataset.status,text:e.textContent,usage:e.dataset.usage?JSON.parse(e.dataset.usage):null};})()`,returnByValue:true});result=r.result.value;if(result.status!=='ready')await new Promise(done=>setTimeout(done,20));}while(result.status!=='ready'&&Date.now()<end);
    assert.equal(result.status,'ready',JSON.stringify(result));const golden=JSON.parse(await readFile(join(root,`view-font-${format}.golden.json`),'utf8'));
    assert.equal(result.text,golden.logs.join('\n'));assert.deepEqual(result.usage,golden.usage);
    const nodes=await inspect(call);assert.equal(nodes.length,3);
    for(const node of nodes){assert.match(node.family,/varta_font_/);assert.equal(node.width,32);assert.equal(node.layout,32);}
    assert.equal(nodes.find(node=>node.id==='via').color,'rgb(255, 0, 0)');
  });
  // Packaged faces (configuration fonts): the bundle registers every face
  // with its weight and style, maps the monospace alias and makes the first
  // family the default, as varta-shell does. A synthesized bold or italic of
  // the regular face, or the browser's own monospace, would be 32px wide.
  await withBrowserPage(`${base}/view-font-packaged/index.html`,async(call)=>{
    let result;const end=Date.now()+10000;
    do{const r=await call('Runtime.evaluate',{expression:`(()=>{const e=document.querySelector('#varta-output');return{status:e.dataset.status,text:e.textContent,faces:document.fonts.size};})()`,returnByValue:true});result=r.result.value;if(result.status!=='ready')await new Promise(done=>setTimeout(done,20));}while(result.status!=='ready'&&Date.now()<end);
    assert.equal(result.status,'ready',JSON.stringify(result));assert.equal(result.faces,4);
    const golden=JSON.parse(await readFile(join(root,'view-font-packaged.golden.json'),'utf8'));assert.equal(result.text,golden.logs.join('\n'));
    const {root:document}=await call('DOM.getDocument',{depth:-1,pierce:true}),widths={};
    const visit=async(node)=>{
      for(let i=0;i<(node.attributes?.length??0);i+=2)if(node.attributes[i]==='id'&&['regular','bold','italic','generic'].includes(node.attributes[i+1])){
        const remote=await call('DOM.resolveNode',{nodeId:node.nodeId});
        const measured=await call('Runtime.callFunctionOn',{objectId:remote.object.objectId,functionDeclaration:`function(){const range=document.createRange();range.selectNodeContents(this);return range.getBoundingClientRect().width;}`,returnByValue:true});
        widths[node.attributes[i+1]]=measured.result.value;
      }
      for(const child of [...(node.children??[]),...(node.shadowRoots??[])])await visit(child);
    };await visit(document);
    assert.deepEqual(widths,{regular:32,bold:72,italic:72,generic:72});
  });
  await withBrowserPage(`${base}/__embed`,async(call)=>{
    const result=await call('Runtime.evaluate',{expression:`(async()=>{globalThis.fontApps=[];const bytes=await(await fetch(${JSON.stringify(base+'/view-font-wide/varta-resource-0.ttf')})).arrayBuffer();const ambient=new FontFace('Brand',bytes);await ambient.load();document.fonts.add(ambient);globalThis.ambientFace=ambient;for(const name of ['view-font-ttf','view-font-wide']){const{mount}=await import(${JSON.stringify(base+'/')}+name+'/varta-host.js');const container=document.createElement('div');document.body.append(container);const app=mount({container});fontApps.push(app);await app.ready;}return document.fonts.size;})()`,awaitPromise:true,returnByValue:true});
    assert(!result.exceptionDetails,JSON.stringify(result.exceptionDetails));assert.equal(result.result.value,3);
    const nodes=await inspect(call);assert.equal(nodes.length,6);
    for(const node of nodes.slice(0,3))assert.equal(node.width,32);
    for(const node of nodes.slice(3))assert.equal(node.width,72);
    assert.notEqual(nodes[0].family,nodes[3].family);
    const closed=await call('Runtime.evaluate',{expression:`(async()=>{fontApps[0].close();await fontApps[0].finished;return document.fonts.size;})()`,awaitPromise:true,returnByValue:true});assert.equal(closed.result.value,2);
    for(const node of (await inspect(call)).slice(3))assert.equal(node.width,72);
    const done=await call('Runtime.evaluate',{expression:`(async()=>{fontApps[1].close();await fontApps[1].finished;const left=[...document.fonts].map(face=>face.family);document.fonts.delete(ambientFace);return left;})()`,awaitPromise:true,returnByValue:true});assert.deepEqual(done.result.value,['Brand']);
  });
  for(const mode of ['abort','deadline'])await withBrowserPage(`${base}/__embed`,async(call)=>{
    const result=await call('Runtime.evaluate',{expression:`(async()=>{const Original=FontFace;let release,entered;const loading=new Promise(resolve=>entered=resolve);globalThis.FontFace=class extends Original{load(){entered();return new Promise(resolve=>{release=()=>resolve(this);});}};const logs=[];const{mount}=await import(${JSON.stringify(base+'/view-font-ttf/varta-host.js')});const container=document.createElement('div');document.body.append(container);const app=mount({container,budgetCeiling:{deadlinePerTurnMs:500},onLog:line=>logs.push(line)});try{await loading;if(${JSON.stringify(mode)}==='abort')app.close();const result=await app.finished.then(value=>({status:value.status}),error=>({status:'failed',error:error.message,guestStarted:error.guestStarted}));release();await new Promise(resolve=>setTimeout(resolve,20));return{...result,logs,faces:document.fonts.size};}finally{globalThis.FontFace=Original;app.close();}})()`,awaitPromise:true,returnByValue:true});
    assert(!result.exceptionDetails,JSON.stringify(result.exceptionDetails));const value=result.result.value;
    assert.deepEqual(value.logs,[]);assert.equal(value.faces,0);assert.equal(value.status,mode==='abort'?'closed':'failed');
    if(mode==='deadline'){assert.equal(value.guestStarted,false);assert.match(value.error,/deadline exceeded/);}
  });
  for(const kind of ['hash','length','broken','image']){
    tamper=['broken','image'].includes(kind)?undefined:kind;const name=kind==='broken'?'view-font-broken':kind==='image'?'view-font-as-image':'view-font-ttf';
    await withBrowserPage(`${base}/${name}/index.html`,async(call)=>{
      let result;const end=Date.now()+10000;
      do{const r=await call('Runtime.evaluate',{expression:`(()=>{const e=document.querySelector('#varta-output');return{status:e.dataset.status,text:e.textContent,faces:document.fonts.size};})()`,returnByValue:true});result=r.result.value;if(result.status!=='failed')await new Promise(done=>setTimeout(done,20));}while(result.status!=='failed'&&Date.now()<end);
      assert.equal(result.status,'failed',JSON.stringify(result));assert.equal(result.faces,0);assert(!result.text.includes('varta log:'),'verification/sanitization must precede even the first guest log');
      if(kind==='image')assert.match(result.text,/unpackaged view asset/);
      if(['hash','length'].includes(kind))assert.match(result.text,kind==='hash'?/integrity mismatch/:/length mismatch/);
    });
  }
  assert(!requests.some(path=>/Brand|woff.*\?/.test(path)),'font-family values must never become fetch URLs');
  console.log('Packaged fonts passed: TTF/OTF/WOFF/WOFF2 decoding, native usage, direct/quoted/variable families, resource-section faces with weight, style, generic alias and default family, actual text metrics, simultaneous session and ambient-name isolation, cleanup and pre-guest failures');
}finally{await new Promise(done=>server.close(done));}
