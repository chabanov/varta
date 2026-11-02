// Resource admission must survive portable and source-free native execution.
import assert from 'node:assert/strict';
import {mkdtemp,writeFile,rm,rename,readdir} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {registry} from '../crates/varta-common/browser/registry.mjs';

const root=resolve(import.meta.dirname,'..');
const tool=name=>join(root,'target','debug',process.platform==='win32'?`${name}.exe`:name);
const dir=await mkdtemp(join(tmpdir(),'varta-select-work-'));
let isolated;
const code=(rows,name)=>rows.find(row=>row.name===name).code;
try{
  const config=join(dir,'varta.config.json');
  await writeFile(config,JSON.stringify({entry:'main.ts',target:'wasm',out:'dist/app',languageProfile:'checked-f64-utf16-v38',policy:'strict',capabilities:{}}));
  await writeFile(join(dir,'main.ts'),`export function main():void {
    console.log('BEFORE');
    const select=View.node();View.createElement(select,${code(registry.view.tags,'select')});
    const value='X'.repeat(4096);
    for(let i=0;i<20;i++){
      const option=View.node();View.createElement(option,${code(registry.view.tags,'option')});
      View.setAttribute(option,${code(registry.view.attributes,'value')},value);View.insert(select,option,0);
    }
    View.insert(${registry.view.wire.root},select,0);
    for(let i=0;i<128;i++)View.setValue(select,'missing'+i);
    try{View.commit();console.log('AFTER');}catch(error){console.log('CAUGHT');}
  }`);
  const built=spawnSync(tool('varta'),['build','--single','--config',config],{encoding:'utf8',timeout:30000});
  assert.equal(built.status,0,`${built.error??''}\n${built.stdout}\n${built.stderr}`);
  await rm(join(dir,'main.ts'));await rm(config);
  const packaged=join(dir,'dist',process.platform==='win32'?'app.exe':'app');
  const failsBeforeEffects=(binary,args,cwd)=>{
    const result=spawnSync(binary,args,{cwd,encoding:'utf8',timeout:20000});
    assert.equal(result.error,undefined);assert.equal(result.signal,null);
    assert(Number.isInteger(result.status)&&result.status!==0,`${result.stdout}\n${result.stderr}`);
    assert.match(result.stderr,/single-select work exceeds its transaction limit/);
    const lines=result.stdout.trim().split(/\r?\n/);
    assert(lines.includes('varta log: BEFORE'));assert(!lines.includes('varta log: AFTER'));assert(!lines.includes('varta log: CAUGHT'));
  };
  failsBeforeEffects(tool('varta-runtime'),['run',join(dir,'dist','app.wasm')],dir);
  isolated=await mkdtemp(join(tmpdir(),'varta-select-single-'));
  const name=process.platform==='win32'?'app.exe':'app',single=join(isolated,name);
  await rename(packaged,single);await rm(dir,{recursive:true,force:true});
  assert.deepEqual(await readdir(isolated),[name]);
  failsBeforeEffects(single,[],isolated);
  console.log('Select work limit: portable runtime and source-free single binary fail before catch/following effects');
}finally{await rm(dir,{recursive:true,force:true});if(isolated)await rm(isolated,{recursive:true,force:true});}
