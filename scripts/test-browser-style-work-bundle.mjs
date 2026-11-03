// Source-free generated loader/Worker regressions for checked view resource failures.
import assert from 'node:assert/strict';
import {mkdtemp,writeFile,readFile,rm} from 'node:fs/promises';
import {createServer} from 'node:http';
import {tmpdir} from 'node:os';
import {join,resolve,sep} from 'node:path';
import {spawnSync} from 'node:child_process';
import {withBrowserPage} from './browser-probe.mjs';
import {registry} from '../crates/varta-common/browser/registry.mjs';
assert(process.argv[2],'Usage: node scripts/test-browser-style-work-bundle.mjs <varta-executable> [--tree-depth|--cascade-work|--expanded-values]');
const treeDepth=process.argv[3]==='--tree-depth',cascadeWork=process.argv[3]==='--cascade-work',expandedValues=process.argv[3]==='--expanded-values';
assert(process.argv[3]===undefined||treeDepth||cascadeWork||expandedValues,'optional mode: --tree-depth, --cascade-work or --expanded-values');
const directory=await mkdtemp(join(tmpdir(),'varta-browser-view-resource-'));
const expandedSource=':root{--seed:"'+'a'.repeat(registry.view.limits.valueBytes/2-3)+'"}.a{--copy:var(--seed)/}';
const limitBody=expandedValues?`View.stylesheet(2,${JSON.stringify(expandedSource)});
    for(let i=0;i<127;i++){const child=View.node();View.createElement(child,${registry.view.tags.find(t=>t.name==='span').code});View.setClass(child,'a');View.insert(1,child,0);}`:cascadeWork?`const count=event.seq===0?1338:1339;View.stylesheet(2,'.a{}'.repeat(count));
    for(let i=0;i<128;i++){const child=View.node();View.createElement(child,${registry.view.tags.find(t=>t.name==='span').code});View.setClass(child,'a');View.insert(1,child,0);}
    View.command(button,${registry.view.commands.find(c=>c.name==='focus').code},0);View.stylesheet(2,'');`:treeDepth?`let parent=1;const count=${registry.view.limits.treeDepth}+(event.seq===0?1:2);
    for(let i=0;i<count;i++){const child=View.node();View.createElement(child,${registry.view.tags.find(t=>t.name==='span').code});View.insert(parent,child,0);parent=child;}`:String.raw`
    const count=event.seq===0?1100:1101;
    const conditions='(min-width:0px) and '.repeat(499)+'(min-width:0px)';
    View.stylesheet(2,'@media '+conditions+'{'+'.a{}'.repeat(count)+'}');`;
const source=String.raw`export function main():void {
  View.setTitle('before');View.stylesheet(1,'.trigger { padding:4px }');
  const button=View.node();View.createElement(button,20);View.setAttribute(button,1,'trigger');View.setClass(button,'trigger');
  const label=View.node();View.createText(label,'Trigger');View.insert(button,label,0);
  View.on(button,1,(event:ViewEvent):void=>{
    ${limitBody}View.setTitle('after');
    try{View.commit();console.log('AFTER');}catch(error){console.log('CAUGHT');}
  },0,0,0,0);
  View.insert(1,button,0);View.commit();console.log('READY');
}`;
let server;
try{
  await writeFile(join(directory,'main.ts'),source);
  await writeFile(join(directory,'varta.config.json'),JSON.stringify({entry:'main.ts',target:'wasm',policy:'strict',out:'dist',languageProfile:'checked-f64-utf16-v38',capabilities:{}}));
  const built=spawnSync(resolve(process.argv[2]),['build','--target','browser','--config',join(directory,'varta.config.json')],{encoding:'utf8',timeout:30000});
  assert.equal(built.error,undefined);assert.equal(built.status,0,built.stderr);
  // The browser must execute only its packaged artifact, without source/config.
  await rm(join(directory,'main.ts'));await rm(join(directory,'varta.config.json'));
  const root=join(directory,'web');
  server=createServer(async(request,response)=>{
    try{
      const url=new URL(request.url,'http://loopback'),name=url.pathname==='/'?'/index.html':url.pathname;
      const path=resolve(root,`.${decodeURIComponent(name)}`);if(!path.startsWith(root+sep))throw Error('outside bundle');
      const bytes=await readFile(path);response.setHeader('Cache-Control','no-store');
      response.setHeader('Content-Type',path.endsWith('.js')?'text/javascript':path.endsWith('.html')?'text/html':path.endsWith('.wasm')?'application/wasm':'application/octet-stream');response.end(bytes);
    }catch{response.writeHead(404).end();}
  });
  await new Promise(done=>server.listen(0,'127.0.0.1',done));
  await withBrowserPage(`http://127.0.0.1:${server.address().port}/`,async call=>{
    const output=async()=>{const r=await call('Runtime.evaluate',{returnByValue:true,expression:"(()=>{const o=document.querySelector('#varta-output');return {status:o?.dataset.status,text:o?.textContent,title:document.title};})()"});assert(!r.exceptionDetails);return r.result.value;};
    const wait=async predicate=>{const end=Date.now()+10000;for(;;){const state=await output();if(predicate(state))return state;if(Date.now()>=end)throw Error(JSON.stringify(state));await new Promise(done=>setTimeout(done,20));}};
    const ready=await wait(state=>state.status==='ready'||state.status==='failed');assert.equal(ready.status,'ready',ready.text);assert(ready.text.includes('READY'));assert.equal(ready.title,'before');
    const {root:doc}=await call('DOM.getDocument',{depth:-1,pierce:true});
    const find=node=>{for(let i=0;i<(node.attributes?.length??0);i+=2)if(node.attributes[i]==='id'&&node.attributes[i+1]==='trigger')return node;return [...node.children??[],...node.shadowRoots??[]].map(find).find(Boolean);};
    const button=find(doc);assert(button,'initial committed button missing');
    const {model}=await call('DOM.getBoxModel',{nodeId:button.nodeId}),q=model.content,x=(q[0]+q[2])/2,y=(q[1]+q[5])/2;
    for(const type of ['mousePressed','mouseReleased'])await call('Input.dispatchMouseEvent',{type,x,y,button:'left',clickCount:1});
    const failed=await wait(state=>state.status==='failed');assert.match(failed.text,expandedValues?/expanded value budget/:cascadeWork?/cascade work budget/:treeDepth?/tree depth budget/:/stylesheet work budget/);
    assert(!failed.text.includes('AFTER')&&!failed.text.includes('CAUGHT'),failed.text);assert.equal(failed.title,'before');
    const remote=await call('DOM.resolveNode',{nodeId:button.nodeId});
    const retained=await call('Runtime.callFunctionOn',{objectId:remote.object.objectId,returnByValue:true,functionDeclaration:'function(){return this.isConnected&&this.textContent===\'Trigger\';}'});
    assert.equal(retained.result.value,true,'last successful DOM must remain visible');
    console.log(`Generated ${expandedValues?'expanded values':cascadeWork?'cascade work':treeDepth?'tree depth':'CSS work'}: initial stylesheet, trusted event, typed Worker failure before catch/logging and retained DOM/title passed`);
  });
}finally{
  if(server){server.closeAllConnections();await new Promise(done=>server.close(done));}
  await rm(directory,{recursive:true,force:true});
}
