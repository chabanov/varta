// Compare native rem conversion with real Chrome shadow-root CSS geometry.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {withBrowserPage} from './browser-probe.mjs';

assert(process.argv[2], 'pass the JSON emitted by native_lengths_probe');
const oracle=JSON.parse(await readFile(process.argv[2],'utf8'));
let checks=0;
await withBrowserPage('about:blank',async call=>{
  async function evaluate(expression) {
    const result=await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});
    assert(!result.exceptionDetails,JSON.stringify(result.exceptionDetails));
    return result.result.value;
  }
  await call('Emulation.setDeviceMetricsOverride',{width:1024,height:200,deviceScaleFactor:1,mobile:false});
  await evaluate(`(async()=>{const face=new FontFace(${JSON.stringify(oracle.family)},Uint8Array.from(${JSON.stringify(oracle.font)}));document.fonts.add(await face.load());document.body.style.margin='0';})()`);
  for(const fixture of oracle.cases) {
    const actual=await evaluate(`(()=>{
      document.body.replaceChildren();document.documentElement.style.fontSize=${JSON.stringify(`${fixture.reference}px`)};
      const shell=document.createElement('div');document.body.append(shell);
      const shadow=shell.attachShadow({mode:'closed'}),root=document.createElement('div');shadow.append(root);
      const style=document.createElement('style');style.textContent=${JSON.stringify(fixture.stylesheet)};shadow.append(style);
      root.style.width='1024px';root.style.height='200px';
      const apply=(element,styles)=>{for(const [name,value] of styles)element.style.setProperty(name,value);};
      apply(root,${JSON.stringify(fixture.rootStyles)});
      const parent=document.createElement('div');parent.className='media';apply(parent,${JSON.stringify(fixture.parentStyles)});root.append(parent);
      let child;
      if(${JSON.stringify(fixture.text)}!==null)child=document.createTextNode(${JSON.stringify(fixture.text)});
      else{child=document.createElement('div');apply(child,${JSON.stringify(fixture.childStyles)});}
      parent.append(child);
      const box=element=>{const {x,y,width,height}=element.getBoundingClientRect();return {x,y,width,height};};
      return {documentRoot:getComputedStyle(document.documentElement).fontSize,viewRoot:getComputedStyle(root).fontSize,boxes:child.nodeType===3?[box(parent)]:[box(parent),box(child)]};
    })()`);
    assert.equal(actual.documentRoot,`${fixture.reference}px`);
    assert.equal(actual.viewRoot,`${fixture.reference*2}px`);
    checks+=2;
    // Text-node rectangles are anonymous layout boxes in the native API;
    // DOM Range rectangles describe glyph runs, so compare their container.
    for(let index=0;index<actual.boxes.length;index++)for(const key of ['x','y','width','height']) {
      assert(Math.abs(actual.boxes[index][key]-fixture.boxes[index][key])<0.01,
        `${fixture.name} @ ${fixture.reference}px node ${index} ${key}: ${actual.boxes[index][key]} != ${fixture.boxes[index][key]}`);
      checks++;
    }
  }
});
console.log(`Native/Chrome rem geometry: ${checks} checks passed`);
