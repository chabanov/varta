import {pathToFileURL} from 'node:url';
import {resolve} from 'node:path';
if (process.argv.length !== 4) throw new Error('Usage: node scripts/test-browser-integrity-matrix.mjs <fixture-directory> <playwright-module>');
const {firefox,webkit,chromium} = await import(pathToFileURL(resolve(process.argv[3])).href);
import {readFile,readdir} from 'node:fs/promises';
import {createServer} from 'node:http';
import {join} from 'node:path';
import assert from 'node:assert/strict';
const root=resolve(process.argv[2]),files=new Map();
for(const entry of await readdir(join(root,'ok'),{withFileTypes:true}))if(entry.isFile())files.set('/ok/'+entry.name,await readFile(join(root,'ok',entry.name)));
let tamper;
const server=createServer((request,response)=>{const path=request.url.split('?')[0];let bytes=files.get(path);if(!bytes){response.writeHead(404).end();return;}if(tamper&&path==='/ok/'+tamper){if(tamper==='varta-host.js')bytes=Buffer.concat([bytes,Buffer.from('\nglobalThis.tamperedLoaderRan=true;\n')]);else{bytes=Buffer.from(bytes);bytes[8]^=1;}}response.setHeader('Content-Type',path.endsWith('.js')?'text/javascript':path.endsWith('.html')?'text/html':path.endsWith('.wasm')?'application/wasm':'application/octet-stream');response.setHeader('Cache-Control','no-store');response.end(bytes);});
await new Promise(done=>server.listen(0,'127.0.0.1',done));const report={cases:[],errors:[]};
try{for(const[name,type,options]of[['firefox',firefox,{}],['webkit',webkit,{}],['chrome',chromium,{channel:'chrome',chromiumSandbox:true}]]){let browser;try{browser=await type.launch({headless:true,timeout:45000,...options});for(const file of ['app.wasm','app.private.wasm','app.manifest.json','varta-worker.js','varta-host.js']){tamper=file;const context=await browser.newContext();await context.addInitScript(()=>{globalThis.testWorkers=0;const Original=Worker;globalThis.Worker=class extends Original{constructor(...args){super(...args);globalThis.testWorkers++;}};});const page=await context.newPage();try{
 await page.goto(`http://127.0.0.1:${server.address().port}/ok/index.html`);
 if(file==='varta-host.js')await page.waitForTimeout(250);else await page.waitForFunction(()=>document.querySelector('#varta-output')?.dataset.status==='failed',null,{timeout:20000});
 const state=await page.evaluate(()=>{const out=document.querySelector('#varta-output');return{status:out.dataset.status,started:out.dataset.guestStarted,text:out.textContent,workers:globalThis.testWorkers,tampered:globalThis.tamperedLoaderRan};});
 assert.equal(state.workers,0,JSON.stringify(state));assert(!state.text.includes('varta log:'),state.text);assert.equal(state.tampered,undefined);
 if(file!=='varta-host.js'){assert.equal(state.status,'failed');assert.equal(state.started,'false');assert(state.text.includes('integrity mismatch'),state.text);}else assert.equal(state.status,undefined);
 report.cases.push({engine:name,version:browser.version(),file,status:'pass'});console.log('PASS',name,file);
 }catch(error){report.errors.push({engine:name,file,error:String(error)});console.log('FAIL',name,file,String(error));}finally{await context.close();}}}catch(error){report.errors.push({engine:name,error:String(error)});}finally{await browser?.close();}}}finally{await new Promise(done=>server.close(done));}
console.log('TOTAL',report.cases.length,'passed',report.errors.length,'failed');if(report.errors.length)process.exitCode=1;
