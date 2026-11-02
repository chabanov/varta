// Per-activation URL consent; every open is a spy, never an external navigation.
import {createServer} from 'node:http';
import {readFile} from 'node:fs/promises';
import {resolve,join,sep} from 'node:path';
import assert from 'node:assert/strict';
import {withBrowserPage} from './browser-probe.mjs';
const root=resolve(process.argv[2]),url='https://destination.invalid/path?x=1#part';
const server=createServer(async(request,response)=>{try{
  const route=new URL(request.url,'http://loopback').pathname;
  if(route==='/__embed'){response.setHeader('Content-Type','text/html');response.end(`<!doctype html><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; worker-src blob:; trusted-types varta-worker 'allow-duplicates'; require-trusted-types-for 'script'"><title>Navigation consent</title>`);return;}
  const path=resolve(root,`.${route}`);if(!path.startsWith(root+sep))throw new Error('outside fixture');response.setHeader('Content-Type',path.endsWith('.js')?'text/javascript':'application/octet-stream');response.end(await readFile(path));
}catch{response.writeHead(404).end();}});
await new Promise(done=>server.listen(0,'127.0.0.1',done));const base=`http://127.0.0.1:${server.address().port}`;
const evaluate=async(call,expression)=>{const result=await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});assert(!result.exceptionDetails,JSON.stringify(result.exceptionDetails));return result.result.value;};
const wait=async(call,expression)=>{for(let i=0;i<250;i++){if(await evaluate(call,expression))return;await new Promise(done=>setTimeout(done,20));}throw new Error(`not settled ${expression}: ${JSON.stringify(await evaluate(call,'probe'))}`);};
const find=(node,predicate)=>predicate(node)?node:[...(node.children??[]),...(node.shadowRoots??[])].map(child=>find(child,predicate)).find(Boolean);
const attribute=(node,key,value)=>node.attributes?.some((name,index)=>index%2===0&&name===key&&node.attributes[index+1]===value);
const node=async(call,predicate)=>{const{root}=await call('DOM.getDocument',{depth:-1,pierce:true});const value=find(root,predicate);assert(value);return value;};
const click=async(call,predicate=n=>attribute(n,'id','external'))=>{const button=await node(call,predicate),{model}=await call('DOM.getBoxModel',{nodeId:button.nodeId}),q=model.content,x=(q[0]+q[2])/2,y=(q[1]+q[5])/2;await call('Input.dispatchMouseEvent',{type:'mousePressed',x,y,button:'left',clickCount:1});await call('Input.dispatchMouseEvent',{type:'mouseReleased',x,y,button:'left',clickCount:1});};
const normalized=report=>({...report,audit:report.audit.map(({at_ms,...entry})=>entry)});
const setup=async(call,name,answer,extra='')=>{
  const decisions=JSON.parse(await readFile(join(root,`${name}.decisions.json`),'utf8'));
  const ceiling=name==='view-link-hostwide'?{'ui.open_url':{patterns:['https://*']}}:name==='view-link-hostexplicit'?{'ui.open_url':{patterns:['https://destination.invalid/**']}}:name==='view-link-hostdeny'?{}:undefined;
  return evaluate(call,`(async()=>{const{mount,createPermissionPrompt}=await import(${JSON.stringify(base+'/'+name+'/varta-host.js')});globalThis.probe={logs:[],prompts:[],opens:[],turns:[],audit:[]};window.open=(...args)=>{probe.opens.push(args);return null;};const container=document.createElement('div');document.body.append(container);globalThis.app=mount({container,decisions:${JSON.stringify(decisions)},permissionCeiling:${JSON.stringify(ceiling)??'undefined'},budgetCeiling:{deadlinePerTurnMs:700},onPrompt:info=>{probe.prompts.push(info);return ${JSON.stringify(answer)};},onLog:line=>probe.logs.push(line),onTurn:usage=>probe.turns.push(usage),onAudit:entry=>probe.audit.push(entry),${extra}});app.finished.then(value=>probe.result=value,error=>probe.result={status:'failed',error:error.message,permissions:error.permissions});await app.ready;return true;})()`);
};
try{
  for(const name of ['view-link-wide','view-link-explicit','view-link-hostwide','view-link-hostexplicit','view-link-hostdeny'])for(const answer of ['allow','deny','dismiss'])await withBrowserPage(`${base}/__embed`,async(call)=>{
    await setup(call,name,answer);const golden=JSON.parse(await readFile(join(root,`${name}.${answer}.events.golden.json`),'utf8'));
    for(let i=0;i<3;i++){await click(call);await wait(call,`probe.logs.length===${(i+1)*2}&&probe.turns.at(-1).turns===${(i+1)*2+1}`);}
    await evaluate(call,'app.close();true');await wait(call,'probe.result!==undefined');const value=await evaluate(call,'probe');assert.deepEqual(value.result.logs,golden.logs,`${name}/${answer}`);assert.deepEqual(value.result.usage,golden.usage,`${name}/${answer}`);assert.deepEqual(normalized(value.result.permissions),normalized(golden.permissions),`${name}/${answer}`);
    assert.deepEqual(value.opens.map(args=>args[0]),golden.snapshot.opened);for(const args of value.opens)assert.deepEqual(args,[url,'_blank','noopener,noreferrer']);
    const expected=['view-link-wide','view-link-hostwide','view-link-hostexplicit'].includes(name)?3:0;assert.equal(value.prompts.length,expected,`${name}/${answer}`);for(const info of value.prompts){assert.equal(info.capability,'ui.open_url');assert.equal(info.required,false);assert.equal(info.activation,true);assert.deepEqual(info.scope,[url]);}assert.deepEqual(value.result.permissions.decisions,{'ui.open_url':'allow'});
  });
  for(const action of ['allow','revoke','close','observer','platform'])await withBrowserPage(`${base}/__embed`,async(call)=>{
    const extra="onPrompt:info=>{probe.prompts.push(info);return new Promise(resolve=>globalThis.answerNavigation=resolve);},"+(action==='observer'?"onAudit:entry=>{probe.audit.push(entry);if(entry.source==='prompt')throw new Error('audit observer failed');},":"");
    await setup(call,'view-link-wide','allow',extra);if(action==='platform')await evaluate(call,"window.open=(...args)=>{probe.opens.push(args);throw new Error('platform refused');};true");
    await click(call);await wait(call,'probe.prompts.length===1');const before=await evaluate(call,'probe.turns.at(-1)');assert.deepEqual(await evaluate(call,'probe.logs'),[]);assert.deepEqual(await evaluate(call,'probe.opens'),[]);
    if(action==='allow'){await new Promise(done=>setTimeout(done,1000));assert.equal(await evaluate(call,'probe.result===undefined'),true);assert.deepEqual(await evaluate(call,'probe.turns.at(-1)'),before);}
    if(action==='revoke')assert.equal(await evaluate(call,"app.revoke('ui.open_url')"),true);
    if(action==='close')await evaluate(call,'app.close();true');await evaluate(call,"answerNavigation('allow');true");
    if(action==='close'||action==='observer'){await wait(call,'probe.result!==undefined');assert.equal(await evaluate(call,'probe.opens.length'),0);if(action==='observer')assert.match(await evaluate(call,'probe.result.error'),/audit observer failed/);}
    else{const status=action==='revoke'?1:action==='platform'?2:0;await wait(call,`probe.logs.includes('varta log: link ${status}')`);assert.equal(await evaluate(call,'probe.opens.length'),action==='revoke'?0:1);await evaluate(call,'app.close();true');}
  });
  await withBrowserPage(`${base}/__embed`,async(call)=>{
    await setup(call,'view-link-wide','allow',"onPrompt:createPermissionPrompt(),");await click(call);await wait(call,"document.querySelectorAll('[data-varta-permission-prompt]').length===1");const dialog=await node(call,n=>n.nodeName==='DIALOG'),object=await call('DOM.resolveNode',{nodeId:dialog.nodeId});const text=await call('Runtime.callFunctionOn',{objectId:object.object.objectId,functionDeclaration:'function(){return this.textContent;}',returnByValue:true});assert(text.result.value.includes(url));assert(text.result.value.includes('Open once'));assert(text.result.value.includes('not saved'));
    await click(call,n=>attribute(n,'data-decision','allow'));await wait(call,"probe.logs.includes('varta log: link 0')");assert.equal(await evaluate(call,'probe.opens.length'),1);await evaluate(call,'app.close();true');
  });
  for(const action of ['headless','synthetic','overlap'])await withBrowserPage(`${base}/__embed`,async(call)=>{
    const extra=action==='headless'?'onPrompt:undefined,':action==='overlap'?"onPrompt:info=>{probe.prompts.push(info);return new Promise(resolve=>globalThis.answerNavigation=resolve);},":'';
    await setup(call,'view-link-wide','allow',extra);
    if(action==='synthetic'){
      const link=await node(call,n=>attribute(n,'id','external')),object=await call('DOM.resolveNode',{nodeId:link.nodeId});await call('Runtime.callFunctionOn',{objectId:object.object.objectId,functionDeclaration:'function(){this.click();}'});await wait(call,'probe.logs.length===1&&probe.turns.at(-1).turns===2');assert.deepEqual(await evaluate(call,'probe.logs'),['varta log: click']);
    }else{
      await click(call);
      if(action==='overlap'){await wait(call,'probe.prompts.length===1');await click(call);assert.equal(await evaluate(call,'probe.prompts.length'),1);await evaluate(call,"answerNavigation('allow');true");await wait(call,'probe.logs.length===4');assert.deepEqual(await evaluate(call,'probe.logs'),['varta log: click','varta log: click','varta log: link 1','varta log: link 0']);assert.equal(await evaluate(call,'probe.opens.length'),1);}
      else await wait(call,"probe.logs.includes('varta log: link 1')");
    }
    if(action!=='overlap'){assert.equal(await evaluate(call,'probe.opens.length'),0);assert.equal(await evaluate(call,'probe.prompts.length'),0);}await evaluate(call,'app.close();true');
  });
  console.log('Browser navigation matches native: per-activation consent, explicit/wide/request/ceiling rules, ephemeral choices, exact logs/usage/audits, checked pause/resume, uncharged waits, revocation/abort/platform refusal and owned URL dialog');
}finally{await new Promise(done=>server.close(done));}
