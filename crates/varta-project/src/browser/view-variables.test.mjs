import {test} from 'node:test';
import assert from 'node:assert/strict';
import {registry} from '../../../varta-common/browser/registry.mjs';
import {checkStylesheet,checkStyleDeclaration} from './view-css.mjs';
import {createFontNamespace} from './view-fonts.mjs';
const V=registry.view;
test('variable fallbacks admit nested component lists and defer numeric notes',()=>{
 for(const [property,value] of [['color','var(--missing, rgb(10,20,30))'],['color','var(--a,var(--b,#abc))'],
 ['padding','var(--gap,1px 2px)'],['padding','var(--gap,) 2px'],['width','var(--none,)auto'],['padding','var(--a,)var(--b,)var(--c,)var(--d,)var(--e,) 2px'],['padding','var(--gap,-1px)'],['opacity','VAR(--alpha,2)']])
  assert.doesNotThrow(()=>checkStyleDeclaration(V,property,value));
 for(const [property,value] of [['padding','-1px'],['opacity','2'],['color','var(--a,var(no-name))']])
  assert.throws(()=>checkStyleDeclaration(V,property,value));
});
test('custom property syntax rejects unbalanced functions, empty hashes and malformed nested references',()=>{
 for(const value of ['rgb(1,2,3','rgb(1,2,3))','#','var(no-name)','f(var(no-name))'])
  assert(checkStylesheet(`:root{--x:${value}}`,V).errors.length,value);
 for(const value of ['rgb(var(--r),20,30)','var(--a,var(--b,#abc))','"var(no-name)"','VAR(--Tone)','var(--a,)'])
  assert.deepEqual(checkStylesheet(`:root{--x:${value}}`,V).errors,[],value);
});
test('case-insensitive font variable functions keep nested fallbacks in the private namespace',()=>{
 const font=createFontNamespace('variables',['Known']);
 const value=font.value('VAR(--first, VaR(--second, "Known"))');
 assert(value.includes(font.variable('--first')));assert(value.includes(font.variable('--second')));
 assert(value.includes('varta_font_variables_0'));assert(!value.includes('Known'));
});

test('variable deferral preserves the property whitelist and forbidden-source policy',()=>{
 assert.throws(()=>checkStyleDeclaration(V,'unknown-property','var(--x)'));
 assert(checkStylesheet('.a{unknown-property:var(--x)}',V).errors.length);
 assert.throws(()=>checkStyleDeclaration(V,'color','var(--x,url(evil))'));
});
