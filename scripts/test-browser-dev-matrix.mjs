import {pathToFileURL} from 'node:url';
import {resolve} from 'node:path';
if (process.argv.length !== 4) throw new Error('Usage: node scripts/test-browser-dev-matrix.mjs <varta-executable> <playwright-module>');
const {firefox,webkit,chromium} = await import(pathToFileURL(resolve(process.argv[3])).href);
import {mkdtemp,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {spawn} from 'node:child_process';
import assert from 'node:assert/strict';
const executable=resolve(process.argv[2]);
const config={entry:'main.ts',target:'wasm',out:'dist/app',languageProfile:'checked-f64-utf16-v38'};
const report={playwright:'1.64.1',cases:[],errors:[]};
const view='export function main():void{const root=View.node();View.createElement(root,1);const button=View.node();View.createElement(button,20);const text=View.node();View.createText(text,"0");View.insert(button,text,0);View.insert(root,button,0);let count=0;View.on(button,1,(event:ViewEvent):void=>{count++;View.setText(text,String(count));console.log(count);},0,0,0,0);View.mount(root);}';
for(const [name,type] of [['firefox',firefox],['webkit',webkit],['chrome',chromium]]){
 let browser,child,stderr='',stdout='';const root=await mkdtemp(join(tmpdir(),'varta-live-matrix-'));
 async function stop(){if(child?.exitCode===null&&child.signalCode===null){child.kill('SIGINT');await Promise.race([new Promise(done=>child.once('exit',done)),new Promise((_,reject)=>setTimeout(()=>reject(new Error('shutdown timeout')),5000))]);assert.equal(child.exitCode,0,stderr);}}
 try{
  await writeFile(join(root,'varta.config.json'),JSON.stringify(config));await writeFile(join(root,'main.ts'),'export function main():void{console.log("initial");}');
  child=spawn(executable,['dev','--target','browser'],{cwd:root,stdio:['ignore','pipe','pipe']});child.stdout.on('data',chunk=>stdout+=chunk);child.stderr.on('data',chunk=>stderr+=chunk);
  let url;for(let i=0;!url;i++){url=/varta dev: browser (http:\/\/[^\s]+)/.exec(stdout)?.[1];if(url)break;if(child.exitCode!==null)throw new Error(stderr);if(i>500)throw new Error('startup timeout');await new Promise(done=>setTimeout(done,30));}
  browser=await type.launch({headless:true,timeout:45000,...(name==='chrome'?{channel:'chrome',chromiumSandbox:true}:{})});const context=await browser.newContext();
  await context.addInitScript(()=>{const attach=Element.prototype.attachShadow;globalThis.vartaTestRoots=new WeakMap();Element.prototype.attachShadow=function(options){const root=attach.call(this,options);globalThis.vartaTestRoots.set(this,root);return root;};const terminate=Worker.prototype.terminate;Worker.prototype.terminate=function(...args){sessionStorage.setItem('workerStops',String(Number(sessionStorage.getItem('workerStops')??0)+1));return terminate.apply(this,args);};});
  const page=await context.newPage();await page.goto(url);const logs=async(text,status='ok')=>page.waitForFunction(({text,status})=>{const out=document.querySelector('#varta-output');return out?.dataset.status===status&&out.textContent===text;},{text,status},{timeout:20000});
  await logs('varta log: initial');
  await writeFile(join(root,'main.ts'),'import {value} from "./missing.ts"; export function main():void{console.log(value);}');
  await page.waitForFunction(()=>Boolean(document.querySelector('#varta-dev-errors')?.textContent),null,{timeout:20000});
  await writeFile(join(root,'missing.ts'),'export const value:string="fixed";');await logs('varta log: fixed');
  report.cases.push({engine:name,version:browser.version(),name:'missing dependency error/recovery and live reload',status:'pass'});
  await writeFile(join(root,'main.ts'),view);await logs('','ready');
  const point=await page.evaluate(()=>{const host=document.querySelector('#varta-view'),root=globalThis.vartaTestRoots.get(host.firstElementChild),button=root.querySelector('button'),box=button.getBoundingClientRect();return{x:box.x+box.width/2,y:box.y+box.height/2};});
  await page.mouse.click(point.x,point.y);await logs('varta log: 1','ready');await page.evaluate(()=>sessionStorage.setItem('workerStops','0'));
  await writeFile(join(root,'main.ts'),'export function main():void{console.log("replaced");}');await logs('varta log: replaced');
  assert.equal(await page.evaluate(()=>sessionStorage.getItem('workerStops')),'2');
  // The old live Worker stops before navigation, then the bounded replacement
  // console Worker terminates normally on completion: exactly two terminations.
  report.cases.push({engine:name,version:browser.version(),name:'trusted view input and owned live Worker replacement',status:'pass'});
  await writeFile(join(root,'main.ts'),view);await logs('','ready');await page.evaluate(()=>sessionStorage.setItem('workerStops','0'));
  await stop();await page.waitForFunction(()=>document.querySelector('#varta-dev-errors')?.textContent.includes('stopped'),null,{timeout:10000});
  assert.equal(await page.evaluate(()=>sessionStorage.getItem('workerStops')),'1');
  assert(!stdout.includes('varta log:'),'guest must not run on native server');
  report.cases.push({engine:name,version:browser.version(),name:'server shutdown stops owned live Worker',status:'pass'});
  await context.close();console.log('PASS',name,browser.version(),report.cases.filter(c=>c.engine===name).length,'live dev scenarios');
 }catch(error){report.errors.push({engine:name,error:String(error),stderr,stdout});console.log('FAIL',name,String(error));}
 finally{await browser?.close();if(child?.exitCode===null&&child.signalCode===null){child.kill('SIGKILL');await new Promise(done=>child.once('exit',done));}await rm(root,{recursive:true,force:true});}
}
console.log('TOTAL',report.cases.length,'passed',report.errors.length,'failed');if(report.errors.length)process.exitCode=1;
