import {test} from 'node:test';
import assert from 'node:assert/strict';
import {registry} from '../../../varta-common/browser/registry.mjs';
import {createViewModel} from './view-model.mjs';
import {checkStylesheet} from './view-css.mjs';
import {encodeViewBatch} from './view-codec.mjs';
const V=registry.view,LIMIT=V.limits.cascadeWork,W=V.limits.stylesheetNodeWork;
assert(Number.isInteger(LIMIT),'regenerated cascade contract required');
const sheet=(text,id=1)=>({name:'stylesheet',sheet:id,text});
const focus=()=>({name:'command',node:2,command:'focus',argument:0});
const commit=(model,...ops)=>model.commit(encodeViewBatch(ops));
function setup(count,source,classes='a'){
 const ops=[sheet(source)];for(let id=2;id<count+2;id++)ops.push({name:'create_element',node:id,tag:'button'},
 {name:'set_class',node:id,text:classes},{name:'insert',parent:1,node:id,before:0});return ops;
}
function work(count,source,classes='a'){return W+count*(W+classes.length+checkStylesheet(source,V).rules.reduce((n,r)=>n+r.work,0));}
function rejected(model,ops,operation){
 const before=model.snapshot();assert.throws(()=>commit(model,...ops),e=>e.rule==='cascade work budget'&&e.operation===operation);
 assert.deepEqual(model.snapshot(),before);commit(model);assert.deepEqual(model.snapshot(),before);
}
test('exact potential inventory limit rejects one extra byte and removal reclaims capacity',()=>{
 const source='.a{}'.repeat(1337),count=128,classes='a'+' '.repeat(LIMIT-work(count,source)),m=createViewModel();
 commit(m,...setup(count,source),{name:'set_class',node:2,text:classes});
 rejected(m,[{name:'set_title',text:'rollback'},{name:'set_class',node:2,text:classes+' '}],undefined);
 commit(m,{name:'remove',node:2},{name:'set_class',node:3,text:classes+' '});
});
test('candidate rules deduplicate anchors and failed sheet changes cannot poison private indices',()=>{
 const source='.a,.a.b,.b{}'.repeat(496),count=128,classes='a a b',m=createViewModel();
 commit(m,...setup(count,source,classes),{name:'set_class',node:2,text:classes+' '.repeat(LIMIT-work(count,source,classes))});
 rejected(m,[sheet(source+'.a{}')],undefined);
 commit(m,sheet(''),sheet('.unused{color:#abc}',2));commit(m,sheet('.a{color:#123}'));
});
test('variable lookup includes ancestor candidates and detached components cannot evade inventory',()=>{
 const source='.a{color:var(--tone)}'.repeat(100),m=createViewModel(),chain=[];
 commit(m,...setup(64,source),sheet(':root{--tone:#abc}',2));
 for(let id=3;id<66;id++)chain.push({name:'insert',parent:id-1,node:id,before:0});
 rejected(m,chain,undefined);commit(m,sheet('',2),...chain);
 rejected(m,[sheet(':root{--tone:#abc}',2)],undefined);
 const detached=[sheet('.a{}'.repeat(1338))];for(let id=2;id<130;id++)detached.push({name:'create_element',node:id,tag:'span'},{name:'set_class',node:id,text:'a'});
 rejected(createViewModel(),detached,undefined);
});
test('equal focus reads and the final inventory share a transaction allowance',()=>{
 const source='.a{}'.repeat(100),m=createViewModel(),reads=Math.floor(LIMIT/work(64,source));commit(m,...setup(64,source));
 commit(m,...Array.from({length:reads-1},focus));
 rejected(m,Array.from({length:reads},focus),undefined);
 rejected(m,Array.from({length:reads+1},focus),reads);
});
test('transient excess is reclaimable unless a command first reads its cascade',()=>{
 const source='.a{}'.repeat(1337),expanded=source+'.a{}',m=createViewModel();commit(m,...setup(128,source));
 rejected(m,[sheet(expanded),focus(),sheet('')],1);commit(m,sheet(expanded),sheet(''));
});
test('inactive media and unmatched additional selector classes do not hide potential work',()=>{
 const css='@media (min-width:999999px){'+'.a.absent:checked{}'.repeat(1338)+'}',m=createViewModel();
 rejected(m,setup(128,css),undefined);
 commit(m,...setup(128,css,'unrelated'));commit(m,sheet(''));
});
