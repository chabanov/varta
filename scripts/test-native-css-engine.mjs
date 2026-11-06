// Real-Chrome oracle for the K8 T2 CSS engine. Two modes compare the
// native boxes and pixels of each scene:
//
// - `bridge` (default): the official browser host's view bridge
//   (`view-dom.mjs`) applies the scene; its Wasm style engine computes every
//   value and the bridge sets them inline, so this compares the web host
//   with the native renderer;
// - `platform`: Chrome's own cascade evaluates the stylesheet text in a
//   closed shadow root, as the specification oracle for the engine's
//   semantics. Chrome ignores `@property` in shadow trees although
//   registrations are not tree-scoped (Properties and Values API 1 §2.8), so
//   this mode registers them in the document.
//
// A scene with a recorded deviation (CSS Color 4 gamut mapping, which
// Chrome does not perform on its own cascade; `color-mix()` with other than
// two colours, which Chrome drops) reports its difference instead of
// failing in `platform` mode.
//
// cargo run -p varta-view --example native_css_engine_probe -- 2 > target/native-css-2.json
// node scripts/test-native-css-engine.mjs target/native-css-2.json [bridge|platform]
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {createServer} from 'node:http';
import {resolve,sep} from 'node:path';
import {withBrowserPage} from './browser-probe.mjs';
import {registry} from '../crates/varta-common/browser/registry.mjs';

const mode=process.argv[3]??'bridge';
assert(['bridge','platform'].includes(mode),'mode is bridge or platform');
// `:root` maps to the root container in the platform mode.
const rewriteRootSelector=(text,selector)=>text.replace(/:root(?![A-Za-z0-9_-])/g,selector);
const root=resolve(import.meta.dirname,'..');
const server=createServer(async(request,response)=>{
  try{
    const name=new URL(request.url,'http://loopback').pathname;
    if(name==='/__css_engine'){response.setHeader('Content-Type','text/html');response.end(`<!doctype html><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; img-src data:; style-src 'unsafe-inline'"><body style="margin:0;background:white"></body>`);return;}
    const path=resolve(root,`.${decodeURIComponent(name)}`);if(!path.startsWith(root+sep))throw new Error('outside workspace');
    const bytes=await readFile(path);response.setHeader('Content-Type',path.endsWith('.wasm')?'application/wasm':'text/javascript');response.end(bytes);
  }catch{response.writeHead(404).end();}
});
await new Promise(done=>server.listen(0,'127.0.0.1',done));

