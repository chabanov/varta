import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createViewModel} from './view-model.mjs';
import {encodeViewBatch} from './view-codec.mjs';
import { viewEngine } from "./view-engine.node.mjs";
const create=(node,tag)=>({name:'create_element',node,tag});
const attr=(node,attribute,text)=>({name:'set_attribute',node,attribute,text});
const insert=(parent,node)=>({name:'insert',parent,node,before:0});
const checked=(node,base_seq=0)=>({name:'set_checked',node,checked:1,base_seq});
const commit=(model,...ops)=>model.commit(encodeViewBatch(ops));
const name='Ж'.repeat(512);
function setup(){
  const model=createViewModel({viewEngine}),ops=[create(2,'div'),insert(1,2),create(1000,'div'),insert(1,1000),{name:'set_title',text:'before'}];
  for(let node=3;node<103;node++)ops.push(create(node,'input'),attr(node,'type','radio'),attr(node,'name',name),insert(2,node));
  commit(model,...ops,checked(3));return model;
}
test('radio scans charge repeated checked, type, name and stale writes before effects',()=>{
  for(const kind of ['checked','type','name','stale']){
    const model=setup(),before=model.snapshot();
    const ops=Array.from({length:128},(_,i)=>{
      const node=3+i%2;
      return kind==='type'?attr(node,'type','radio'):kind==='name'?attr(node,'name',name):checked(node,kind==='stale'?1:0);
    });
    assert.throws(()=>commit(model,{name:'set_title',text:'must roll back'},...ops),error=>error.rule==='radio work budget'&&error.operation>0);
    assert.deepEqual(model.snapshot(),before,kind);
    commit(model,...Array.from({length:60},(_,i)=>checked(3+i%2)));
    const nodes=new Map(model.snapshot().nodes.map(n=>[n.id,n]));
    assert.equal(nodes.get(3).checked,false);assert.equal(nodes.get(4).checked,true);assert.equal(nodes.get(4).seq,0);
  }
});
test('radio subtree scans include unchecked members and roll back reparenting',()=>{
  const model=setup();commit(model,{...checked(3),checked:0});const before=model.snapshot();
  assert.throws(()=>commit(model,{name:'set_title',text:'must roll back'},insert(1000,2)),error=>error.rule==='radio work budget'&&error.operation===1);
  assert.deepEqual(model.snapshot(),before);
  commit(model,insert(1000,3));assert.equal(model.snapshot().nodes.find(n=>n.id===3).parent,1000);
});

test('radio scans admit the exact work allowance and reject one additional scan',async()=>{
  const {registry}=await import('../../../varta-common/browser/registry.mjs');
  const {radioWork,radioNodeWork}=registry.view.limits,model=createViewModel({viewEngine});
  commit(model,create(2,'input'),attr(2,'type','radio'),attr(2,'name','X'.repeat(1024-4*radioNodeWork)),insert(1,2));
  const count=radioWork/1024,ops=Array.from({length:count},()=>checked(2));
  commit(model,...ops);const before=model.snapshot();assert.equal(before.nodes.find(n=>n.id===2).checked,true);
  assert.throws(()=>commit(model,...ops,checked(2)),error=>error.rule==='radio work budget'&&error.operation===count);
  assert.deepEqual(model.snapshot(),before);
});
