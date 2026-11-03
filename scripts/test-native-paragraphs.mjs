// Compare real Chrome Unicode layout with the native paragraph probe.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { withBrowserPage } from './browser-probe.mjs';

assert(process.argv[2], 'pass the JSON emitted by native_paragraph_probe');
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
  })()`);
  for (const fixture of oracle.cases) {
    const actual = await evaluate(`(()=>{
      document.body.replaceChildren();
      const element=document.createElement('div');
      for(const [name,value] of ${JSON.stringify(fixture.styles)})element.style.setProperty(name,value);
      element.textContent=${JSON.stringify(fixture.text)};document.body.append(element);
      const {x,y,width,height}=element.getBoundingClientRect();
      return {bounds:{x,y,width,height},text:element.textContent};
    })()`);
    const label = `${JSON.stringify(fixture.text)} ${JSON.stringify(fixture.styles)}`;
    assert.deepEqual(actual.bounds, fixture.bounds, label);
    assert.equal(actual.text, fixture.text, label);
    checks += 2;
    if (fixture.visualGlyphs !== null) {
      // Independently validate the glyph-order oracle using browser shaping.
      // This compares source and aliases within Chrome, not cross-backend pixels.
      const equal = await evaluate(`(()=>{
        const canvas=document.createElement('canvas');canvas.width=160;canvas.height=60;
        const context=canvas.getContext('2d');context.font='20px '+${JSON.stringify(JSON.stringify(oracle.family))};
        context.direction='ltr';
        context.fillText(${JSON.stringify(fixture.text)},0,30);
        const source=context.getImageData(0,0,160,60).data;
        context.clearRect(0,0,160,60);
        context.fillText(${JSON.stringify(fixture.visualGlyphs)},0,30);
        const aliases=context.getImageData(0,0,160,60).data;
        return source.some(byte=>byte!==0)&&source.every((byte,index)=>byte===aliases[index]);
      })()`);
      assert.equal(equal, true, `browser glyph order: ${label}`);
      checks++;
    }
  }
});
console.log(`Native/Chrome Unicode paragraphs: ${checks} checks passed`);
