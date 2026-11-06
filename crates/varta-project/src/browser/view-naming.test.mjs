import {test} from 'node:test';
import assert from 'node:assert/strict';
import {registry} from '../../../varta-common/browser/registry.mjs';
import {createViewModel} from './view-model.mjs';
import {encodeViewBatch} from './view-codec.mjs';

import { viewEngine } from "./view-engine.node.mjs";
const commit=(model,ops=[])=>model.commit(encodeViewBatch(ops));
function setup(attached=true){
 const ops=[{name:'create_element',node:2,tag:'button'},{name:'set_attribute',node:2,attribute:'aria-hidden',text:'true'},
  {name:'set_attribute',node:2,attribute:'aria-labelledby',text:Array(255).fill('label').join(' ')},
  {name:'create_element',node:3,tag:'div'},{name:'set_attribute',node:3,attribute:'id',text:'label'},
  {name:'create_text',node:4,text:'é'.repeat(62)+'a'.repeat(32768-62)},{name:'insert',node:4,parent:3,before:0},
  {name:'set_attribute',node:2,attribute:'aria-label',text:'x'.repeat(1579)}];
 if(attached)ops.push({name:'insert',node:2,parent:1,before:0},{name:'insert',node:3,parent:1,before:0});
 assert.equal(79+60*255+255*(32768+62)+1579,registry.view.limits.namingWork);return ops;
}
function reject(model,ops){const before=model.snapshot();assert.throws(()=>commit(model,ops),e=>e.rule==='accessible name work budget'&&e.operation===undefined);assert.deepEqual(model.snapshot(),before);commit(model);}
const padding=size=>({name:'set_attribute',node:2,attribute:'aria-label',text:'x'.repeat(size)});
test('naming inventory admits the exact UTF8 boundary and rolls back a single byte excess',()=>{
 const model=createViewModel({viewEngine});commit(model,setup());reject(model,[{name:'set_title',text:'rollback'},padding(1580)]);
 commit(model,[padding(1580),padding(1579)]);reject(model,[{name:'command',node:2,command:'focus',argument:0},padding(1580)]);
 commit(model,[{name:'remove_attribute',node:2,attribute:'aria-labelledby'},padding(1580)]);
});
test('detached, hidden and precedence alternatives retain potential naming work',()=>{
 const model=createViewModel({viewEngine});commit(model,setup(false));reject(model,[padding(1580)]);
 commit(model,[{name:'insert',node:2,parent:1,before:0},{name:'insert',node:3,parent:1,before:0}]);
 commit(model,[{name:'set_attribute',node:3,attribute:'aria-hidden',text:'true'}]);
 reject(model,[padding(1580)]);
 commit(model,[{name:'remove_attribute',node:3,attribute:'id'},padding(1580)]);
 reject(model,[{name:'set_attribute',node:3,attribute:'id',text:'label'}]);
 commit(model,[padding(1579),{name:'set_attribute',node:3,attribute:'id',text:'label'}]);
});
test('embedded field alternatives reserve live capacity independently of local input',()=>{
 const model=createViewModel({viewEngine});commit(model,[{name:'create_element',node:2,tag:'button'},
  {name:'set_attribute',node:2,attribute:'aria-hidden',text:'true'},{name:'set_attribute',node:2,attribute:'aria-labelledby',text:Array(100).fill('field').join(' ')},
  {name:'create_element',node:3,tag:'input'},{name:'set_attribute',node:3,attribute:'type',text:'checkbox'},{name:'set_attribute',node:3,attribute:'id',text:'field'}]);
 reject(model,[{name:'set_attribute',node:3,attribute:'type',text:'password'}]);
 commit(model,[{name:'set_attribute',node:2,attribute:'aria-labelledby',text:'field'},{name:'set_attribute',node:3,attribute:'type',text:'password'}]);
 model.input(3,'😀',false,true);
 reject(model,[{name:'set_attribute',node:2,attribute:'aria-labelledby',text:Array(100).fill('field').join(' ')}]);
});
test('host label alternatives include referenced traversal work with bounded cycles',()=>{
 const model=createViewModel({viewEngine});const ops=setup(false).filter(op=>op.node!==2&&op.node!==5);
 reject(model,[...ops,{name:'create_element',node:6,tag:'input'},{name:'set_attribute',node:6,attribute:'id',text:'field'},
  {name:'create_element',node:7,tag:'label'},{name:'set_attribute',node:7,attribute:'for',text:'field'},
  {name:'set_attribute',node:7,attribute:'aria-labelledby',text:Array(300).fill('label').join(' ')}]);
 commit(model,[{name:'create_element',node:2,tag:'button'},{name:'set_attribute',node:2,attribute:'id',text:'a'},
  {name:'set_attribute',node:2,attribute:'aria-labelledby',text:'b'},{name:'create_element',node:3,tag:'button'},
  {name:'set_attribute',node:3,attribute:'id',text:'b'},{name:'set_attribute',node:3,attribute:'aria-labelledby',text:'a'}]);
});
test('author-only roles do not charge content they cannot read as a root name',()=>{
 const model=createViewModel({viewEngine}),ops=[];
 for(let node=2;node<=registry.view.limits.treeDepth;node++)ops.push({name:'create_element',node,tag:'span'},
  {name:'set_attribute',node,attribute:'role',text:'list'},{name:'insert',node,parent:node-1,before:0});
 const leaf=registry.view.limits.treeDepth+1;
 ops.push({name:'create_text',node:leaf,text:'a'.repeat(registry.view.limits.valueBytes/2)},
  {name:'insert',node:leaf,parent:leaf-1,before:0});commit(model,ops);
 reject(model,ops.filter(op=>op.name==='set_attribute').map(op=>({...op,text:'cell'})));
 commit(model,[{name:'set_attribute',node:2,attribute:'role',text:'cell'}]);
});
