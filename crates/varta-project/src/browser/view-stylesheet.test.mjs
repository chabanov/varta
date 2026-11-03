import {spawnSync} from 'node:child_process';
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {registry} from '../../../varta-common/browser/registry.mjs';
import {checkStylesheet,StylesheetWorkError} from './view-css.mjs';
import {createViewModel} from './view-model.mjs';
import {encodeViewBatch} from './view-codec.mjs';
const V=registry.view;
const source=(conditions,rules)=>`@media ${Array(conditions).fill('(min-width:0px)').join(' and ')}{${'.a{}'.repeat(rules)}}`;
const sheet=(sheet,text)=>({name:'stylesheet',sheet,text});
const commit=(m,...ops)=>m.commit(encodeViewBatch(ops));
function rejected(m,ops,index){
 const before=m.snapshot();assert.throws(()=>commit(m,...ops),error=>error.rule==='stylesheet work budget'&&error.operation===index);
 assert.deepEqual(m.snapshot(),before);commit(m);assert.deepEqual(m.snapshot(),before);
}
test('parsed stylesheet cost admits the exact allowance and limits media expansion',()=>{
 const css='.a{}',parsed=checkStylesheet(css,V);
 assert.deepEqual(parsed.errors,[]);assert.equal(parsed.work,css.length+3*V.limits.stylesheetNodeWork+1);
 assert.equal(checkStylesheet(css,V,{workLimit:parsed.work}).work,parsed.work);
 assert.throws(()=>checkStylesheet(css,V,{workLimit:parsed.work-1}),StylesheetWorkError);
 assert.throws(()=>checkStylesheet(source(500,1100),V),StylesheetWorkError);
});
test('installed stylesheet work is aggregate and rollback/replacement reclaim counters',()=>{
 const m=createViewModel(),css=source(500,200);
 for(let id=1;id<=5;id++)commit(m,sheet(id,css));
 rejected(m,[{name:'set_title',text:'after'},sheet(6,css)],1);
 commit(m,sheet(1,''),sheet(6,css));
 commit(m,sheet(6,'.a { color:#abc }'));commit(m,sheet(7,css));
 rejected(m,[sheet(8,css)],0);
});
test('repeated replacement shares a transaction parsing allowance',()=>{
 const m=createViewModel(),css=source(500,200);commit(m,sheet(1,css));
 rejected(m,Array.from({length:6},()=>sheet(1,css)),5);
 commit(m,sheet(1,'.a { color:#abc }'));
});

test('keyword-only property grammars admit a whole var component',()=>{
 assert.deepEqual(checkStylesheet(':root { --mode:none } .a { display:var(--mode) }',V).errors,[]);
 assert.deepEqual(checkStylesheet('.a { display:var(--mode, none) }',V).errors,[]);
 assert.deepEqual(checkStylesheet('.a { display:var(--mode,) }',V).errors,[]);
});

test('empty declaration placement matches the native stylesheet grammar',()=>{
 for(const css of ['.a{;}','.a{;color:#abc}','.a{color:#abc;;}'])assert(checkStylesheet(css,V).errors.length,css);
 for(const css of ['.a{}','.a{color:#abc;}'])assert.deepEqual(checkStylesheet(css,V).errors,[],css);
});

test('ambiguous repeated variable tracks reject an invalid tail without exponential backtracking',()=>{
 const module=new URL('./view-css.mjs',import.meta.url).href,registryModule=new URL('../../../varta-common/browser/registry.mjs',import.meta.url).href;
 const code=`import {createValueMatcher} from ${JSON.stringify(module)};import {registry} from ${JSON.stringify(registryModule)};
 const check=createValueMatcher(Object.fromEntries(registry.view.css.map(row=>[row.name,row.grammar])));
 const valid='var(--x) '.repeat(24).trim();check('grid-template-columns',valid);
 let rejected=false;try{check('grid-template-columns',valid+' bogus');}catch{rejected=true;}
 if(!rejected)throw Error('invalid tail admitted');console.log('checked');`;
 const result=spawnSync(process.execPath,['--input-type=module','-e',code],{encoding:'utf8',timeout:10000});
 assert.equal(result.error,undefined);assert.equal(result.status,0,result.stderr);assert.equal(result.stdout.trim(),'checked');
});
