// Application-owned acceptance for mockups projected through the official host.
import assert from 'node:assert/strict';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {createMockupServer} from './serve-mockups.mjs';

const [modulePath,selection='all']=process.argv.slice(2);
if(!modulePath)throw new Error('Usage: node examples/uarp-console/scripts/test-mockups.mjs <playwright-module> [all|chrome|firefox|webkit]');
const {chromium,firefox,webkit}=await import(pathToFileURL(resolve(modulePath)).href);
const channel=process.env.VARTA_CHROMIUM_CHANNEL??'chrome';
const engines=[['chrome',chromium,channel==='chromium'?{chromiumSandbox:true}:{channel,chromiumSandbox:true}],['firefox',firefox,{}],['webkit',webkit,{}]];
assert(['all',...engines.map(([name])=>name)].includes(selection),'unknown browser selection');
const server=createMockupServer();await new Promise(done=>server.listen(0,'127.0.0.1',done));
const base=`http://127.0.0.1:${server.address().port}`,results=[];
try{
  for(const [name,type,options] of engines){
    if(selection!=='all'&&selection!==name)continue;
    const browser=await type.launch({headless:true,timeout:45000,...options});
    try{
      const page=await browser.newPage({colorScheme:'light'}),errors=[],requests=[];
      page.on('pageerror',error=>errors.push(error.message));
      await page.route('**/*',route=>{
        const url=route.request().url();requests.push(url);
        return new URL(url).origin===base?route.continue():route.abort();
      });
      await page.goto(base+'/uarp-console/mockups/lint.html');
      await page.waitForFunction(()=>globalThis.__lint,undefined,{timeout:30000});
      const lint=await page.evaluate(()=>globalThis.__lint);
      assert.equal(lint.errors,0,`${name}: ${JSON.stringify(lint.report)}`);
      assert.equal(Object.keys(lint.report.mockups).length,5);
      const fixtures=await page.evaluate(async()=>{
        const {lintTree}=await import('/uarp-console/mockups/lint/lint-html.js');
        const {createViewDom,encodeViewBatch,view,linkPolicy}=await import('/uarp-console/mockups/official-host.js');
        const {mockupBatches}=await import('/uarp-console/mockups/conformance/mockup-batch.js');
        const {compareMockup}=await import('/uarp-console/mockups/conformance/compare.js');
        let checks=0;
        const check=(condition,label)=>{if(!condition)throw new Error(label);checks++;};
        const doc=body=>new DOMParser().parseFromString(`<div id="l0-root">${body}</div>`,'text/html');
        const cases=[
          ['plain','<span>A</span>',true],['textarea','<textarea>one\ntwo</textarea>',true],
          ['radio','<input type="radio" checked>',true],
          ['select','<select><option value="same" selected>A</option><option value="same">B</option></select>',true],
          ['requested link','<a href="https://docs.snaga.ai/guide">Guide</a>',true],
          ['unknown tag','<video></video>',false],['unknown attribute','<span onclick="x()">A</span>',false],
          ['attribute placement','<span rows="1">A</span>',false],['input type','<input type="invented">',false],
          ['checked text field','<input type="text" checked>',false],['textarea style','<div style="resize:none">A</div>',false],
          ['duplicate id','<span id="same">A</span><span id="same">B</span>',false],
          ['unrequested link','<a href="https://docs.snaga.ai.other/guide">Guide</a>',false],
          ['credential URL','<a href="https://user@docs.snaga.ai/guide">Guide</a>',false],
          ['number grammar','<progress max=".5"></progress>',false],['label target','<label for="missing">A</label>',false],
          ['unpackaged image','<img src="unpackaged.png" alt="A">',false],
        ];
        for(const [label,body,valid] of cases)check((lintTree(doc(body)).errors.length===0)===valid,`lint: ${label}`);
        for(const text of ['\0','\uD800']){const source=doc('<span>A</span>');source.querySelector('span').textContent=text;check(lintTree(source).errors.length>0,'invalid source string');}
        const large=doc('<select>'+Array.from({length:220},()=>`<option value="${'X'.repeat(384)}">A</option>`).join('')+'</select>');
        check(lintTree(large).errors.some(error=>error.includes('select work budget')),'whole-batch select work validation');
        const project=(body,options={})=>{
          const reference=doc(body),container=document.createElement('div');document.body.append(container);
          const app=createViewDom({container,interactive:false,linkRequested:linkPolicy(),...options}),batches=mockupBatches(reference,'');
          try{app.apply(batches.tree);app.apply(batches.states);}catch(error){app.close();container.remove();throw error;}
          return {reference:reference.getElementById('l0-root'),app,container,root:app.inspectNode(view.wire.root)};
        };
        for(const [label,body,mutate,message] of [
          ['input','<input value="before">',root=>root.querySelector('input').value='wrong','field value'],
          ['textarea','<textarea>one\ntwo</textarea>',root=>root.querySelector('textarea').value='wrong','field value'],
          ['checkbox','<input type="checkbox" checked>',root=>root.querySelector('input').checked=false,'checked state'],
          ['duplicate select','<select><option value="same">A</option><option value="same">B</option></select>',root=>root.querySelector('select').selectedIndex=1,'selected option'],
          ['dialog','<dialog open><span>A</span></dialog>',root=>root.querySelector('dialog').close(),'dialog state'],
        ]){
          const item=project(body);try{
            check(compareMockup(item.reference,item.root,{layout:false}).errors.length===0,`${label}: initial projection`);
            mutate(item.root);check(compareMockup(item.reference,item.root,{layout:false}).errors.some(error=>error.includes(message)),`${label}: state mismatch detection`);
          }finally{item.app.close();item.container.remove();}
        }
        const disabled=project('<select><option disabled>A</option><option disabled>B</option></select>',{interactive:true});
        try{
          check(disabled.app.snapshot().nodes.filter(node=>node.tag==='option').every(node=>!node.optionSelected),'disabled select model');
          const select=disabled.root.querySelector('select'),snapshot=disabled.app.snapshot();
          const id=snapshot.nodes.find(node=>node.tag==='select').id,options=snapshot.nodes.filter(node=>node.tag==='option');
          check(select.selectedIndex===-1,'disabled select DOM follows shared selection contract');
          const events=[];for(const kind of ['input','change'])disabled.root.addEventListener(kind,()=>events.push(kind));
          const apply=(...operations)=>disabled.app.apply(encodeViewBatch(operations));
          apply({name:'remove_attribute',node:options[0].id,attribute:'disabled'});
          check(select.selectedIndex===-1,'enabling an option retains no selection');
          apply({name:'set_attribute',node:options[1].id,attribute:'selected',text:''});
          check(select.selectedIndex===1&&select.value==='B','selected attribute over browser dirty flags');
          apply({name:'set_value',node:id,text:'missing',base_seq:0});check(select.selectedIndex===-1,'unmatched value clears selection');
          apply({name:'create_element',node:10000,tag:'div'},{name:'insert',parent:view.wire.root,node:10000,before:0},{name:'insert',parent:10000,node:id,before:0});
          check(select.selectedIndex===-1,'reinsertion retains explicit none');
          apply({name:'remove_attribute',node:options[1].id,attribute:'selected'});check(select.selectedIndex===0,'attribute reconciliation picks first enabled option');
          const descriptor=Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'selectedIndex');let writes=0;
          Object.defineProperty(select,'selectedIndex',{get(){return descriptor.get.call(this);},set(value){writes++;descriptor.set.call(this,value);}});
          apply();check(writes===0,'empty commit avoids a selectedIndex write');
          check(events.length===0&&disabled.app.snapshot().nodes.find(node=>node.id===id).seq===0,'reconciliation emits no edits');
        }finally{disabled.app.close();disabled.container.remove();}
        const budget=project('<select>'+Array.from({length:20},()=>`<option value="${'X'.repeat(4096)}">A</option>`).join('')+'</select>');
        try{
          const before=JSON.stringify(budget.app.snapshot()),select=budget.root.querySelector('select'),index=select.selectedIndex;
          const id=budget.app.snapshot().nodes.find(node=>node.tag==='select').id;
          let failure;try{budget.app.apply(encodeViewBatch(Array.from({length:128},(_,i)=>({name:'set_value',node:id,text:`missing${i}`,base_seq:0}))));}catch(error){failure=error;}
          check(failure?.rule==='select work budget','official DOM work refusal');
          check(JSON.stringify(budget.app.snapshot())===before&&select.selectedIndex===index,'official DOM work rollback');
        }finally{budget.app.close();budget.container.remove();}
        return checks;
      });
      await page.goto(base+'/uarp-console/mockups/conformance/conformance.html');
      await page.waitForFunction(()=>globalThis.__conformance,undefined,{timeout:45000});
      const conformance=await page.evaluate(()=>globalThis.__conformance);
      assert.equal(conformance.failed,0,`${name}: ${JSON.stringify(conformance.failures)}`);assert.equal(conformance.passed,15);
      assert.deepEqual(errors,[],`${name}: browser errors`);
      assert(!requests.some(url=>url.includes('/varta-web-host/')||new URL(url).origin!==base),`${name}: prototype or ambient request`);
      results.push({browser:name,mockups:5,projections:conformance.passed,regressions:fixtures});
      console.log(`${name}: five mockups, ${conformance.passed} official-host projections and ${fixtures} regression checks passed`);
    }finally{await browser.close();}
  }
  console.log(JSON.stringify({matrix:results}));
}finally{server.closeAllConnections();await new Promise(done=>server.close(done));}
