// Real Chrome geometry/hit/pixel oracle for native_controls_probe: each case's
// view tree is rebuilt as DOM under the official browser host's control
// normalization (view-dom.mjs base sheet and the packaged-font control rule),
// so unstyled controls keep Chrome's native theme, control font and focus ring.
import assert from 'node:assert/strict';
import {readFile,writeFile} from 'node:fs/promises';
import {withBrowserPage} from './browser-probe.mjs';
import {registry} from '../crates/varta-common/browser/registry.mjs';
assert(process.argv[2], 'pass native_controls_probe JSON');
const oracle=JSON.parse(await readFile(process.argv[2],'utf8'));
const scale=oracle.scale??1;
assert(Number.isFinite(scale)&&scale>=0.25&&scale<=8,'invalid oracle scale');
const viewport=oracle.viewport;
assert(viewport&&Number.isInteger(viewport.width)&&viewport.width>0&&viewport.width<=4096&&Number.isInteger(viewport.height)&&viewport.height>0&&viewport.height<=4096,'invalid oracle viewport');
const controls=registry.view.defaults.controlFontTags.join(', ');
// view-dom.mjs's base rules for controls, and view-fonts.mjs's packaged-font
// control rule. The caret blinks in Chrome; it is hidden so screenshots are
// deterministic (the native caret stays, a one-pixel column per field).
const baseSheet=`button, input, textarea, select { box-sizing: border-box; margin: 0; } textarea { resize: none; } :where(${controls}) { font-family: inherit; font-synthesis: inherit; } :where(${controls}) { caret-color: transparent; }`;
let checks=0;
const MAX_MEAN_RGB_DELTA=2; // Same bound as the native/GPU scene probes.
const BOX_TOLERANCE=0.02;
await withBrowserPage('about:blank',async call=>{
  async function evaluate(expression) {
    const r=await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});
    assert(!r.exceptionDetails,JSON.stringify(r.exceptionDetails));return r.result.value;
  }
  await call('Emulation.setDeviceMetricsOverride',{...viewport,deviceScaleFactor:scale,mobile:false});
  // A headless page has no window focus; Chrome paints focus rings only in a
  // focused page.
  await call('Emulation.setFocusEmulationEnabled',{enabled:true});
  await evaluate(`(async()=>{document.open();document.write("<!doctype html><html><head></head><body></body></html>");document.close();document.fonts.add(await new FontFace(${JSON.stringify(oracle.family)},Uint8Array.from(${JSON.stringify(oracle.font)})).load());const style=document.createElement('style');style.textContent=${JSON.stringify(baseSheet)};document.head.append(style);document.body.style.margin='0';document.body.style.background='white';document.documentElement.style.colorScheme='light';})()`);
  for(const fixture of oracle.cases) {
    const actual=await evaluate(`(async()=>{
      document.activeElement?.blur?.();document.body.replaceChildren();const nodes=new Map();
      function create(n){const e=n.text!==undefined?document.createTextNode(n.text):document.createElement(n.tag==='root'?'div':n.tag);nodes.set(n.id,e);
        if(e.nodeType===1){e.dataset.node=n.id;for(const [a,v]of Object.entries(n.attributes??{}))e.setAttribute(a,v);for(const [p,v]of Object.entries(n.style??{}))e.style.setProperty(p,v);for(const c of n.children??[])e.append(create(c));
          if(n.value!==undefined)e.value=n.value;if(n.checked!==undefined)e.checked=n.checked;}
        return e;}
      document.body.append(create(${JSON.stringify(fixture.tree)}));
      await document.fonts.ready;
      const focus=${JSON.stringify(fixture.focus)};if(focus!==null)nodes.get(focus).focus();
      const box=r=>({x:r.x,y:r.y,width:r.width,height:r.height}),boxes={};
      for(const [id,e]of nodes)if(e.nodeType===1&&id!==1)boxes[id]=box(e.getBoundingClientRect());
      const rects=[...nodes.values()].filter(e=>e.nodeType===1).flatMap(e=>[...e.getClientRects()]);
      const hits=${JSON.stringify(fixture.hits)}.map(p=>{const x=p.x+.25,y=p.y+.25;
        const edge=rects.some(r=>x>=r.left-1&&x<=r.right+1&&y>=r.top-1&&y<=r.bottom+1&&Math.min(Math.abs(x-r.left),Math.abs(x-r.right),Math.abs(y-r.top),Math.abs(y-r.bottom))<=1);
        let target=document.elementFromPoint(x,y);while(target&&!target.dataset?.node)target=target.parentElement;
        return {x:p.x,y:p.y,id:Number(target?.dataset.node)||null,edge};});
      return {boxes,hits,focused:document.activeElement?.dataset?.node??null};
    })()`);
    if(fixture.focus!==null)assert.equal(Number(actual.focused),fixture.focus,`${fixture.name}: focus`);
    // Report every differing box of a case before failing it.
    const differences=[];
    for(const [id,box]of Object.entries(actual.boxes))for(const key of ['x','y','width','height']){
      assert(fixture.boxes[id],`${fixture.name}: native has no box for node ${id}`);
      if(Math.abs(box[key]-fixture.boxes[id][key])>=BOX_TOLERANCE)differences.push(`node ${id} ${key}: chrome ${box[key]}, native ${fixture.boxes[id][key]}`);else checks++;
    }
    assert.deepEqual(differences,[],`${fixture.name} boxes`);
    const misses=[];
    for(let i=0;i<actual.hits.length;i++) {
      if(actual.hits[i].edge||fixture.hits[i].boundary)continue;
      const chrome=actual.hits[i].id > 1 ? actual.hits[i].id : null, native=fixture.hits[i].id > 1 ? fixture.hits[i].id : null;
      if(chrome!==native)misses.push(`(${actual.hits[i].x}, ${actual.hits[i].y}): chrome ${chrome}, native ${native}`);else checks++;
    }
    assert.deepEqual(misses,[],`${fixture.name} hits`);
    const screenshot=await call('Page.captureScreenshot',{format:'png'});
    if(process.argv[3])await writeFile(`${process.argv[3]}-${fixture.name}.png`,Buffer.from(screenshot.data,'base64'));
    const [width,height]=fixture.raster_size;
    assert.equal(width,Math.round(viewport.width*scale),`${fixture.name}: raster viewport width`);
    assert.equal(height,Math.round(viewport.height*scale),`${fixture.name}: raster viewport height`);
    assert.equal(fixture.raster.length,width*height*4,`${fixture.name}: invalid native raster size`);
    const pixels=await evaluate(`(async()=>{const image=new Image();image.src=${JSON.stringify('data:image/png;base64,'+screenshot.data)};await image.decode();const canvas=document.createElement('canvas');canvas.width=${width};canvas.height=${height};const c=canvas.getContext('2d');c.drawImage(image,0,0);return [...c.getImageData(0,0,${width},${height}).data];})()`);
    if(process.argv[3]){
      // Diagnostic: the native raster composited on white, as PNG via canvas.
      const native=await evaluate(`(()=>{const canvas=document.createElement('canvas');canvas.width=${width};canvas.height=${height};const c=canvas.getContext('2d');const data=c.createImageData(${width},${height});const raw=${JSON.stringify(fixture.raster)};for(let i=0;i<raw.length;i+=4){const white=255-raw[i+3];data.data[i]=Math.min(255,raw[i]+white);data.data[i+1]=Math.min(255,raw[i+1]+white);data.data[i+2]=Math.min(255,raw[i+2]+white);data.data[i+3]=255;}c.putImageData(data,0,0);return canvas.toDataURL('image/png').split(',')[1];})()`);
      await writeFile(`${process.argv[3]}-${fixture.name}-native.png`,Buffer.from(native,'base64'));
    }
    let sum=0,maximum=0,count=0,strong=null;
    // Diagnostic only: the device-pixel bounds of strongly differing pixels.
    const STRONG_DELTA=128;
    for(let y=0;y<height;y++)for(let x=0;x<width;x++){
      const at=(y*width+x)*4,white=255-fixture.raster[at+3];
      for(let channel=0;channel<3;channel++){const delta=Math.abs(pixels[at+channel]-Math.min(255,fixture.raster[at+channel]+white));sum+=delta;maximum=Math.max(maximum,delta);count++;if(delta>=STRONG_DELTA)strong=strong?{x0:Math.min(strong.x0,x),y0:Math.min(strong.y0,y),x1:Math.max(strong.x1,x),y1:Math.max(strong.y1,y)}:{x0:x,y0:y,x1:x,y1:y};}
    }
    const mean=sum/count;
    console.log(JSON.stringify({case:fixture.name,rgbMeanDelta:mean,rgbMaxDelta:maximum,pixelCount:count/3,strongDeltaBounds:strong}));
    assert(mean<=MAX_MEAN_RGB_DELTA,`${fixture.name}: RGB mean ${mean}, max ${maximum}`);checks++;
  }
  // input type=number stepping: the guest sets the value, the field is
  // focused with the caret at the end, and real key presses (with held
  // modifiers) step it; the value and the input/change events must match.
  // Chrome sanitizes an assigned value, so the native grammar is checked
  // separately: a valid value is kept, an invalid one becomes empty.
  assert(Array.isArray(oracle.steps)&&oracle.steps.length,'number stepping cases missing');
  assert(Array.isArray(oracle.grammar)&&oracle.grammar.length,'number grammar cases missing');
  const ARROW_KEY_CODES={ArrowUp:38,ArrowDown:40};
  // CDP Input.dispatchKeyEvent modifier bits.
  const MODIFIER_BITS={Alt:1,Ctrl:2,Meta:4,Shift:8};
  const stepMismatches=[];
  for(const step of oracle.steps){
    await evaluate(`(()=>{document.body.replaceChildren();const field=document.createElement('input');field.type='number';field.id='step';document.body.append(field);field.value=${JSON.stringify(step.value)};field.focus();window.stepEvents=[];for(const kind of ['input','change'])field.addEventListener(kind,()=>stepEvents.push(kind));return true;})()`);
    for(const pressed of step.keys){
      const parts=pressed.split('+'),key=parts.pop();
      assert(ARROW_KEY_CODES[key],`unknown step key ${pressed}`);
      const modifiers=parts.reduce((bits,modifier)=>{assert(MODIFIER_BITS[modifier],`unknown modifier ${pressed}`);return bits|MODIFIER_BITS[modifier];},0);
      for(const type of ['rawKeyDown','keyUp'])await call('Input.dispatchKeyEvent',{type,key,code:key,windowsVirtualKeyCode:ARROW_KEY_CODES[key],modifiers});
    }
    const chrome=await evaluate(`({value:document.getElementById('step').value,events:stepEvents})`);
    const row=`${JSON.stringify(step.value)} ${step.keys.join(',')}`;
    if(chrome.value!==step.native)stepMismatches.push(`${row}: chrome ${JSON.stringify(chrome.value)}, native ${JSON.stringify(step.native)}`);else checks++;
    if(JSON.stringify(chrome.events)!==JSON.stringify(step.events))stepMismatches.push(`${row} events: chrome ${chrome.events}, native ${step.events}`);else checks++;
  }
  for(const {value,valid} of [...oracle.steps,...oracle.grammar]){
    const sanitized=await evaluate(`(()=>{const field=document.createElement('input');field.type='number';field.value=${JSON.stringify(value)};return field.value;})()`);
    assert(sanitized===value||sanitized==='',`${JSON.stringify(value)}: Chrome sanitized it to ${JSON.stringify(sanitized)}`);
    const chromeValid=value!==''&&sanitized===value;
    if(chromeValid!==valid)stepMismatches.push(`${JSON.stringify(value)} validity: chrome ${chromeValid}, native ${valid}`);else checks++;
  }
  assert.deepEqual(stepMismatches,[],'number stepping');
});
console.log(`Native/Chrome controls (UA table ${oracle.ua_version}, scale ${scale}): ${checks} checks passed`);
