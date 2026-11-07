// Independent real-Chrome CSS transition/keyframe oracle. All clocks are
// virtual; screenshots compare actual ink, including outside the border box.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {withBrowserPage} from './browser-probe.mjs';
assert(process.argv[2], 'pass native_shadow_motion_probe JSON');
const oracle=JSON.parse(await readFile(process.argv[2],'utf8'));
const [width,height]=oracle.rasterSize;
let checks=0;
await withBrowserPage('about:blank',async call=>{
 const evaluate=async expression=>{
  const r=await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});
  assert(!r.exceptionDetails,JSON.stringify(r.exceptionDetails));return r.result.value;
 };
 await call('Emulation.setDeviceMetricsOverride',{...oracle.viewport,deviceScaleFactor:oracle.scale,mobile:false});
 for(const fixture of oracle.cases){
  await evaluate(`(()=>{document.open();document.write('<!doctype html><body style="margin:0;background:white"></body>');document.close();const sheet=document.createElement('style');sheet.textContent=${JSON.stringify(fixture.sheet)};document.head.append(sheet);const box=document.createElement('div');box.style.cssText=${JSON.stringify(fixture.style)};document.body.append(box);window.box=box;window.created=new Map();window.setTime=at=>{for(const a of document.getAnimations()){if(!created.has(a)){created.set(a,at);a.pause();}a.currentTime=at-created.get(a);}};getComputedStyle(box).boxShadow;setTime(0);})()`);
  const steps=[...fixture.steps];let maximumMean=0;
  for(const sample of fixture.samples){
   while(steps.length&&steps[0].at<=sample.at){const step=steps.shift();await evaluate(`(()=>{setTime(${step.at});for(const c of ${JSON.stringify(step.changes)})box.style.setProperty(c.property,c.value);getComputedStyle(box).boxShadow;setTime(${step.at});})()`);}
   const state=await evaluate(`(()=>{setTime(${sample.at});const b=box.getBoundingClientRect();return {shadow:getComputedStyle(box).boxShadow,running:document.getAnimations().some(a=>a.currentTime<a.effect.getComputedTiming().endTime),box:[b.x,b.y,b.width,b.height]};})()`);
   assert.equal(sample.animating,state.running,`${fixture.name}@${sample.at}: running`);checks++;
   assert.deepEqual(state.box,[40,40,32,24]);checks++;
   const screenshot=await call('Page.captureScreenshot',{format:'png'});
   const pixels=await evaluate(`(async()=>{const image=new Image();image.src=${JSON.stringify('data:image/png;base64,'+screenshot.data)};await image.decode();const canvas=document.createElement('canvas');canvas.width=${width};canvas.height=${height};const c=canvas.getContext('2d');c.drawImage(image,0,0);return [...c.getImageData(0,0,${width},${height}).data];})()`);
   assert.equal(sample.raster.length,width*height*4);
   let sum=0;
   const overWhite=(at,c)=>Math.min(255,sample.raster[at+c]+255-sample.raster[at+3]);
   for(let at=0;at<pixels.length;at+=4)for(let c=0;c<3;c++)sum+=Math.abs(pixels[at+c]-overWhite(at,c));
   const mean=sum/(width*height*3);maximumMean=Math.max(maximumMean,mean);
   assert(mean<=2,`${fixture.name}@${sample.at}: RGB mean ${mean}, ${state.shadow}`);checks++;
   // Interior and exterior samples prevent the scene mean from concealing
   // a missing/incorrect shadow. Blurred ink keeps the existing 5% bound.
   for(const [x,y]of [[36,52],[38,52],[42,52],[60,52],[74,52],[76,52]]){
    const at=(Math.floor(y*oracle.scale)*width+Math.floor(x*oracle.scale))*4;
    for(let c=0;c<3;c++) {
     const native=overWhite(at,c),delta=Math.abs(pixels[at+c]-native);
     // Fractional sharp edges have different AA quantization. As in the
     // static-shadow oracle, bound only these edge samples by Chrome's
     // immediate neighbours; uniform interior/exterior ink keeps its bound.
     const neighbours=[pixels[at-4+c],pixels[at+c],pixels[at+4+c]];
     const edge=fixture.tolerance===2&&Math.max(...neighbours)-Math.min(...neighbours)>2;
     assert(delta<=fixture.tolerance||(edge&&native>=Math.min(...neighbours)-2&&native<=Math.max(...neighbours)+2),`${fixture.name}@${sample.at} (${x},${y}) channel ${c}: native ${native}, chrome ${pixels[at+c]}, ${state.shadow}`);
    }
    checks++;
   }
  }
  console.log(JSON.stringify({scale:oracle.scale,case:fixture.name,samples:fixture.samples.length,maxMeanRgbDelta:+maximumMean.toFixed(4)}));
 }
});
console.log(`Native/Chrome shadow motion @${oracle.scale}x: ${checks} checks passed`);