assert(process.argv[2], 'pass native_css_engine_probe JSON');
const oracle=JSON.parse(await readFile(process.argv[2],'utf8'));
const scale=oracle.scale;
assert(Number.isFinite(scale)&&scale>=0.25&&scale<=8,'invalid oracle scale');
const {width,height}=oracle.viewport;
const MAX_MEAN_RGB_DELTA=2; // Same bound as the native/GPU scene probes.
const MAX_CENTRE_RGB_DELTA=1; // One 8-bit rounding step inside a fill.
const BOX_TOLERANCE=0.02;
let checks=0;
const report=[];
try{await withBrowserPage(`http://127.0.0.1:${server.address().port}/__css_engine`,async call=>{
  async function evaluate(expression) {
    const r=await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});
    assert(!r.exceptionDetails,JSON.stringify(r.exceptionDetails));return r.result.value;
  }
  await call('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:scale,mobile:false});
  await evaluate(`(async()=>{document.documentElement.style.colorScheme='light';
    globalThis.viewEngine=await WebAssembly.compileStreaming(fetch('/crates/varta-project/src/browser/view-engine.wasm'));
    globalThis.bridge=await import('/crates/varta-project/src/browser/view-dom.mjs');
    globalThis.codec=await import('/crates/varta-project/src/browser/view-codec.mjs');})()`);
  const environment=await evaluate(`(()=>Object.fromEntries(['(hover: hover)','(any-hover: hover)','(pointer: fine)','(any-pointer: fine)','(prefers-reduced-motion: no-preference)','(prefers-contrast: no-preference)','(forced-colors: none)'].map(q=>[q,matchMedia(q).matches])))()`);
  for(const [query,matches]of Object.entries(environment)){assert(matches,`Chrome reports ${query} false; the native defaults assume it true`);checks++;}
  for(const fixture of oracle.cases){
    await call('Emulation.setEmulatedMedia',{features:[{name:'prefers-reduced-motion',value:fixture.reducedMotion?'reduce':'no-preference'}]});
    // Properties and Values API 1 §2.8: registrations are not scoped to a
    // tree scope, but Chrome ignores @property rules in shadow trees (a
    // recorded deviation). The oracle registers them in the document, which
    // the specification makes equivalent.
    const registrations=fixture.sheet.match(/@property\s+--[A-Za-z0-9_-]+\s*\{[^}]*\}/g)??[];
    const sheet=rewriteRootSelector(fixture.sheet,registry.view.defaults.rootSelector);
    const boxes=mode==='bridge'?await evaluate(`(async()=>{
      globalThis.app?.close();document.body.replaceChildren();
      const host=document.createElement('div');host.style.width='${width}px';host.style.height='${height}px';document.body.append(host);
      const app=globalThis.app=bridge.createViewDom({viewEngine,container:host,interactive:false});
      const ops=[{name:'stylesheet',sheet:1,text:${JSON.stringify(fixture.sheet)}}];let next=2;
      const build=(parent,list)=>{for(const n of list){const id=next++;ops.push({name:'create_element',node:id,tag:'div'},{name:'set_class',node:id,text:n.class},{name:'insert',parent,node:id,before:0});build(id,n.children);}};
      build(1,${JSON.stringify(fixture.nodes)});
      app.apply(codec.encodeViewBatch(ops));
      await new Promise(done=>requestAnimationFrame(()=>requestAnimationFrame(done)));
      return Object.fromEntries(Array.from({length:next-2},(_,i)=>i+2).map(id=>{const r=app.inspectNode(id).getBoundingClientRect();return [id,{x:r.x,y:r.y,width:r.width,height:r.height}];}));
    })()`):await evaluate(`(()=>{
      globalThis.app?.close();globalThis.app=undefined;document.body.replaceChildren();
      for(const old of document.head.querySelectorAll('style[data-registrations]'))old.remove();
      const registrations=document.createElement('style');registrations.dataset.registrations='';registrations.textContent=${JSON.stringify(registrations.join('\n'))};document.head.append(registrations);
      const host=document.createElement('div');host.style.width='${width}px';host.style.height='${height}px';document.body.append(host);
      const shadow=host.attachShadow({mode:'closed'});
      const style=document.createElement('style');style.textContent=${JSON.stringify(sheet)};shadow.append(style);
      const root=document.createElement('div');root.setAttribute('data-l0-root','');root.style.cssText=${JSON.stringify(registry.view.defaults.rootStyle)};shadow.append(root);
      let next=2;const nodes=new Map();
      const build=(parent,list)=>{for(const n of list){const e=document.createElement('div');e.className=n.class;nodes.set(next++,e);parent.append(e);build(e,n.children);}};
      build(root,${JSON.stringify(fixture.nodes)});
      return Object.fromEntries([...nodes].map(([id,e])=>{const r=e.getBoundingClientRect();return [id,{x:r.x,y:r.y,width:r.width,height:r.height}];}));
    })()`);
    if(process.env.VARTA_CSS_PROBE_DEBUG)console.log(fixture.name,JSON.stringify(boxes),JSON.stringify(fixture.boxes));
    for(const [id,box]of Object.entries(boxes))for(const key of ['x','y','width','height']){
      assert(Math.abs(box[key]-fixture.boxes[id][key])<BOX_TOLERANCE,`${fixture.name} node ${id} ${key}: chrome ${box[key]}, native ${fixture.boxes[id][key]}`);checks++;
    }
    const screenshot=await call('Page.captureScreenshot',{format:'png'});
    const [w,h]=fixture.rasterSize;
    assert.equal(fixture.raster.length,w*h*4,`${fixture.name}: native raster size`);
    const pixels=await evaluate(`(async()=>{const image=new Image();image.src=${JSON.stringify('data:image/png;base64,'+screenshot.data)};await image.decode();const canvas=document.createElement('canvas');canvas.width=${w};canvas.height=${h};const c=canvas.getContext('2d');c.drawImage(image,0,0);return [...c.getImageData(0,0,${w},${h}).data];})()`);
    let sum=0,maximum=0,count=0,differing=0;
    for(let at=0;at<w*h*4;at+=4){
      const white=255-fixture.raster[at+3];let pixelMax=0;
      for(let channel=0;channel<3;channel++){const delta=Math.abs(pixels[at+channel]-Math.min(255,fixture.raster[at+channel]+white));sum+=delta;maximum=Math.max(maximum,delta);pixelMax=Math.max(pixelMax,delta);count++;}
      if(pixelMax>1)differing++;
    }
    const mean=sum/count;
    // Per element: the centre pixel natively and in Chrome, for diagnosis.
    if(process.env.VARTA_CSS_PROBE_DEBUG)for(const [id,box]of Object.entries(fixture.boxes)){
      const x=Math.floor((box.x+box.width/2)*scale),y=Math.floor((box.y+box.height/2)*scale),at=(y*w+x)*4;
      if(x>=w||y>=h)continue;
      const white=255-fixture.raster[at+3];
      console.log(fixture.name,id,'native',[0,1,2].map(c=>Math.min(255,fixture.raster[at+c]+white)),'chrome',[...pixels.slice(at,at+3)]);
    }
    const row={mode,scale,case:fixture.name,rgbMeanDelta:+mean.toFixed(4),rgbMaxDelta:maximum,pixelsOverOne:differing,deviation:fixture.deviation};
    report.push(row);console.log(JSON.stringify(row));
    // The bridge applies the engine's gamut-mapped colours, so only the
    // platform cascade shows Chrome's clipping.
    if(!fixture.deviation||mode==='bridge'){
      assert(mean<=MAX_MEAN_RGB_DELTA,`${fixture.name}: RGB mean ${mean}, max ${maximum}`);checks++;
      // The scene mean hides one wrong fill, so every box's centre pixel,
      // away from antialiased edges, must match within 1/255 as well.
      for(const [id,box]of Object.entries(fixture.boxes)){
        if(box.width<1||box.height<1)continue;
        const x=Math.floor((box.x+box.width/2)*scale),y=Math.floor((box.y+box.height/2)*scale);
        if(x>=w||y>=h)continue;
        const at=(y*w+x)*4,white=255-fixture.raster[at+3];
        const native=[0,1,2].map(c=>Math.min(255,fixture.raster[at+c]+white)),chrome=[...pixels.slice(at,at+3)];
        assert(native.every((value,c)=>Math.abs(value-chrome[c])<=MAX_CENTRE_RGB_DELTA),`${fixture.name} #${id}: centre native ${native}, Chrome ${chrome}`);checks++;
      }
    }
  }
});}finally{server.close();}
console.log(`Native/Chrome CSS engine (${mode}) @${scale}x: ${checks} checks passed`);
