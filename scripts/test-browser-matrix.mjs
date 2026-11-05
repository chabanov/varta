import {pathToFileURL} from 'node:url';
import {resolve} from 'node:path';
// node scripts/test-browser-matrix.mjs <fixture-directory|-> <playwright-module> [--effects <varta-executable>] [--kv <varta-executable>]
// "-" skips the generated bundle fixtures; --effects also runs the pilot
// effect and idle-collection cases of scripts/test-browser-effects.mjs and
// --kv the KV cases of scripts/test-browser-kv.mjs in every engine against
// fresh native goldens.
const USAGE = 'Usage: node scripts/test-browser-matrix.mjs <fixture-directory|-> <playwright-module> [--effects <varta-executable>] [--kv <varta-executable>]';
const FIXED_ARGUMENTS = 4;
const options = new Map();
for (let at = FIXED_ARGUMENTS; at < process.argv.length; at += 2) {
  const [flag, value] = process.argv.slice(at, at + 2);
  if (!['--effects', '--kv'].includes(flag) || !value || options.has(flag)) throw new Error(USAGE);
  options.set(flag, value);
}
if (process.argv.length < FIXED_ARGUMENTS) throw new Error(USAGE);
const effectsExecutable = options.get('--effects'), kvExecutable = options.get('--kv');
const {firefox,webkit,chromium} = await import(pathToFileURL(resolve(process.argv[3])).href);
// The report records the version of the Playwright package that owns the
// loaded module: the nearest package.json above its real path.
const PACKAGE_FILE='package.json';
async function loadedPlaywrightVersion(module){
 const {realpath}=await import('node:fs/promises');const {dirname}=await import('node:path');
 for(let directory=dirname(await realpath(module));;directory=dirname(directory)){
  const text=await readFile(join(directory,PACKAGE_FILE),'utf8').catch((error)=>{if(error.code==='ENOENT')return undefined;throw error;});
  if(text!==undefined){const {version}=JSON.parse(text);if(typeof version!=='string')throw new Error(`no version in ${join(directory,PACKAGE_FILE)}`);return version;}
  if(dirname(directory)===directory)throw new Error(`no ${PACKAGE_FILE} above ${module}`);
 }
}
import {readFile,readdir} from 'node:fs/promises';
import {createServer} from 'node:http';
import {join} from 'node:path';
import assert from 'node:assert/strict';
const fixtures=process.argv[2]!=='-';
if(!fixtures&&!effectsExecutable&&!kvExecutable)throw new Error('nothing to run: pass a fixture directory, --effects or --kv');
const root=fixtures?resolve(process.argv[2]):undefined;
const names=fixtures?['ok','fifo','handled','unhandled','timer-order','timer-v36','timer-v37','timer-cancel','timer-unref','view-live','view-image','view-font-woff','view-font-woff2']:[];
const files=new Map(),goldens=new Map();
for(const name of names){goldens.set(name,JSON.parse(await readFile(join(root,name+'.golden.json'),'utf8')));for(const entry of await readdir(join(root,name),{withFileTypes:true})){if(entry.isFile())files.set('/'+name+'/'+entry.name,await readFile(join(root,name,entry.name)));}}
const runEffects=effectsExecutable?(await import('./test-browser-effects.mjs')).runEffects:undefined;
const runKv=kvExecutable?(await import('./test-browser-kv.mjs')).runKv:undefined;
const server=createServer((request,response)=>{const path=request.url.split('?')[0],bytes=files.get(path);if(!bytes){response.writeHead(404).end();return;}response.setHeader('Content-Type',path.endsWith('.js')?'text/javascript':path.endsWith('.html')?'text/html':path.endsWith('.json')?'application/json':path.endsWith('.wasm')?'application/wasm':'application/octet-stream');response.setHeader('Cache-Control','no-store');response.end(bytes);});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const report={playwright:await loadedPlaywrightVersion(resolve(process.argv[3])),fixtureRoot:root,cases:[],errors:[]};
console.log('PLAYWRIGHT',report.playwright);
try{for(const [engine,type,options] of [['firefox',firefox,{}],['webkit',webkit,{}],['chrome',chromium,{channel:'chrome',chromiumSandbox:true}]]){
 let browser;try{
  browser=await type.launch({headless:true,timeout:45000,...options});console.log(engine,browser.version());
  for(const name of names){const context=await browser.newContext();const page=await context.newPage();const messages=[];page.on('pageerror',error=>messages.push(String(error)));try{
   await page.goto(`http://127.0.0.1:${server.address().port}/${name}/index.html`,{timeout:45000});
   await page.waitForFunction(()=>['ok','failed','ready'].includes(document.querySelector('#varta-output')?.dataset.status),null,{timeout:20000});
   const result=await page.evaluate(()=>{const output=document.querySelector('#varta-output');return{status:output.dataset.status,text:output.textContent,usage:output.dataset.usage?JSON.parse(output.dataset.usage):null};});
   const golden=goldens.get(name);assert.equal(result.status,name.startsWith('view-')?'ready':golden.status,JSON.stringify(result));
   if(golden.status==='ok')assert.equal(result.text,golden.logs.join('\n'));else assert(result.text.endsWith(golden.error),result.text);
   assert.deepEqual(result.usage,golden.usage);
   report.cases.push({engine,version:browser.version(),name,status:'pass'});console.log('PASS',engine,name);
  }catch(error){report.errors.push({engine,name,error:String(error),messages});console.log('FAIL',engine,name,String(error),messages);}finally{await context.close();}}
  // Each dev-server case opens one fresh context on the dev server's page.
  const open=async(url,action)=>{const context=await browser.newContext();const page=await context.newPage();try{
    await page.goto(url,{timeout:45000});await page.waitForFunction(()=>document.querySelector('script[data-varta-dev]')!==null,null,{timeout:20000});
    return await action((expression)=>page.evaluate(expression));
  }finally{await context.close();}};
  for(const [group,runner,executable] of [['effects',runEffects,effectsExecutable],['kv',runKv,kvExecutable]]){
   if(!runner)continue;
   try{const passed=await runner(resolve(executable),open);for(const name of passed){report.cases.push({engine,version:browser.version(),name:`${group}/${name}`,status:'pass'});console.log('PASS',engine,`${group}/${name}`);}}
   catch(error){report.errors.push({engine,name:group,error:String(error)});console.log('FAIL',engine,group,String(error));}
  }
 }catch(error){report.errors.push({engine,error:String(error)});console.log('ENGINE FAIL',engine,String(error));}finally{await browser?.close();}
}}finally{await new Promise(resolve=>server.close(resolve));}
console.log('TOTAL',report.cases.length,'passed',report.errors.length,'failed');if(report.errors.length)process.exitCode=1;
