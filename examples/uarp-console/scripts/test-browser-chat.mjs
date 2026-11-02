// Optional real-browser pilot acceptance; runs only this application, never engine fixtures.
import {readFile} from 'node:fs/promises';
import {resolve,dirname,join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawn} from 'node:child_process';
import assert from 'node:assert/strict';
import {withBrowserPage} from '../../../scripts/browser-probe.mjs';
const project=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const binary=resolve(process.argv[2]??'target/debug/varta');
let child;
const sleep=ms=>new Promise(done=>setTimeout(done,ms));
try{
 child=spawn(binary,['dev','--target','browser','--config',join(project,'varta.view.json')],{stdio:['ignore','pipe','pipe']});let stdout='',stderr='';child.stdout.on('data',b=>stdout+=b);child.stderr.on('data',b=>stderr+=b);
 let base;for(let i=0;!base;i++){base=/varta dev: browser (http:\/\/[^\s]+)/.exec(stdout)?.[1];if(base)break;if(child.exitCode!==null)throw Error(stderr);if(i>1000)throw Error('server startup timeout');await sleep(30);}
 const page=await(await fetch(base)).text();const script=/data-varta-dev src="([^"]+)"/.exec(page)?.[1];assert(script);const source=await(await fetch(new URL(script,base))).text();const descriptor=JSON.parse(/^const descriptor = (.+);\n/.exec(source)[1]);
 await withBrowserPage(base,async call=>{
  const evaluate=async expression=>{const r=await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});assert(!r.exceptionDetails,JSON.stringify(r.exceptionDetails));return r.result.value;};
  // Dismiss the generated page's independent first-run session. The test below
  // owns a second session with explicit test decisions and environment inputs.
  for(let i=0;i<300;i++){
   const {root}=await call('DOM.getDocument',{depth:-1,pierce:true});const find=n=>n.nodeName==='BUTTON'&&(n.children??[]).some(c=>c.nodeValue==='Deny')?n:[...(n.children??[]),...(n.shadowRoots??[])].map(find).find(Boolean);const button=find(root);
   if(button){const {model}=await call('DOM.getBoxModel',{nodeId:button.nodeId}),q=model.content,x=(q[0]+q[2])/2,y=(q[1]+q[5])/2;await call('Input.dispatchMouseEvent',{type:'mousePressed',x,y,button:'left',clickCount:1});await call('Input.dispatchMouseEvent',{type:'mouseReleased',x,y,button:'left',clickCount:1});break;}
   if(i===299)throw Error('initial permission prompt missing');await sleep(20);
  }
  await evaluate(`(()=>{globalThis.pilot={ready:false,logs:[],turns:0};const container=document.createElement('div');container.id='pilot-test';document.body.append(container);import(${JSON.stringify(new URL(script,base).href)}).then(({mount})=>{const app=mount({container,environmentInputs:[['TEST_VIEW','chat']],decisions:{applications:[{artifact:${JSON.stringify(descriptor.original.identity)},decisions:[{id:'env.get',decision:'allow'},{id:'time.timer',decision:'allow'}]}]},onLog:line=>pilot.logs.push(line),onReady:()=>pilot.ready=true,onTurn:usage=>pilot.turns=usage.turns});globalThis.pilotApp=app;app.finished.catch(error=>pilot.error=error.message);}).catch(error=>pilot.error=error.message);})()`);
  const until=async predicate=>{for(let i=0;i<700;i++){const state=await evaluate('globalThis.pilot');if(state?.error)throw Error(JSON.stringify(state));if(predicate(state))return state;await sleep(20);}throw Error('pilot timeout');};
  await until(s=>s?.ready);await sleep(800);
  const find=async id=>{const {root}=await call('DOM.getDocument',{depth:-1,pierce:true});const visit=n=>{const a=n.attributes??[];for(let i=0;i<a.length;i+=2)if(a[i]==='id'&&a[i+1]===id)return n;return [...(n.children??[]),...(n.shadowRoots??[])].map(visit).find(Boolean);};return visit(root);};
  const click=async id=>{const before=(await evaluate('pilot.turns'));const element=await find(id);assert(element,`missing ${id}`);await call('DOM.scrollIntoViewIfNeeded',{nodeId:element.nodeId});const {model}=await call('DOM.getBoxModel',{nodeId:element.nodeId}),q=model.content,x=(q[0]+q[2])/2,y=(q[1]+q[5])/2;await call('Input.dispatchMouseEvent',{type:'mousePressed',x,y,button:'left',clickCount:1});await call('Input.dispatchMouseEvent',{type:'mouseReleased',x,y,button:'left',clickCount:1});await until(s=>s.turns>before);};
  await click('thinking-run-1');await click('approve-run-2');await click('option-run-3-1');
  const composer=await find('composer');assert(composer);await call('DOM.scrollIntoViewIfNeeded',{nodeId:composer.nodeId});const {model}=await call('DOM.getBoxModel',{nodeId:composer.nodeId}),q=model.content,x=(q[0]+q[2])/2,y=(q[1]+q[5])/2;await call('Input.dispatchMouseEvent',{type:'mousePressed',x,y,button:'left',clickCount:1});await call('Input.dispatchMouseEvent',{type:'mouseReleased',x,y,button:'left',clickCount:1});
  let before=(await evaluate('pilot.turns'));await call('Input.insertText',{text:'Дякую!'});await until(s=>s.turns>before);
  before=(await evaluate('pilot.turns'));await call('Input.dispatchKeyEvent',{type:'keyDown',key:'Enter',code:'Enter',windowsVirtualKeyCode:13,nativeVirtualKeyCode:13});await call('Input.dispatchKeyEvent',{type:'keyUp',key:'Enter',code:'Enter',windowsVirtualKeyCode:13,nativeVirtualKeyCode:13});await until(s=>s.turns>before);await sleep(300);
  const {root}=await call('DOM.getDocument',{depth:-1,pierce:true});const text=[];const walk=n=>{if(n.nodeName==='#text'&&n.nodeValue.trim())text.push(n.nodeValue.trim());for(const child of [...(n.children??[]),...(n.shadowRoots??[])])walk(child);};walk(root);
  const expectedText=await readFile(join(project,'src/tests/view/chat.expected.json'),'utf8');
  const expected=JSON.parse(expectedText.replace(/\nvarta dev: completed\s*$/,''));
  const names=[];const collect=value=>{if(Array.isArray(value))value.forEach(collect);else if(value&&typeof value==='object'){if(typeof value.name==='string')names.push(value.name);Object.values(value).forEach(collect);}};collect(expected.accessibility);
  for(const value of ['Дякую!','надсилається…']){assert(names.includes(value),`native expected output missing ${value}`);assert(text.includes(value),JSON.stringify(text));}
  assert.deepEqual((await evaluate('pilot.logs')),[]);
  console.log('UARP chat browser: trusted thinking/approval/option/input/Enter turns and final text passed');await evaluate('pilotApp.close()');
 });
}finally{if(child?.exitCode===null&&child.signalCode===null){child.kill('SIGINT');await new Promise(done=>child.once('exit',done));}}
