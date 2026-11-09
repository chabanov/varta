// Test-only CDP orchestration of the real CLI, Wasm env policy and rebuild.
import assert from 'node:assert/strict';
import {mkdtemp,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {spawn} from 'node:child_process';
import {withBrowserPage} from './browser-probe.mjs';
const executable=resolve(process.argv[2]??'target/debug/varta');
const root=await mkdtemp(join(tmpdir(),'varta-entry-'));
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
let child;
async function stop(){if(child&&child.exitCode===null&&child.signalCode===null){child.kill('SIGINT');await new Promise(resolve=>child.once('exit',resolve));}child=undefined;}
const evaluate=async(call,expression)=>{const value=await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});assert(!value.exceptionDetails,JSON.stringify(value.exceptionDetails));return value.result.value;};
async function wait(call,needle){let last;for(let i=0;i<500;i++){try{const state=await evaluate(call,"({status:document.querySelector('#varta-output')?.dataset.status,text:document.querySelector('#varta-output')?.textContent,path:location.pathname})");last=state;if(state.status==='ok'&&state.text.includes(needle))return state;}catch(error){if(!/context|navigat/i.test(error.message))throw error;}await delay(20);}throw Error('entry execution/reload timeout: '+JSON.stringify({needle,last}));}
try {
 for(const mode of ['allowed','source-denied','host-denied']){
  const program=version=>`export function main():void{const result=varta.result.capture(varta.env.get('VARTA_ENTRY_PATH'));console.log('${version}:'+(result.status===0?result.value:'denied'));}`;
  await writeFile(join(root,'main.ts'),program('first'));
  await writeFile(join(root,'varta.config.json'),JSON.stringify({entry:'main.ts',target:'wasm',out:'dist/app',languageProfile:'checked-f64-utf16-v38',capabilities:mode==='source-denied'?{}:{env:{get:['VARTA_ENTRY_PATH']}}}));
  await writeFile(join(root,'policy.json'),JSON.stringify({schemaVersion:1,abiVersion:34,capabilities:mode==='host-denied'?[]:[{id:'env.get',patterns:['VARTA_ENTRY_PATH']}]}));
  child=spawn(executable,['dev','--target','browser','--browser-entry-prefix','/apps',...(mode==='host-denied'?['--policy','policy.json']:[])],{cwd:root,stdio:['ignore','pipe','pipe']});
  let stdout='',stderr='';child.stdout.on('data',data=>stdout+=data);child.stderr.on('data',data=>stderr+=data);
  let url;for(let i=0;i<1200;i++){url=/varta dev: browser (http:\/\/\S+)/.exec(stdout)?.[1];if(url)break;assert.equal(child.exitCode,null,stderr);await delay(25);}assert(url,stderr);
  const path='/apps/opaque-address_123';
  await withBrowserPage(new URL(path,url).href,async call=>{
   const expected=mode==='allowed'?path:'denied';
   assert.equal((await wait(call,'first:'+expected)).path,path);
   await writeFile(join(root,'main.ts'),program('second'));
   assert.equal((await wait(call,'second:'+expected)).path,path,'reload must preserve the stable entry');
  });
  await stop();
 }
 console.log('PASS: stable shell entry, actual Wasm path input, source/host denials, immutable assets and path-preserving reload');
} finally {await stop();await rm(root,{recursive:true,force:true});}
