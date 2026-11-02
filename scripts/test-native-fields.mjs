// Compare native source offsets with real Chrome's password/search caret hits.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { withBrowserPage } from './browser-probe.mjs';

assert(process.argv[2], 'pass the JSON emitted by the native_fields_probe example');
const oracle = JSON.parse(await readFile(process.argv[2], 'utf8'));
let checks = 0;
await withBrowserPage('about:blank', async (call) => {
  async function evaluate(expression) {
    const result = await call('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
    assert(!result.exceptionDetails, JSON.stringify(result.exceptionDetails));
    return result.result.value;
  }
  await evaluate(`(async()=>{
    const face=new FontFace(${JSON.stringify(oracle.family)},Uint8Array.from(${JSON.stringify(oracle.font)}));
    document.fonts.add(await face.load());document.body.style.margin='0';
    const style=document.createElement('style');style.textContent='input::-webkit-search-cancel-button{display:none}';document.head.append(style);
  })()`);
  for (const fixture of oracle.cases) {
    const bounds = await evaluate(`(()=>{
      document.querySelector('input')?.remove();const input=document.createElement('input');input.type=${JSON.stringify(fixture.kind)};
      input.style.appearance='none';input.style.boxSizing='border-box';
      for(const [name,value] of ${JSON.stringify(oracle.styles)})input.style.setProperty(name,value);
      input.value=${JSON.stringify(fixture.value)};document.body.append(input);
      const {x,y,width,height}=input.getBoundingClientRect();return {x,y,width,height};
    })()`);
    assert.deepEqual(bounds, fixture.bounds);
    checks++;
    for (const point of fixture.points) {
      for (const type of ['mousePressed', 'mouseReleased']) {
        await call('Input.dispatchMouseEvent', { type, x: point.x, y: point.y, button: 'left', clickCount: 1 });
      }
      const actual = await evaluate(`(()=>{const input=document.querySelector('input');return {value:input.value,position:new TextEncoder().encode(input.value.slice(0,input.selectionStart)).length};})()`);
      assert.equal(actual.value, fixture.value);
      assert.equal(actual.position, point.position, `${fixture.kind} ${JSON.stringify(fixture.value)} at x=${point.x}`);
      checks++;
    }
  }
});
console.log(`Native/Chrome field geometry and UTF-8 caret positions: ${checks} checks passed`);
