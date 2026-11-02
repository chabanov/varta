// Live generated loader/Worker acceptance for the desktop select fixture.
import {createServer} from 'node:http';
import {readFile} from 'node:fs/promises';
import {resolve,sep} from 'node:path';
import assert from 'node:assert/strict';
import {withBrowserPage} from './browser-probe.mjs';
assert(process.argv[2],'pass the select fixture browser web directory');
const root=resolve(process.argv[2]);
const server=createServer(async(request,response)=>{
  try{
    const pathname=new URL(request.url,'http://loopback').pathname;
    const path=resolve(root,`.${decodeURIComponent(pathname==='/'?'/index.html':pathname)}`);
    if(!path.startsWith(root+sep))throw new Error('outside bundle');
    const bytes=await readFile(path);
    response.setHeader('Content-Type',path.endsWith('.js')?'text/javascript':path.endsWith('.html')?'text/html':path.endsWith('.wasm')?'application/wasm':'application/octet-stream');
    response.end(bytes);
  }catch{response.writeHead(404).end();}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
try{
  await withBrowserPage(`http://127.0.0.1:${server.address().port}/`,async(call)=>{
    async function status(){const r=await call('Runtime.evaluate',{returnByValue:true,expression:"(()=>{const e=document.querySelector('#varta-output');return {status:e?.dataset.status,text:e?.textContent};})()"});assert(!r.exceptionDetails);return r.result.value;}
    async function wait(predicate){const until=Date.now()+10000;for(;;){const current=await status();assert.notEqual(current.status,'failed',current.text);if(predicate(current))return current;if(Date.now()>=until)throw new Error(JSON.stringify(current));await new Promise(resolve=>setTimeout(resolve,25));}}
    await wait(s=>s.status==='ready');
    const doc=await call('DOM.getDocument',{depth:-1,pierce:true});
    const stack=[doc.root];let select;
    while(stack.length){const node=stack.pop();if(node.nodeName==='SELECT'){select=node;break;}stack.push(...node.children??[],...node.shadowRoots??[]);}
    assert(select,'select missing from generated view');
    const object=await call('DOM.resolveNode',{nodeId:select.nodeId});
    async function selected(){const r=await call('Runtime.callFunctionOn',{objectId:object.object.objectId,returnByValue:true,functionDeclaration:'function(){return {value:this.value,index:this.selectedIndex};}'});assert(!r.exceptionDetails);return r.result.value;}
    assert.deepEqual(await selected(),{value:'same',index:1});
    await call('Page.bringToFront');
    await call('Runtime.callFunctionOn',{objectId:object.object.objectId,functionDeclaration:'function(){this.focus();}'});
    // A real character key exercises native select typeahead without relying on
    // the platform's separate popup window being available in headless Chrome.
    for(const type of ['keyDown','keyUp'])await call('Input.dispatchKeyEvent',{type,key:'a',code:'KeyA',windowsVirtualKeyCode:65,...(type==='keyDown'?{text:'a',unmodifiedText:'a'}:{})});
    await wait(s=>s.text.includes('input same 1')&&s.text.includes('change same 1'));
    assert.deepEqual(await selected(),{value:'same',index:2});
    // The input callback echoes the same value through the actual Worker batch.
    // Selection must stay on the second duplicate rather than jumping backward.
    await wait(s=>s.status==='ready');assert.deepEqual(await selected(),{value:'same',index:2});
    console.log('Generated browser select: trusted keyboard, duplicate identity, input/change and guest echo passed');
  });
}finally{await new Promise(resolve=>server.close(resolve));}
