// Compare the same environment program through native and browser runtimes.
import {mkdtemp,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {spawn,spawnSync} from 'node:child_process';
import assert from 'node:assert/strict';
import {withBrowserPage} from './browser-probe.mjs';
const executable=resolve(process.argv[2]??'target/debug/varta');
const root=await mkdtemp(join(tmpdir(),'varta-browser-environment-'));
const program=`export function main():void {
 const a=varta.result.capture(varta.env.get("VARTA_TEST_ALLOWED"));
 const b=varta.result.capture(varta.env.get("VARTA_TEST_SECRET"));
 const c=varta.result.capture(varta.env.get("VARTA_TEST_MISSING"));
 console.log(JSON.stringify({value:a.value,allowed:a.status,secret:b.status,missing:c.status}));
 console.log(varta.env.has("VARTA_TEST_ALLOWED"));
 console.log(varta.env.has("VARTA_TEST_MISSING"));
}`;
let child;
try {
 await writeFile(join(root,'main.ts'),program);
 await writeFile(join(root,'varta.config.json'),JSON.stringify({entry:'main.ts',target:'wasm',out:'dist/app',languageProfile:'checked-f64-utf16-v38',capabilities:{env:{get:['VARTA_TEST_ALLOWED','VARTA_TEST_MISSING'],has:['VARTA_TEST_ALLOWED','VARTA_TEST_MISSING']}}}));
 const nativeEnvironment={...process.env,VARTA_TEST_ALLOWED:'Привіт 😀',VARTA_TEST_SECRET:'PRIVATE'};delete nativeEnvironment.VARTA_TEST_MISSING;
 const native=spawnSync(executable,['dev','--budget-report'],{cwd:root,encoding:'utf8',env:nativeEnvironment});
 assert.equal(native.status,0,native.stdout+native.stderr);
 const nativeLogs=native.stdout.split(/\r?\n/).filter(line=>line.startsWith('varta log: '));
 const turn=/turn 1 [^:]+: fuel (\d+)\/\d+, host calls (\d+)\/\d+, microtasks (\d+)/.exec(native.stderr);
 const total=/total: \d+ turns, fuel \d+, peak memory \d+\/\d+ bytes, output (\d+)\/\d+ bytes/.exec(native.stderr);
 assert(turn&&total,native.stderr);
 child=spawn(executable,['dev','--target','browser'],{cwd:root,stdio:['ignore','pipe','pipe']});let stdout='',stderr='';child.stdout.on('data',chunk=>stdout+=chunk);child.stderr.on('data',chunk=>stderr+=chunk);
 let url;for(let i=0;!url;i++){url=/varta dev: browser (http:\/\/[^\s]+)/.exec(stdout)?.[1];if(url)break;if(child.exitCode!==null)throw new Error(stderr);if(i>500)throw new Error('startup timeout');await new Promise(done=>setTimeout(done,30));}
 await withBrowserPage(url,async call=>{
  const expression=`(async()=>{const {run}=await import(document.querySelector('script[data-varta-dev]').src);return run({environmentInputs:[['VARTA_TEST_ALLOWED','Привіт 😀'],['VARTA_TEST_SECRET','PRIVATE']]});})()`;
  const response=await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});assert(!response.exceptionDetails,JSON.stringify(response.exceptionDetails));const result=response.result.value;
  assert.equal(result.status,'ok',JSON.stringify(result));assert.deepEqual(result.logs,nativeLogs);assert(!JSON.stringify(result).includes('PRIVATE'));
  assert.deepEqual([result.fuelUsed,result.hostCalls,result.microtasks,result.outputBytes],[Number(turn[1]),Number(turn[2]),Number(turn[3]),Number(total[1])]);
  const deniedExpression=`(async()=>{const {run}=await import(document.querySelector('script[data-varta-dev]').src);return run({environmentInputs:[['VARTA_TEST_ALLOWED','Привіт 😀']],permissionCeiling:{'env.get':{patterns:[]}}});})()`;
  const denied=await call('Runtime.evaluate',{expression:deniedExpression,awaitPromise:true,returnByValue:true});assert(!denied.exceptionDetails,JSON.stringify(denied.exceptionDetails));assert.equal(denied.result.value.status,'ok');assert(denied.result.value.logs.some(line=>line.includes('<capability denied: env>')));assert(!JSON.stringify(denied.result.value).includes('Привіт'));
 });
 console.log('Environment native/browser logs, Unicode, denial, missing input and empty host ceiling passed');
}finally{if(child?.exitCode===null&&child.signalCode===null){child.kill('SIGINT');await new Promise(done=>child.once('exit',done));}await rm(root,{recursive:true,force:true});}
