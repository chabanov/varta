import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createFontNamespace, packagedFamilies, faceDescriptors, FontValueError } from './view-fonts.mjs';
globalThis.__fonts={FontValueError};
test('family names are case-insensitive and isolated, including unknown ambient fonts',()=>{
  const a=createFontNamespace('one',['Brand','Two Words']),b=createFontNamespace('two',['Brand']);
  assert.equal(a.family('Brand'),a.family('bRaNd'));assert.notEqual(a.family('Brand'),b.family('Brand'));
  assert.equal(a.value('Brand, "Two Words", sans-serif'),`${a.family('Brand')}, "${a.family('Two Words')}", sans-serif`);
  assert(!a.value('Arial, serif').includes('Arial'));assert(!a.value('"serif"').includes('serif'));
  assert.throws(()=>createFontNamespace('bad prefix'));
});
test('computed family lists keep quoted names, decode escapes and refuse malformed text',()=>{
  const {FontValueError}=globalThis.__fonts;
  const a=createFontNamespace('wide',['Brand','A"B']);
  assert.equal(a.value('"inherit"'),JSON.stringify(a.family('inherit')));
  assert.equal(a.value('"A\\22 B", serif'),`${JSON.stringify(a.family('A"B'))}, serif`);
  for(const text of ['"open','a b','a,,b',''])if(text)assert.throws(()=>a.value(text),FontValueError,text);
});
test('packaged generic families map to their family and the first family is the default, as in the native shell',()=>{
  const faces=[{key:'Sans',weight:400,style:'normal',generics:[]},{key:'Sans',weight:700,style:'normal',generics:[]},{key:'Sans',weight:400,style:'italic',generics:[]},{key:'Mono',weight:400,style:'normal',generics:['monospace']}];
  const [families,options]=packagedFamilies(faces);
  assert.deepEqual(families,['Sans','Mono']);assert.equal(options.defaultFamily,'Sans');
  const n=createFontNamespace('pack',families,options);
  assert.equal(n.value('monospace'),n.family('Mono'));assert.equal(n.value('MONOSPACE'),n.family('Mono'));
  assert.equal(n.value('Missing, monospace'),`${n.family('Missing')}, ${n.family('Mono')}`);
  // A quoted generic name is a family name, and unmapped generics pass through.
  assert.equal(n.value('"monospace"'),JSON.stringify(n.family('monospace')));assert.equal(n.value('serif'),'serif');
  assert.equal(n.defaultFamily,n.family('Sans'));
  assert.deepEqual(faces.map(faceDescriptors),[{weight:'400',style:'normal'},{weight:'700',style:'normal'},{weight:'400',style:'italic'},{weight:'400',style:'normal'}]);
  // browser.fonts faces carry no descriptors and no default family.
  const [plain,none]=packagedFamilies([{key:'Brand'}]);assert.deepEqual(plain,['Brand']);assert.equal(none.defaultFamily,undefined);
  assert.deepEqual(faceDescriptors({key:'Brand'}),{});assert.equal(createFontNamespace('plain',plain,none).defaultFamily,undefined);
});
test('generic aliases and face descriptors are refused unless exact',()=>{
  assert.throws(()=>packagedFamilies([{key:'A',weight:400,style:'normal',generics:['monospace']},{key:'B',weight:400,style:'normal',generics:['monospace']}]));
  assert.throws(()=>createFontNamespace('bad',['A'],{aliases:new Map([['cursive','A']])}));
  assert.throws(()=>createFontNamespace('bad',['A'],{aliases:new Map([['serif','B']])}));
  assert.throws(()=>createFontNamespace('bad',['A'],{defaultFamily:'B'}));
  for(const face of [{weight:0,style:'normal'},{weight:1001,style:'normal'},{weight:400.5,style:'normal'},{weight:400,style:'oblique'}])assert.throws(()=>faceDescriptors(face),JSON.stringify(face));
});
test('packaged base rules map user-agent monospace elements, make controls inherit and turn synthesis off',async()=>{
  const {registry}=await import('../../../varta-common/browser/registry.mjs');
  const D=registry.view.defaults;
  assert.deepEqual(D.monospaceTags,['pre','code']);assert.deepEqual(D.controlFontTags,['button','input','textarea','select']);
  const faces=[{key:'Sans',weight:400,style:'normal',generics:[]},{key:'Mono',weight:400,style:'normal',generics:['monospace']}];
  const n=createFontNamespace('rules',...packagedFamilies(faces));
  assert.equal(n.rules(D),`${D.rootSelector} { font-family: ${n.family('Sans')}; font-synthesis: none; } :where(pre, code) { font-family: ${n.family('Mono')}; } :where(button, input, textarea, select) { font-family: inherit; font-synthesis: inherit; }`);
  // Without a monospace alias the user-agent monospace elements keep the
  // browser's generic (the documented undeclared-generic divergence).
  const plain=createFontNamespace('rules',...packagedFamilies([faces[0]]));
  assert.equal(plain.rules(D),`${D.rootSelector} { font-family: ${plain.family('Sans')}; font-synthesis: none; } :where(button, input, textarea, select) { font-family: inherit; font-synthesis: inherit; }`);
  // browser.fonts faces and the empty namespace add no rule.
  assert.equal(createFontNamespace('rules',...packagedFamilies([{key:'Brand'}])).rules(D),'');
  assert.equal(createFontNamespace('rules').rules(D),'');
});
