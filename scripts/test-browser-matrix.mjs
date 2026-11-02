import {pathToFileURL} from 'node:url';
import {resolve} from 'node:path';
if (process.argv.length !== 4) throw new Error('Usage: node scripts/test-browser-matrix.mjs <fixture-directory> <playwright-module>');
const {firefox,webkit,chromium} = await import(pathToFileURL(resolve(process.argv[3])).href);
import {readFile,readdir} from 'node:fs/promises';
import {createServer} from 'node:http';
import {join} from 'node:path';
import assert from 'node:assert/strict';
const root=resolve(process.argv[2]);
const names=['ok','fifo','handled','unhandled','timer-order','timer-v36','timer-v37','timer-cancel','timer-unref','view-live','view-image','view-font-woff','view-font-woff2'];
const files=new Map(),goldens=new Map();
for(const name of names){goldens.set(name,JSON.parse(await readFile(join(root,name+'.golden.json'),'utf8')));for(const entry of await readdir(join(root,name),{withFileTypes:true})){if(entry.isFile())files.set('/'+name+'/'+entry.name,await readFile(join(root,name,entry.name)));}}
const server=createServer((request,response)=>{const path=request.url.split('?')[0],bytes=files.get(path);if(!bytes){response.writeHead(404).end();return;}response.setHeader('Content-Type',path.endsWith('.js')?'text/javascript':path.endsWith('.html')?'text/html':path.endsWith('.json')?'application/json':path.endsWith('.wasm')?'application/wasm':'application/octet-stream');response.setHeader('Cache-Control','no-store');response.end(bytes);});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const report={playwright:'1.64.1',fixtureRoot:root,cases:[],errors:[]};
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
 }catch(error){report.errors.push({engine,error:String(error)});console.log('ENGINE FAIL',engine,String(error));}finally{await browser?.close();}
}}finally{await new Promise(resolve=>server.close(resolve));}
console.log('TOTAL',report.cases.length,'passed',report.errors.length,'failed');if(report.errors.length)process.exitCode=1;
