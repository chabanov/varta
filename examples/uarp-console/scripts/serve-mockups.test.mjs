import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,mkdir,writeFile,rm,symlink} from 'node:fs/promises';
import {join,dirname} from 'node:path';
import {tmpdir} from 'node:os';
import {request as httpRequest} from 'node:http';
import {createMockupServer,MAX_STATIC_BYTES} from './serve-mockups.mjs';

test('mockup server serves only named public assets and rejects private paths and methods',async()=>{
  const root=await mkdtemp(join(tmpdir(),'varta-mockup-server-'));
  const put=async(name,text)=>{const path=join(root,name);await mkdir(dirname(path),{recursive:true});await writeFile(path,text);return path;};
  await put('examples/uarp-console/mockups/console.css','body{}');
  await put('crates/varta-common/browser/registry.mjs','export const registry={};');
  const secret=await put('examples/uarp-console/.env.local','PRIVATE_TEST_KEY');
  const server=createMockupServer({root});await new Promise(done=>server.listen(0,'127.0.0.1',done));
  const base=`http://127.0.0.1:${server.address().port}`;
  try{
    const css=await fetch(base+'/uarp-console/mockups/console.css');assert.equal(css.status,200);assert.equal(await css.text(),'body{}');
    assert.match(css.headers.get('content-type'),/text\/css/);assert.equal(css.headers.get('cache-control'),'no-store');
    const registry=await fetch(base+'/crates/varta-common/browser/registry.mjs');assert.equal(registry.status,200);assert.match(registry.headers.get('content-type'),/javascript/);
    for(const path of ['/uarp-console/.env.local','/uarp-console/mockups/../.env.local','/uarp-console/mockups/%2e%2e/.env.local',
      '/uarp-console/mockups/..%2f.env.local','/crates/varta-common/src/view.rs','/.claude/settings.json','/uarp-console/mockups/unknown.html']){
      const result=await fetch(base+path);assert.equal(result.status,404,path);assert(!(await result.text()).includes('PRIVATE_TEST_KEY'));
    }
    assert.equal((await fetch(base+'/uarp-console/mockups/console.css',{method:'POST'})).status,405);
    const head=await fetch(base+'/uarp-console/mockups/console.css',{method:'HEAD'});assert.equal(head.status,200);assert.equal(await head.text(),'');
    const badHost=await new Promise((done,reject)=>{
      const request=httpRequest(base+'/',{headers:{Host:'unexpected.test'}},response=>{response.resume();response.on('end',()=>done(response.statusCode));});
      request.on('error',reject);request.end();
    });
    assert.equal(badHost,403);
    const linked=join(root,'examples/uarp-console/mockups/frame.css');await symlink(secret,linked);
    assert.equal((await fetch(base+'/uarp-console/mockups/frame.css')).status,404);
    await put('examples/uarp-console/mockups/console.css','X'.repeat(MAX_STATIC_BYTES+1));
    assert.equal((await fetch(base+'/uarp-console/mockups/console.css')).status,413);
  }finally{server.closeAllConnections();await new Promise(done=>server.close(done));await rm(root,{recursive:true,force:true});}
});
