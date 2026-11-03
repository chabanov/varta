import {test} from 'node:test';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {registry} from '../../../varta-common/browser/registry.mjs';
import {compileVariableValue, createVariableScope} from './view-variables.mjs';
const push = (scope, definitions) => scope.extend(new Map(Object.entries(definitions).map(([name, value]) => [name, compileVariableValue(value)])));

test('computed scopes freeze inherited aliases, mask invalid bindings and restore sibling state', () => {
  const scope = createVariableScope();
  push(scope, {'--base': '#123', '--alias': 'var(--base)'});
  const child = push(scope, {'--base': '#456', '--bad': 'var(--missing)', '--cross': 'var(--alias)'});
  assert.equal(scope.substitute('var(--base)'), '#456');
  assert.equal(scope.substitute('var(--cross)'), '#123');
  const grandchild = push(scope, {'--alias': 'var(--bad)'});
  assert.equal(scope.substitute('var(--alias,#abc)'), '#abc');
  scope.restore(grandchild);
  scope.restore(child);
  assert.equal(scope.substitute('var(--base)'), '#123');
  assert.equal(scope.substitute('var(--cross,#789)'), '#789');
  push(scope, {'--cross': 'var(--alias)'});
  assert.equal(scope.substitute('var(--cross)'), '#123');
});
test('active cycles do not invalidate outside fallback consumers or unused fallback edges', () => {
  for (const definitions of [
    {'--a': 'var(--b,var(--c))', '--b': 'var(--a)', '--c': 'var(--b,#abc)'},
    {'--b': 'var(--c,var(--a))', '--c': 'var(--b)', '--a': 'var(--c,#abc)'},
  ]) {
    const scope = createVariableScope(); push(scope, definitions);
    const consumer = definitions['--c'].includes('#abc') ? '--c' : '--a';
    assert.equal(scope.substitute(`var(${consumer})`), '#abc');
  }
  const scope = createVariableScope();
  push(scope, {'--safe': '#111', '--self': 'var(--safe,var(--self))', '--a': 'var(--missing) var(--b)', '--b': 'var(--a,#abc)'});
  assert.equal(scope.substitute('var(--self)'), '#111');
  assert.equal(scope.substitute('var(--b,#789)'), '#789');
});
test('empty fan-out stays valid without retaining exponential expansion work', () => {
 const verify = () => {
  const scope = createVariableScope(), definitions = {'--e2047': 'var(--missing,)'};
  for (let i = 0; i < 2047; i++) definitions[`--e${i}`] = `var(--e${i + 1})var(--e${i + 1})`;
  push(scope, definitions);
  assert.equal(scope.substitute('var(--e0,#abc)'), '');
  assert.equal(scope.substitute('var(--e0,9px) 2px'), '2px');
  push(scope, {'--n': '10', '--quoted': '""', '--a': 'var(--e0)var(--b)', '--b': 'var(--a)'});
  assert.equal(scope.substitute('var(--n)var(--e0)px'), '10 px');
  assert.equal(scope.substitute('var(--quoted)'), '""');
  assert.equal(scope.substitute('var(--a,#abc)'), '#abc');
 };
 const module = new URL('./view-variables.mjs', import.meta.url).href;
 const code = `import assert from 'node:assert/strict';import {compileVariableValue,createVariableScope} from ${JSON.stringify(module)};const push=${push.toString()};(${verify.toString()})();`;
 const result = spawnSync(process.execPath, ['--input-type=module', '-e', code], {encoding:'utf8', timeout:10000});
 assert.equal(result.error, undefined); assert.equal(result.status, 0, result.stderr);
});
test('deep forward alias dependencies and restore do not recurse with dependency depth', () => {
  const scope = createVariableScope(), definitions = {};
  for (let i = 0; i < 5000; i++) definitions[`--v${String(i).padStart(4, '0')}`] = i === 4999 ? '#123' : `var(--v${String(i + 1).padStart(4, '0')})`;
  const mark = push(scope, definitions);
  assert.equal(scope.substitute('var(--v0000)'), '#123');
  scope.restore(mark);
  assert.equal(scope.substitute('var(--v0000,#abc)'), '#abc');
});
test('length summaries enforce the native UTF16 limit before nonempty expansion', () => {
  const scope = createVariableScope(), half = 'x'.repeat(registry.view.limits.valueBytes / 4 - 1);
  push(scope, {'--a': half, '--b': 'var(--a)var(--a)', '--c': 'var(--b)/', '--d': 'var(--c)/'});
  assert.equal(scope.substitute('var(--c)').length, registry.view.limits.valueBytes / 2);
  assert.equal(scope.substitute('var(--d,fallback)'), 'fallback');
  push(scope, {'--emoji': `"${'😀'.repeat(8191)}"`, '--pair': 'var(--emoji)var(--emoji)', '--over': 'var(--pair)/'});
  assert.equal(scope.substitute('var(--pair)').length, registry.view.limits.valueBytes / 2);
  assert.equal(scope.substitute('var(--over,fallback)'), 'fallback');
});
test('functions, quotes, keyword selection and case-sensitive names preserve native substitution', () => {
  const scope = createVariableScope();
  push(scope, {'--tone': '#123', '--Tone': '#456', '--literal': '"var(--absent)"', '--r': '10', '--rgb': 'rgb(var(--r),20,30)'});
  push(scope, {'--tone': 'VAR(--missing,InHeRiT)', '--alias': 'VaR(--Tone)'});
  assert.equal(scope.substitute('var(--tone)'), '#123');
  assert.equal(scope.substitute('var(--alias)'), '#456');
  assert.equal(scope.substitute('var(--literal)'), '"var(--absent)"');
  assert.equal(scope.substitute('var(--missing,var(--rgb))'), 'rgb(10,20,30)');
  assert.equal(scope.substitute('myvar(--missing)'), 'myvar(--missing)');
  push(scope, {'--tone': 'initial'});
  assert.equal(scope.substitute('var(--tone,#abc)'), '#abc');
});
