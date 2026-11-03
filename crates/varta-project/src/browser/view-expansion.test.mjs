import {test} from 'node:test';
import assert from 'node:assert/strict';
import {registry} from '../../../varta-common/browser/registry.mjs';
import {compileExpansionValue,createExpansionScope} from './view-expansion.mjs';
import {compileVariableValue,createVariableScope} from './view-variables.mjs';
import {createViewModel} from './view-model.mjs';
import {encodeViewBatch} from './view-codec.mjs';
const V=registry.view,CAP=V.limits.valueBytes/2;
function candidates(definitions){return new Map(Object.entries(definitions).map(([name,values])=>[name,values.map(compileExpansionValue)]));}
function setup(count,extra=''){
 const literal='"'+'a'.repeat(CAP-2)+'"';
 const ops=[{name:'stylesheet',sheet:1,text:`:root{--big:${literal};${extra}}.a{font-family:var(--big)}`}];
 for(let node=2;node<count+2;node++)ops.push({name:'create_element',node,tag:'button'},{name:'set_class',node,text:'a'},{name:'insert',node,parent:1,before:0});
 return ops;
}
const commit=(model,ops=[])=>model.commit(encodeViewBatch(ops));
function reject(model,ops,operation){
 const before=model.snapshot();assert.throws(()=>commit(model,ops),error=>error.rule==='expanded value budget'&&(error.operation??null)===operation);
 assert.deepEqual(model.snapshot(),before);commit(model);assert.deepEqual(model.snapshot(),before);
}
test('expanded value allowance is exact and one UTF16 code unit rejects atomically',()=>{
 const model=createViewModel();commit(model,setup(127));
 reject(model,[{name:'set_style',node:2,property:'width',text:'0'}],null);
 commit(model,[{name:'set_style',node:2,property:'width',text:'0'},{name:'remove_style',node:2,property:'width'}]);
 commit(model,[{name:'remove',node:2},{name:'set_style',node:3,property:'width',text:'0'}]);
});
test('command reads share expanded allowance and reject transient excess before a repair',()=>{
 const model=createViewModel(),focus={name:'command',node:2,command:'focus',argument:0};commit(model,setup(63));
 commit(model,[focus]);reject(model,[focus,focus],null);reject(model,[focus,focus,focus],2);
 const full=createViewModel();commit(full,setup(127));
 reject(full,[{name:'set_style',node:2,property:'width',text:'0'},focus,{name:'remove_style',node:2,property:'width'}],1);
});
test('pure aliases share fresh literals and scope choices bound cycles without expanding them',()=>{
 const scope=createExpansionScope();let result=scope.extend(candidates({'--a':['var(--b,var(--c))'],'--b':['var(--a)'],'--c':['var(--b,#abc)']}));
 assert.equal(result.units,4);assert.equal(scope.upper(compileExpansionValue('var(--a)')),4);
 result=scope.extend(candidates({'--x':['var(--y)x'],'--y':['var(--x)']}));
 assert.equal(result.units,CAP);assert.equal(scope.upper(compileExpansionValue('var(--y)')),CAP);
 result=scope.extend(candidates({'--empty':['var(--empty)var(--empty)']}));assert.equal(result.units,0);
 const aliases=Array.from({length:100},(_,i)=>`--alias${i}:var(--${i?'alias'+(i-1):'big'});`).join('');
 const model=createViewModel();commit(model,setup(127,aliases));
});
test('inactive selector/media candidates and detached scopes retain their possible capacity',()=>{
 const literal='"'+'a'.repeat(CAP-2)+'"',model=createViewModel();commit(model,setup(127));
 commit(model,[{name:'stylesheet',sheet:1,text:`@media (min-width:999999px){:root{--big:${literal}}}.a.never:hover{font-family:var(--big)}`}]);
 reject(model,[{name:'set_style',node:2,property:'width',text:'0'}],null);
 const detached=createViewModel();commit(detached,setup(126));
 commit(detached,[{name:'stylesheet',sheet:1,text:`:root,.seed{--big:${literal}}.a{font-family:var(--big)}`}]);
 const seed=[{name:'create_element',node:9000,tag:'div'},{name:'set_class',node:9000,text:'seed'}];
 reject(detached,[...seed,{name:'create_element',node:9001,tag:'button'},{name:'set_class',node:9001,text:'a'},{name:'insert',node:9001,parent:9000,before:0}],null);
 commit(detached,seed);commit(detached,[{name:'insert',node:9000,parent:1,before:0}]);
});
test('standard literals larger than the custom-value cap still consume their full text',()=>{
 const model=createViewModel(),text='.a{font-family:"'+'a'.repeat(CAP)+'"}';
 const ops=[{name:'stylesheet',sheet:1,text}];
 for(let node=2;node<129;node++)ops.push({name:'create_element',node,tag:'button'},{name:'set_class',node,text:'a'},{name:'insert',node,parent:1,before:0});
 commit(model,ops);
 reject(model,[{name:'create_element',node:9000,tag:'button'},{name:'set_class',node:9000,text:'a'},{name:'insert',node:9000,parent:1,before:0}],null);
});
function worlds(options){
 let result=[{}];for(const [name,variants] of Object.entries(options)){const next=[];for(const world of result)for(const value of [undefined,...variants])next.push(value===undefined?{...world}:{...world,[name]:value});result=next;}return result;
}
test('candidate bounds dominate actual fresh text and substitutions across inherited worlds',()=>{
 const families=[
  [{'--a':['#123','"😀"'],'--b':['var(--a)','var(--missing,#abcdef)'],'--c':['var(--b)','initial']},{'--a':['inherit','#456'],'--b':['var(--a)','var(--missing,)'],'--c':['var(--b,#abc)','var(--a)']}],
  [{'--a':['var(--b)','#abc'],'--b':['var(--a)','var(--c)'],'--c':['x','var(--a)y']},{'--a':['unset','var(--c)'],'--b':['var(--a)var(--c)','var(--missing,)'],'--c':['var(--b,#abc)','"var(--absent)"']}],
 ];
 const expressions=['var(--a)','var(--b)','var(--c)','var(--a,var(--c,q))','var(--a)var(--b)','var(--b,1px) 2px','"var(--missing)"'];
 const compile=world=>new Map(Object.entries(world).map(([name,text])=>[name,compileVariableValue(text)]));
 for(const [parent,child] of families){
  const potential=createExpansionScope(),root=potential.extend(candidates(parent)),descendant=potential.extend(candidates(child));
  for(const first of worlds(parent))for(const second of worlds(child)){
   const actual=createVariableScope(),a=actual.extend(compile(first));
   let fresh=0;for(let id=a.nodes;id<a.end;id++)fresh+=actual.materialize(id).length;assert(fresh<=root.units);
   const b=actual.extend(compile(second));fresh=0;for(let id=b.nodes;id<b.end;id++)fresh+=actual.materialize(id).length;assert(fresh<=descendant.units);
   for(const expression of expressions){const value=actual.substitute(expression);if(value!==null)assert(value.length<=potential.upper(compileExpansionValue(expression)),JSON.stringify({first,second,expression,value}));}
  }
 }
});
