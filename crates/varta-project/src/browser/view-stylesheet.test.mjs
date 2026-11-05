import {spawnSync} from 'node:child_process';
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {registry} from '../../../varta-common/browser/registry.mjs';
import {checkStylesheet,StylesheetWorkError,rewriteRootSelector} from './view-css.mjs';
import {createViewModel} from './view-model.mjs';
import {encodeViewBatch} from './view-codec.mjs';
const V=registry.view;
test('root rewriting preserves quoted custom text and changes only selector headers',()=>{
 const css=':root{--font:":root"}.a{font-family:var(--font)}@media (min-width:0px){:root{--other:":root"}}';
 assert.equal(rewriteRootSelector(css,'[data-root]'),'[data-root]{--font:":root"}.a{font-family:var(--font)}@media (min-width:0px){[data-root]{--other:":root"}}');
 assert.equal(rewriteRootSelector(':root{--font:":root{"}','[data-root]'),'[data-root]{--font:":root{"}');
});
// The backtracking regex the linear scanner replaced; it is the reference for
// short inputs only, since its cost is quadratic in a run without braces/quotes.
const regexRootRewrite=(text,selector)=>text.replace(/"[^"]*"|[^{}"]+(?=\{)/g,header=>header.startsWith('"')?header:header.replace(/:root(?![A-Za-z0-9_-])/g,selector));
const ROOT_PIECES=['{','}','"',':root',':rootx','a',' '],ROOT_PIECE_COUNT=6,ROOT_TARGET='[data-root]';
test('linear root rewriting matches the regex rewrite on every short piece sequence',()=>{
 let inputs=[''];
 for(let length=1;length<=ROOT_PIECE_COUNT;length++){
  inputs=inputs.flatMap(prefix=>ROOT_PIECES.map(piece=>prefix+piece));
  for(const text of inputs)assert.equal(rewriteRootSelector(text,ROOT_TARGET),regexRootRewrite(text,ROOT_TARGET),JSON.stringify(text));
 }
});
// Generous wall bound for one linear pass over a maximal stylesheet; the replaced
// regex needed seconds for a 32 KiB run and minutes at the stylesheet byte limit.
const LINEAR_REWRITE_WALL_MS=2000;
function timedRewrite(text){
 const start=performance.now(),out=rewriteRootSelector(text,ROOT_TARGET),elapsed=performance.now()-start;
 assert.ok(elapsed<LINEAR_REWRITE_WALL_MS,`rewriteRootSelector took ${elapsed} ms for ${text.length} code units`);
 return out;
}
test('root rewriting stays linear on the oversized custom-property oracle input',()=>{
 // Same shape as browser_contract_probe's variableCases "oversized-custom".
 const run='x'.repeat(V.limits.valueBytes/2+1),css=`:root{--large:${run}}.target{color:var(--large,#abc)}`;
 assert.equal(timedRewrite(css),`${ROOT_TARGET}{--large:${run}}.target{color:var(--large,#abc)}`);
});
test('root rewriting stays linear on an accepted stylesheet of the maximal byte size',()=>{
 const open=':root{',close='}',declarations=[];
 let length=open.length+close.length;
 for(let index=0;;index++){const declaration=`--v${index}:x;`;if(length+declaration.length>V.limits.stylesheetBytes)break;declarations.push(declaration);length+=declaration.length;}
 const body=declarations.join('')+' '.repeat(V.limits.stylesheetBytes-length),css=open+body+close;
 assert.equal(css.length,V.limits.stylesheetBytes);
 assert.deepEqual(checkStylesheet(css,V).errors,[]);
 assert.equal(timedRewrite(css),`${ROOT_TARGET}{${body}}`);
 // A brace- and quote-free run of the whole limit, then one after an unclosed quote.
 const head=':root ',fill=count=>'x'.repeat(V.limits.stylesheetBytes-head.length-count);
 const bare=head+fill(0);
 assert.equal(timedRewrite(bare),bare);
 const unclosed=`"${head}${fill(2)}{`;
 assert.equal(unclosed.length,V.limits.stylesheetBytes);
 assert.equal(timedRewrite(unclosed),`"${ROOT_TARGET} ${fill(2)}{`);
});
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
