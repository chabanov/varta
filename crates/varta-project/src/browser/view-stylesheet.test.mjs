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
// Linearity is checked by counting reads of the input, not by wall time, so the
// verdict does not depend on host speed or load. Per input code unit the scan makes
// at most one loop length check, three delimiter tests and one slice of its run,
// plus at most one failed quote search over it (only the last quote of a text can
// be unclosed); the length check that ends the loop is the one fixed read.
const ROOT_SCAN_READS_PER_UNIT=6,ROOT_SCAN_FIXED_READS=1;
const INDEX_KEY=/^(0|[1-9][0-9]*)$/,BLOCK_OPEN='{';
class RootScanMeterError extends Error{}
// Runs `rewrite` on a metered view of `text`. The view admits only `length`, index
// reads, `indexOf` and `slice`, charging the code units each one reads. Any other
// access (for example a regex `replace`, whose work cannot be counted) throws at
// once, and so does the first read past the linear bound, so a superlinear rewrite
// fails after linear work instead of running to completion.
function meteredRewrite(text,rewrite=rewriteRootSelector){
 const limit=ROOT_SCAN_READS_PER_UNIT*text.length+ROOT_SCAN_FIXED_READS;
 let reads=0;
 const charge=units=>{reads+=units;if(reads>limit)throw new RootScanMeterError(`rewrite read over ${limit} code units of a ${text.length}-unit input`);};
 const span=(from,to)=>Math.max(0,to-from);
 const view=new Proxy(new String(text),{get(_target,key){
  if(key==='length'){charge(1);return text.length;}
  if(typeof key==='string'&&INDEX_KEY.test(key)){charge(1);return text[key];}
  if(key==='indexOf')return(search,from)=>{const found=text.indexOf(search,from);charge(found<0?span(from,text.length):span(from,found+search.length));return found;};
  if(key==='slice')return(start,end)=>{const out=text.slice(start,end);charge(out.length);return out;};
  throw new RootScanMeterError(`rewrite made an uncounted access to ${String(key)}`);
 }});
 return rewrite(view,ROOT_TARGET);
}
test('root rewriting reads every short piece sequence within the linear bound',()=>{
 let inputs=[''];
 for(let length=1;length<=ROOT_PIECE_COUNT;length++){
  inputs=inputs.flatMap(prefix=>ROOT_PIECES.map(piece=>prefix+piece));
  for(const text of inputs)assert.equal(meteredRewrite(text),regexRootRewrite(text,ROOT_TARGET),JSON.stringify(text));
 }
});
// One brace- and quote-free run of the whole stylesheet byte limit.
const ROOT_HEAD=':root ';
const maximalRun=()=>ROOT_HEAD+'x'.repeat(V.limits.stylesheetBytes-ROOT_HEAD.length);
test('the read meter stops the replaced regex and a quadratic scan after linear work',()=>{
 const text=maximalRun();
 assert.throws(()=>meteredRewrite(text,regexRootRewrite),/uncounted access to replace/);
 // Rescans the rest of the text from every position, as the regex backtracked.
 const quadratic=(input,selector)=>{
  for(let at=0;at<input.length;at++){let end=at;while(end<input.length&&input[end]!==BLOCK_OPEN)end+=1;}
  return rewriteRootSelector(input,selector);
 };
 assert.throws(()=>meteredRewrite(text,quadratic),RootScanMeterError);
});
test('root rewriting stays linear on the oversized custom-property oracle input',()=>{
 // Same shape as browser_contract_probe's variableCases "oversized-custom".
 const run='x'.repeat(V.limits.valueBytes/2+1),css=`:root{--large:${run}}.target{color:var(--large,#abc)}`;
 assert.equal(meteredRewrite(css),`${ROOT_TARGET}{--large:${run}}.target{color:var(--large,#abc)}`);
});
test('root rewriting stays linear on an accepted stylesheet of the maximal byte size',()=>{
 const open=':root{',close='}',declarations=[];
 let length=open.length+close.length;
 for(let index=0;;index++){const declaration=`--v${index}:x;`;if(length+declaration.length>V.limits.stylesheetBytes)break;declarations.push(declaration);length+=declaration.length;}
 const body=declarations.join('')+' '.repeat(V.limits.stylesheetBytes-length),css=open+body+close;
 assert.equal(css.length,V.limits.stylesheetBytes);
 assert.deepEqual(checkStylesheet(css,V).errors,[]);
 assert.equal(meteredRewrite(css),`${ROOT_TARGET}{${body}}`);
 // A brace- and quote-free run of the whole limit, then one after an unclosed quote.
 const bare=maximalRun();
 assert.equal(meteredRewrite(bare),bare);
 const unclosedQuote='"',fill='x'.repeat(V.limits.stylesheetBytes-unclosedQuote.length-ROOT_HEAD.length-BLOCK_OPEN.length);
 const unclosed=`${unclosedQuote}${ROOT_HEAD}${fill}${BLOCK_OPEN}`;
 assert.equal(unclosed.length,V.limits.stylesheetBytes);
 assert.equal(meteredRewrite(unclosed),`${unclosedQuote}${ROOT_TARGET} ${fill}${BLOCK_OPEN}`);
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
