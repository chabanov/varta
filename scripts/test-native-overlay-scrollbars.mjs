// Real Chrome oracle for native_overlay_scrollbar_probe: macOS overlay
// scrollbars ("Show scroll bars: when scrolling", headless Chrome's style on
// this Mac). Each case runs the probe's steps in real time — a wheel event
// or a programmatic scroll, the pointer moving onto the bar or away, waits —
// and compares every capture: the scroller takes no space for its bar, the
// pointer hits the scroller on a shown bar and the content otherwise, and
// the pixels around the bar match within the native probes' mean bound.
import assert from 'node:assert/strict';
import {readFile,writeFile} from 'node:fs/promises';
import {withBrowserPage} from './browser-probe.mjs';
assert(process.argv[2],'pass native_overlay_scrollbar_probe JSON');
const oracle=JSON.parse(await readFile(process.argv[2],'utf8'));
const scale=oracle.scale;
assert(Number.isFinite(scale)&&scale>=0.25&&scale<=8,'invalid oracle scale');
const viewport=oracle.viewport;
assert(viewport&&Number.isInteger(viewport.width)&&viewport.width>0&&viewport.width<=4096&&Number.isInteger(viewport.height)&&viewport.height>0&&viewport.height<=4096,'invalid oracle viewport');
const MAX_MEAN_RGB_DELTA=2; // Same bound as the other native probes.
const BOX_TOLERANCE=0.02;
// Chrome renders a frame before a wheel event reaches the scroller.
const FIRST_PAINT_MS=200;
// A wheel offset must arrive well within the 500ms before the fade.
const WHEEL_WAIT_MS=200;
const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
let checks=0;
await withBrowserPage('about:blank',async call=>{
  async function evaluate(expression) {
    const r=await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});
    assert(!r.exceptionDetails,JSON.stringify(r.exceptionDetails));return r.result.value;
  }
  await call('Emulation.setDeviceMetricsOverride',{...viewport,deviceScaleFactor:scale,mobile:false});
  await evaluate(`(()=>{document.open();document.write("<!doctype html><html><head></head><body></body></html>");document.close();document.body.style.margin='0';document.body.style.background='white';document.documentElement.style.colorScheme='light';})()`);
  // Overlay scrollbars take no space in this Chrome.
  const gutter=await evaluate(`(()=>{const d=document.createElement('div');d.style.cssText='overflow:auto;width:50px;height:50px';d.append(Object.assign(document.createElement('div'),{style:'height:100px'}));document.body.append(d);const g=d.offsetWidth-d.clientWidth;d.remove();return g;})()`);
  assert.equal(gutter,0,`Chrome scrollbar gutter ${gutter}: run with overlay scrollbars`);
  const [width,height]=[Math.round(viewport.width*scale),Math.round(viewport.height*scale)];
  for(const fixture of oracle.cases) {
    await call('Input.dispatchMouseEvent',{type:'mouseMoved',x:1,y:viewport.height-1});
    await evaluate(`(()=>{
      document.body.replaceChildren();scrollTo(0,0);
      function create(n){const e=document.createElement(n.tag==='root'?'div':n.tag);e.dataset.node=n.id;for(const [p,v]of Object.entries(n.style??{}))e.style.setProperty(p,v);for(const c of n.children??[])e.append(create(c));return e;}
      const root=create(${JSON.stringify(fixture.tree)});
      root.style.isolation='isolate';
      document.body.append(root);
    })()`);
    await sleep(FIRST_PAINT_MS);
    const scroller=`document.querySelector('[data-node="${oracle.scroller}"]')`;
    let captured=0;
    for(const step of fixture.steps) {
      if(step.wheel) {
        const {x,y,top}=step.wheel;
        const before=await evaluate(`${scroller}.scrollTop`);
        await call('Input.dispatchMouseEvent',{type:'mouseMoved',x,y});
        await call('Input.dispatchMouseEvent',{type:'mouseWheel',x,y,deltaX:0,deltaY:top-before});
        // The compositor scrolls; the offset reaches the page a frame later.
        const offset=await evaluate(`new Promise(resolve=>{const s=${scroller},t=performance.now();(function poll(){if(s.scrollTop===${top}||performance.now()-t>${WHEEL_WAIT_MS})resolve(s.scrollTop);else requestAnimationFrame(poll);})();})`);
        assert.equal(offset,top,`${fixture.name}: wheel offset`);
      } else if(step.program) {
        await evaluate(`${scroller}.scrollTop=${step.program.top}`);
      } else if(step.pointer) {
        await call('Input.dispatchMouseEvent',{type:'mouseMoved',x:step.pointer.x,y:step.pointer.y});
      } else if(step.wait!==undefined) {
        await sleep(step.wait);
      } else if(step.capture) {
        const native=fixture.captures[captured++];
        assert.equal(native.name,step.capture,`${fixture.name}: capture order`);
        const label=`${fixture.name}/${native.name}`;
        const screenshot=await call('Page.captureScreenshot',{format:'png'});
        const actual=await evaluate(`(()=>{const s=${scroller},c=document.querySelector('[data-node="${oracle.content}"]');
          return {content:c.getBoundingClientRect().width,client:s.clientWidth,offset:s.offsetWidth,
            hits:${JSON.stringify(native.hits)}.map(p=>Number(document.elementFromPoint(p.x,p.y)?.dataset.node)||null)};})()`);
        // The overlay bar reserves nothing: the content keeps its width.
        assert(Math.abs(actual.content-native.content_width)<BOX_TOLERANCE,`${label}: content width chrome ${actual.content}, native ${native.content_width}`);checks++;
        native.hits.forEach((hit,i)=>{assert.equal(actual.hits[i],hit.id,`${label}: hit at ${hit.x},${hit.y}`);checks++;});
        if(process.argv[3])await writeFile(`${process.argv[3]}-${fixture.name}-${native.name}.png`,Buffer.from(screenshot.data,'base64'));
        assert.deepEqual(native.raster_size,[width,height],`${label}: raster size`);
        const pixels=await evaluate(`(async()=>{const image=new Image();image.src=${JSON.stringify('data:image/png;base64,'+screenshot.data)};await image.decode();const canvas=document.createElement('canvas');canvas.width=${width};canvas.height=${height};const c=canvas.getContext('2d');c.drawImage(image,0,0);return [...c.getImageData(0,0,${width},${height}).data];})()`);
        const r=native.roi;let sum=0,maximum=0,count=0,differing=0;
        for(let y=Math.max(0,r.y);y<Math.min(height,r.y+r.height);y++)for(let x=Math.max(0,r.x);x<Math.min(width,r.x+r.width);x++){
          const at=(y*width+x)*4,white=255-native.raster[at+3];let pixel=0;
          for(let channel=0;channel<3;channel++){const delta=Math.abs(pixels[at+channel]-Math.min(255,native.raster[at+channel]+white));sum+=delta;maximum=Math.max(maximum,delta);pixel=Math.max(pixel,delta);count++;}
          if(pixel>16)differing++;
        }
        assert(count>0,`${label}: empty comparison`);
        const mean=sum/count;
        console.log(JSON.stringify({case:label,rgbMeanDelta:Number(mean.toFixed(3)),rgbMaxDelta:maximum,pixelsOver16:differing,pixelCount:count/3}));
        assert(mean<=MAX_MEAN_RGB_DELTA,`${label}: RGB mean ${mean}, max ${maximum}`);checks++;
      } else throw new Error(`unknown step ${JSON.stringify(step)}`);
    }
    assert.equal(captured,fixture.captures.length,`${fixture.name}: captures`);
  }
});
console.log(`Native/Chrome overlay scrollbars (scale ${scale}): ${checks} checks passed`);
