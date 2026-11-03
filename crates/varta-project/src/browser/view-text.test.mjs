import {test} from 'node:test';
import assert from 'node:assert/strict';
import {registry} from '../../../varta-common/browser/registry.mjs';
import {createViewModel} from './view-model.mjs';
import {encodeViewBatch} from './view-codec.mjs';
const {retainedTextBytes,fieldValueReservationBytes,valueBytes}=registry.view.limits;
const create=(node,tag='input')=>({name:'create_element',node,tag});
const text=(node,text)=>({name:'create_text',node,text});
const insert=(parent,node)=>({name:'insert',parent,node,before:0});
const attr=(node,attribute,text)=>({name:'set_attribute',node,attribute,text});
const commit=(model,...ops)=>model.commit(encodeViewBatch(ops));
function nearLimit(){
  const model=createViewModel(),fields=Math.floor(retainedTextBytes/fieldValueReservationBytes),ops=[];
  for(let node=2;node<2+fields;node++)ops.push(create(node),insert(1,node));
  const remaining=retainedTextBytes-fields*fieldValueReservationBytes;
  commit(model,...ops,text(1000,'X'.repeat(remaining)),insert(1,1000));return {model,remaining};
}
function rejected(model,...ops){
  const before=model.snapshot();assert.throws(()=>commit(model,...ops),error=>error.rule==='retained text budget'&&error.operation===undefined);
  assert.deepEqual(model.snapshot(),before);
  commit(model); // Rejection must not poison hidden byte caches for a later no-op.
  assert.deepEqual(model.snapshot(),before);
}
test('retained payload counts every source slot and rollback keeps byte caches private',()=>{
  const {model}=nearLimit();
  for(const op of [{name:'set_title',text:'x'},{name:'set_class',node:2,text:'x'},attr(2,'placeholder','界'),
    {name:'set_style',node:2,property:'color',text:'#abc'},{name:'stylesheet',sheet:7,text:'.x { color:#abc; }'},text(1001,'x')])rejected(model,op);
  commit(model,{name:'set_text',node:1000,text:'X'.repeat(32760)},{name:'set_class',node:2,text:'x'},attr(2,'placeholder','界'),{name:'set_style',node:2,property:'color',text:'#abc'});
  commit(model,{name:'remove_attribute',node:2,attribute:'placeholder'},{name:'set_title',text:'界'});
  rejected(model,{name:'set_title',text:'界x'});
  commit(model,{name:'remove_style',node:2,property:'color'},{name:'set_title',text:'界1234'});
  assert.equal(model.snapshot().nodes.find(n=>n.id===2).attributeBytes,undefined);
  assert.equal(model.snapshot().nodes.find(n=>n.id===2).styleBytes,undefined);
});
test('field capacity covers UTF-8 local edits and stays reserved across type changes',()=>{
  const {model,remaining}=nearLimit();model.input(2,'界'.repeat(valueBytes/2),false);
  rejected(model,{name:'set_title',text:'x'});
  commit(model,{name:'set_text',node:1000,text:'X'.repeat(remaining-8)},attr(2,'type','checkbox'));
  rejected(model,create(1001));
  commit(model,{name:'remove',node:2},create(1001),insert(1,1001),{name:'set_title',text:'12345678'});
});
test('static payload accumulation is bounded across batches including detached nodes',()=>{
  const model=createViewModel(),unit='X'.repeat(valueBytes/2),count=retainedTextBytes/unit.length;
  for(let start=0;start<count;start+=3){const ops=[];for(let i=start;i<Math.min(start+3,count);i++)ops.push(text(i+2,unit),insert(1,i+2));commit(model,...ops);}
  rejected(model,text(1000,'x'));
  commit(model,{name:'set_text',node:2,text:''},text(1000,unit));
});
test('a value written before a checkable type change retains its capacity',()=>{
  const model=createViewModel(),ops=[];for(let node=2;node<87;node++)ops.push(create(node));
  commit(model,...ops,{name:'set_value',node:2,text:'kept',base_seq:0},attr(2,'type','checkbox'));
  rejected(model,create(1000));commit(model,{name:'remove',node:2},create(1000));
});

test('synchronized field records cannot bypass the reserved per-value bound',()=>{
  const {model}=nearLimit(),before=model.snapshot();
  for(const value of ['X'.repeat(valueBytes/2+1),'\0','\uD800',42,null]){
    assert.throws(()=>model.syncEvent({kind:'input',target:2,seq:99,value}),/input value/);
    assert.deepEqual(model.snapshot(),before);
  }
});
