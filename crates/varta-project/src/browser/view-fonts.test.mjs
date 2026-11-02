import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createFontNamespace } from './view-fonts.mjs';
test('family names are case-insensitive and isolated, including unknown ambient fonts',()=>{
  const a=createFontNamespace('one',['Brand','Two Words']),b=createFontNamespace('two',['Brand']);
  assert.equal(a.family('Brand'),a.family('bRaNd'));assert.notEqual(a.family('Brand'),b.family('Brand'));
  assert.equal(a.value('Brand, "Two Words", sans-serif'),`${a.family('Brand')} , "${a.family('Two Words')}" , sans-serif`);
  assert(!a.value('Arial, serif').includes('Arial'));assert(!a.value('"serif"').includes('serif'));
  assert.equal(a.value('inherit'),'inherit');assert.throws(()=>createFontNamespace('bad prefix'));
});
test('font variables have parallel names and keep other declaration values unchanged',()=>{
  const n=createFontNamespace('scope',['red']);
  assert.equal(n.value('var(--font, red)'),`var(${n.variable('--font')} , ${n.family('red')})`);
  const values=new Map([['--font','red'],['--nested','var(--font, serif)'],['color','var(--font)'],['font-family','var(--nested, monospace)']]);
  const style={*[Symbol.iterator](){yield* values.keys();},getPropertyValue:(key)=>values.get(key),setProperty:(key,value)=>values.set(key,value)};
  n.sheet([{cssRules:[{style}]}]);
  assert.equal(values.get('--font'),'red');assert.equal(values.get('color'),'var(--font)');
  assert.equal(values.get(n.variable('--font')),n.family('red'));
  assert.equal(values.get(n.variable('--nested')),`var(${n.variable('--font')} , serif)`);
  assert.equal(values.get('font-family'),`var(${n.variable('--nested')} , monospace)`);
  assert.throws(()=>n.value('var(123)'));
});
