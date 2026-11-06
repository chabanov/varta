import {test} from 'node:test';
import assert from 'node:assert/strict';
import {registry} from '../../../varta-common/browser/registry.mjs';
import {createViewModel} from './view-model.mjs';
import {encodeViewBatch} from './view-codec.mjs';
import { viewEngine } from "./view-engine.node.mjs";
const MAX=registry.view.limits.treeDepth;
assert(Number.isInteger(MAX)&&MAX>400,'regenerated tree-depth contract required');
const insert=(parent,node,before=0)=>({name:'insert',parent,node,before});
const create=node=>({name:'create_element',node,tag:'span'});
const commit=(model,...ops)=>model.commit(encodeViewBatch(ops));
function chain(first,count,parent){
 const ops=[];for(let i=0;i<count;i++){const id=first+i;ops.push(create(id));if(i)ops.push(insert(id-1,id));else if(parent!==undefined)ops.push(insert(parent,id));}return ops;
}
function rejected(model,ops,index){
 const before=model.snapshot();assert.throws(()=>commit(model,...ops),e=>e.rule==='tree depth budget'&&e.operation===index);
 assert.deepEqual(model.snapshot(),before);commit(model);assert.deepEqual(model.snapshot(),before);
}
test('tree depth counts component edges, text leaves and inter-batch detached growth',()=>{
 const model=createViewModel({viewEngine});
 commit(model,...chain(2,MAX/2+1));commit(model,...chain(3+MAX/2,MAX/2,2+MAX/2));
 rejected(model,[{name:'set_title',text:'rollback'},insert(1,2)],1);
 commit(model,{name:'remove',node:MAX+2},insert(1,2));
 rejected(model,[{name:'create_text',node:9000,text:'too deep'},insert(MAX+1,9000)],1);
 commit(model,{name:'remove',node:2},{name:'set_title',text:'reclaimed'});
 assert.equal(model.snapshot().title,'reclaimed');
});
test('moving a subtree checks its deepest descendant and restores order after rejection',()=>{
 const model=createViewModel({viewEngine}),parent=1000+MAX/2-1;
 commit(model,...chain(2,MAX/2,1));commit(model,...chain(1000,MAX/2,1));
 commit(model,insert(parent,2));commit(model,create(2000),insert(parent,2000,2));
 rejected(model,[{name:'set_title',text:'rollback'},insert(2000,2)],1);
 commit(model,insert(1,2,1000));assert.equal(model.snapshot().nodes.find(n=>n.id===1).children[0],2);
});
test('an over-deep insertion is rejected even when a later removal would repair it',()=>{
 const model=createViewModel({viewEngine});commit(model,...chain(2,MAX,1));
 rejected(model,[create(9000),insert(MAX+1,9000),{name:'remove',node:9000}],1);
});
