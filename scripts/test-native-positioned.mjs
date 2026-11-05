// Real Chrome geometry/hit/pixel oracle for native_positioned_probe: the same
// checks as test-native-inline.mjs, after applying the fixture's programmatic
// scroll offsets, plus a nearest-edge scrollIntoView comparison.
import assert from 'node:assert/strict';
import {readFile,writeFile} from 'node:fs/promises';
import {withBrowserPage} from './browser-probe.mjs';
assert(process.argv[2], 'pass native_positioned_probe JSON');
const oracle=JSON.parse(await readFile(process.argv[2],'utf8'));
const scale=oracle.scale??1;
assert(Number.isFinite(scale)&&scale>=0.25&&scale<=8,'invalid oracle scale');
const viewport=oracle.viewport;
assert(viewport&&Number.isInteger(viewport.width)&&viewport.width>0&&viewport.width<=4096&&Number.isInteger(viewport.height)&&viewport.height>0&&viewport.height<=4096,'invalid oracle viewport');
let checks=0;
const MAX_MEAN_RGB_DELTA=2; // Same bound as the native/GPU scene probes.
const BOX_TOLERANCE=0.02;
await withBrowserPage('about:blank',async call=>{
  async function evaluate(expression) {
    const r=await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});
    assert(!r.exceptionDetails,JSON.stringify(r.exceptionDetails));return r.result.value;
  }
  await call('Emulation.setDeviceMetricsOverride',{...viewport,deviceScaleFactor:scale,mobile:false});
  await evaluate(`(async()=>{document.open();document.write("<!doctype html><html><head></head><body></body></html>");document.close();document.fonts.add(await new FontFace(${JSON.stringify(oracle.family)},Uint8Array.from(${JSON.stringify(oracle.font)})).load());document.body.style.margin='0';document.body.style.background='white';document.documentElement.style.colorScheme='light';})()`);
  for(const fixture of oracle.cases) {
    const actual=await evaluate(`(async()=>{
      document.body.replaceChildren();scrollTo(0,0);const nodes=new Map(),scrolls=[];
      function create(n){const e=n.text!==undefined?document.createTextNode(n.text):document.createElement(n.tag==='root'?'div':n.tag);nodes.set(n.id,e);if(e.nodeType===1){e.dataset.node=n.id;for(const [p,v]of Object.entries(n.style??{}))e.style.setProperty(p,v);if(n.scrollTop)scrolls.push([e,n.scrollTop]);for(const c of n.children??[])e.append(create(c));}return e;}
      const root=create(${JSON.stringify(fixture.tree)});
      // The native view root is the root stacking context (CSS2 Appendix E's
      // root element), so negative layers paint above its background. Isolation
      // makes the stand-in root div one without changing its geometry.
      root.style.isolation='isolate';
      document.body.append(root);
      await document.fonts.ready;
      for(const [e,top]of scrolls)e.scrollTop=top;
      const box=r=>({x:r.x,y:r.y,width:r.width,height:r.height}),boxes={};
      for(const [id,e]of nodes)if(e.nodeType===1&&id!==1)boxes[id]=box(e.getBoundingClientRect());
      const rects=[...nodes.values()].filter(e=>e.nodeType===1).flatMap(e=>[...e.getClientRects()]);
      const hits=${JSON.stringify(fixture.hits)}.map(p=>{const x=p.x+.25,y=p.y+.25;
        // Chrome tests a physical pixel footprint at fractional coordinates.
        const edge=rects.some(r=>x>=r.left-1&&x<=r.right+1&&y>=r.top-1&&y<=r.bottom+1&&Math.min(Math.abs(x-r.left),Math.abs(x-r.right),Math.abs(y-r.top),Math.abs(y-r.bottom))<=1);
        return {x:p.x,y:p.y,id:Number(document.elementFromPoint(x,y)?.dataset.node)||null,edge};});
      return {boxes,hits,document:[scrollX,scrollY]};
    })()`);
    assert.deepEqual(actual.document,[0,0],`${fixture.name}: the document must not scroll`);
    for(const [id,box]of Object.entries(actual.boxes))for(const key of ['x','y','width','height']){
      assert(Math.abs(box[key]-fixture.boxes[id][key])<BOX_TOLERANCE,`${fixture.name} node ${id} ${key}: chrome ${box[key]}, native ${fixture.boxes[id][key]}`);checks++;
    }
    for(let i=0;i<actual.hits.length;i++) {
      if(actual.hits[i].edge||fixture.hits[i].boundary)continue;
      // Compare guest elements; document/implicit-root background is host-owned.
      assert.equal(actual.hits[i].id > 1 ? actual.hits[i].id : null, fixture.hits[i].id > 1 ? fixture.hits[i].id : null,`${fixture.name} hit ${JSON.stringify(actual.hits[i])}`);checks++;
    }
    const screenshot=await call('Page.captureScreenshot',{format:'png'});
    if(process.argv[3])await writeFile(`${process.argv[3]}-${fixture.name}.png`,Buffer.from(screenshot.data,'base64'));
    const [width,height]=fixture.raster_size;
    assert.equal(width,Math.round(viewport.width*scale),`${fixture.name}: raster viewport width`);
    assert.equal(height,Math.round(viewport.height*scale),`${fixture.name}: raster viewport height`);
    assert.equal(fixture.raster.length,width*height*4,`${fixture.name}: invalid native raster size`);
    const pixels=await evaluate(`(async()=>{const image=new Image();image.src=${JSON.stringify('data:image/png;base64,'+screenshot.data)};await image.decode();const canvas=document.createElement('canvas');canvas.width=${width};canvas.height=${height};const c=canvas.getContext('2d');c.drawImage(image,0,0);return [...c.getImageData(0,0,${width},${height}).data];})()`);
    const r=fixture.roi;let sum=0,maximum=0,count=0,strong=null;
    // Diagnostic only: the device-pixel bounds of strongly differing pixels.
    const STRONG_DELTA=128;
    for(let y=Math.max(0,Math.floor(r.y));y<Math.min(height,Math.ceil(r.y+r.height));y++)for(let x=Math.max(0,Math.floor(r.x));x<Math.min(width,Math.ceil(r.x+r.width));x++){
      const at=(y*width+x)*4,white=255-fixture.raster[at+3];
      for(let channel=0;channel<3;channel++){const delta=Math.abs(pixels[at+channel]-Math.min(255,fixture.raster[at+channel]+white));sum+=delta;maximum=Math.max(maximum,delta);count++;if(delta>=STRONG_DELTA)strong=strong?{x0:Math.min(strong.x0,x),y0:Math.min(strong.y0,y),x1:Math.max(strong.x1,x),y1:Math.max(strong.y1,y)}:{x0:x,y0:y,x1:x,y1:y};}
    }
    assert(count>0,`${fixture.name}: empty raster comparison`);
    const mean=sum/count;
    console.log(JSON.stringify({case:fixture.name,rgbMeanDelta:mean,rgbMaxDelta:maximum,pixelCount:count/3,strongDeltaBounds:strong}));
    assert(mean<=MAX_MEAN_RGB_DELTA,`${fixture.name}: RGB mean ${mean}, max ${maximum}`);checks++;
    if(fixture.reveal){
      // Nearest-edge scrollIntoView changes exactly the planned scrollports.
      const changed=await evaluate(`(()=>{const nodes=[...document.querySelectorAll('[data-node]')];const before=new Map(nodes.map(e=>[e,e.scrollTop]));
        document.querySelector('[data-node="${fixture.reveal.target}"]').scrollIntoView({block:'nearest',inline:'nearest'});
        return {changes:nodes.filter(e=>e.scrollTop!==before.get(e)).map(e=>[Number(e.dataset.node),e.scrollTop]),document:[scrollX,scrollY]};})()`);
      assert.deepEqual(changed.document,[0,0],`${fixture.name}: scrollIntoView must not scroll the document`);
      const expected=new Map(fixture.reveal.plan.map(([id,top])=>[id,top]));
      assert.equal(changed.changes.length,expected.size,`${fixture.name} reveal: chrome ${JSON.stringify(changed.changes)}, native ${JSON.stringify(fixture.reveal.plan)}`);
      for(const [id,top]of changed.changes)assert(Math.abs(top-expected.get(id))<BOX_TOLERANCE,`${fixture.name} reveal node ${id}: chrome ${top}, native ${expected.get(id)}`);
      console.log(JSON.stringify({case:fixture.name,reveal:fixture.reveal.target,changes:changed.changes}));checks++;
    }
  }
});
console.log(`Native/Chrome positioned (scale ${scale}): ${checks} checks passed`);
