import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createViewModel} from './view-model.mjs';
import {encodeViewBatch} from './view-codec.mjs';
const create=(node,tag)=>({name:'create_element',node,tag});
const attr=(node,attribute,text)=>({name:'set_attribute',node,attribute,text});
const insert=(parent,node)=>({name:'insert',parent,node,before:0});
const value=(node,text,base_seq=0)=>({name:'set_value',node,text,base_seq});
function option(id,text,label,disabled=false){return [create(id,'option'),{name:'create_text',node:id+100,text:label},insert(id,id+100),attr(id,'value',text),...(disabled?[attr(id,'disabled','')]:[]),insert(2,id)];}
function fixture(){const model=createViewModel();model.commit(encodeViewBatch([create(2,'select'),insert(1,2),...option(3,'a','A',true),...option(4,'same','AA'),...option(5,'same','AAA')]));return model;}
const state=(model,id)=>model.snapshot().nodes.find(n=>n.id===id);
const commit=(model,...ops)=>model.commit(encodeViewBatch(ops));

test('single-select defaults, duplicate choices and equal echoes keep host identity',()=>{
  const m=fixture();assert.equal(state(m,2).value,'same');assert.equal(state(m,4).optionSelected,true);
  m.input(2,'same',false,true,5);assert.equal(state(m,5).optionSelected,true);assert.equal(state(m,2).seq,1);
  commit(m,value(2,'same',1));assert.equal(state(m,5).optionSelected,true);
  commit(m,attr(5,'selected',''),{name:'remove_attribute',node:5,attribute:'selected'});
  assert.equal(state(m,5).optionSelected,true);
  commit(m,value(2,'missing',1));assert.equal(state(m,2).value,'');
  assert(state(m,2).children.every(id=>!state(m,id).optionSelected));
  commit(m,...option(6,'last','AAAA'));assert.equal(state(m,4).optionSelected,true);
  const before=m.snapshot();assert.throws(()=>commit(m,{name:'remove',node:4},{name:'remove',node:999}),/unknown node/);assert.deepEqual(m.snapshot(),before);
});
test('option text/value and reparenting synchronize live select values without edits',()=>{
  const m=fixture();commit(m,{name:'remove_attribute',node:4,attribute:'value'},{name:'set_text',node:104,text:' AA\n A '});
  assert.equal(state(m,2).value,'AA A');assert.equal(state(m,2).seq,0);
  commit(m,create(6,'select'),insert(1,6),insert(6,4));assert.equal(state(m,6).value,'AA A');assert.equal(state(m,2).value,'same');
  commit(m,value(6,'missing'),create(7,'div'),insert(1,7),insert(7,6));
  assert.equal(state(m,6).value,'');assert.equal(state(m,4).optionSelected,false);
});
test('select choice envelopes validate before mutation and tolerate stale removed options',()=>{
  const m=fixture();const before=m.snapshot();
  for(const choice of [NaN,-1,1.5,2**32,'5'])assert.throws(()=>m.syncEvent({kind:'input',target:2,seq:1,value:'same'},choice),/select option envelope/);
  assert.deepEqual(m.snapshot(),before);
  assert.throws(()=>m.syncEvent({kind:'click',target:2},5),/select option envelope/);
  m.syncEvent({kind:'input',target:2,seq:1,value:'same'},5);assert.equal(state(m,5).optionSelected,true);
  commit(m,{name:'remove',node:5});m.syncEvent({kind:'change',target:2,seq:1,value:'same'},5);
  assert.equal(state(m,4).optionSelected,true);
  m.syncEvent({kind:'input',target:2,seq:2,value:''},0);assert.equal(state(m,2).value,'');assert(state(m,2).children.every(id=>!state(m,id).optionSelected));
});
test('derived option values obey the existing bound even when disabled and unselected',()=>{
  const m=fixture(),before=m.snapshot(),part='A'.repeat(16385);
  assert.throws(()=>commit(m,create(6,'option'),attr(6,'disabled',''),{name:'create_text',node:106,text:part},{name:'create_text',node:107,text:part},insert(6,106),insert(6,107),insert(2,6)),/select value budget/);
  assert.deepEqual(m.snapshot(),before);
});
test('unchanged selected attributes preserve live selection and explicit none',()=>{
  const m=fixture();commit(m,attr(4,'selected',''),attr(5,'selected',''),attr(4,'selected',''));
  assert.equal(state(m,5).optionSelected,true);
  commit(m,value(2,'missing'),attr(4,'selected',''),{name:'remove_attribute',node:4,attribute:'disabled'});
  assert.equal(state(m,2).value,'');assert(state(m,2).children.every(id=>!state(m,id).optionSelected));
  commit(m,{name:'remove_attribute',node:4,attribute:'selected'},attr(4,'selected',''));
  assert.equal(state(m,4).optionSelected,true);
});
