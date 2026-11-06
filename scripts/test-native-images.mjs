// Real Chrome geometry/hit/accessibility/pixel oracle for native_image_probe:
// every case's tree is built in Chrome with the probe's own PNG bytes as
// `img` sources, and its boxes, hit targets, image roles and names, and its
// pixels (the whole viewport and each image's box separately) are compared
// with the native renderer's.
// Usage: cargo run -p varta-view --example native_image_probe -- SCALE > images.json
//        node scripts/test-native-images.mjs images.json [SCREENSHOT_PREFIX]
import assert from 'node:assert/strict';
import {readFile,writeFile} from 'node:fs/promises';
import {withBrowserPage} from './browser-probe.mjs';
assert(process.argv[2], 'pass native_image_probe JSON');
const oracle=JSON.parse(await readFile(process.argv[2],'utf8'));
const scale=oracle.scale??1;
assert(Number.isFinite(scale)&&scale>=0.25&&scale<=8,'invalid oracle scale');
const viewport=oracle.viewport;
assert(viewport&&Number.isInteger(viewport.width)&&viewport.width>0&&viewport.width<=4096&&Number.isInteger(viewport.height)&&viewport.height>0&&viewport.height<=4096,'invalid oracle viewport');
assert(oracle.images&&typeof oracle.images==='object','oracle images missing');
const sources=Object.fromEntries(Object.entries(oracle.images).map(([name,bytes])=>[name,'data:image/png;base64,'+Buffer.from(bytes).toString('base64')]));
let checks=0;
const MAX_MEAN_RGB_DELTA=2; // Same bound as the native/GPU scene probes.
const BOX_TOLERANCE=0.02;
const summary=[];
await withBrowserPage('about:blank',async call=>{
  async function evaluate(expression) {
    const r=await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});
    assert(!r.exceptionDetails,JSON.stringify(r.exceptionDetails));return r.result.value;
  }
  await call('Emulation.setDeviceMetricsOverride',{...viewport,deviceScaleFactor:scale,mobile:false});
  await call('Accessibility.enable');
  await evaluate(`(async()=>{document.open();document.write("<!doctype html><html><head></head><body></body></html>");document.close();document.fonts.add(await new FontFace(${JSON.stringify(oracle.family)},Uint8Array.from(${JSON.stringify(oracle.font)})).load());document.body.style.margin='0';document.body.style.background='white';document.documentElement.style.colorScheme='light';})()`);
  for(const fixture of oracle.cases) {
    const actual=await evaluate(`(async()=>{
      document.body.replaceChildren();scrollTo(0,0);const nodes=new Map(),sources=${JSON.stringify(sources)};
      function create(n){const e=n.text!==undefined?document.createTextNode(n.text):document.createElement(n.tag==='root'?'div':n.tag);nodes.set(n.id,e);if(e.nodeType===1){e.dataset.node=n.id;
        for(const [k,v]of Object.entries(n.attributes??{})){if(k==='src'){if(!(v in sources))throw new Error('unknown image '+v);e.setAttribute('src',sources[v]);}else e.setAttribute(k,v);}
        for(const [p,v]of Object.entries(n.style??{}))e.style.setProperty(p,v);for(const c of n.children??[])e.append(create(c));}return e;}
      const root=create(${JSON.stringify(fixture.tree)});
      // The native view root is the root stacking context.
      root.style.isolation='isolate';
      document.body.append(root);
      await document.fonts.ready;
      await Promise.all([...document.images].map(i=>i.decode()));
      await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
      const box=r=>({x:r.x,y:r.y,width:r.width,height:r.height}),boxes={};
      for(const [id,e]of nodes)if(e.nodeType===1&&id!==1)boxes[id]=box(e.getBoundingClientRect());
      const rects=[...nodes.values()].filter(e=>e.nodeType===1).flatMap(e=>[...e.getClientRects()]);
      const hits=${JSON.stringify(fixture.hits)}.map(p=>{const x=p.x+.25,y=p.y+.25;
        const edge=rects.some(r=>x>=r.left-1&&x<=r.right+1&&y>=r.top-1&&y<=r.bottom+1&&Math.min(Math.abs(x-r.left),Math.abs(x-r.right),Math.abs(y-r.top),Math.abs(y-r.bottom))<=1);
        return {x:p.x,y:p.y,id:Number(document.elementFromPoint(x,y)?.dataset.node)||null,edge};});
      return {boxes,hits,document:[scrollX,scrollY]};
    })()`);
    assert.deepEqual(actual.document,[0,0],`${fixture.name}: the document must not scroll`);
    if(process.env.VARTA_ORACLE_DEBUG)console.log(JSON.stringify({case:fixture.name,chrome:actual.boxes,native:fixture.boxes}));
    for(const [id,box]of Object.entries(actual.boxes))for(const key of ['x','y','width','height']){
      assert(Math.abs(box[key]-fixture.boxes[id][key])<BOX_TOLERANCE,`${fixture.name} node ${id} ${key}: chrome ${box[key]}, native ${fixture.boxes[id][key]}`);checks++;
    }
    for(let i=0;i<actual.hits.length;i++) {
      if(actual.hits[i].edge||fixture.hits[i].boundary)continue;
      assert.equal(actual.hits[i].id > 1 ? actual.hits[i].id : null, fixture.hits[i].id > 1 ? fixture.hits[i].id : null,`${fixture.name} hit ${JSON.stringify(actual.hits[i])}`);checks++;
    }
    // An image's accessible role and name, as Chrome exposes the element.
    const {root:{nodeId:document}}=await call('DOM.getDocument',{depth:0});
    for(const expected of fixture.accessibility){
      const {nodeId}=await call('DOM.querySelector',{nodeId:document,selector:`[data-node="${expected.id}"]`});
      const {nodes}=await call('Accessibility.getPartialAXTree',{nodeId,fetchRelatives:false});
      const node=nodes[0];
      const role=node.ignored?null:node.role?.value, name=node.ignored?null:node.name?.value??'';
      const native=expected.role==='image'||expected.role==='img'?'image':expected.role;
      assert.equal(role,native===null?null:native==='image'?'image':native,`${fixture.name} node ${expected.id} role: chrome ${JSON.stringify(node.role)} ignored=${node.ignored}, native ${expected.role}`);
      if(role!==null)assert.equal(name,expected.name,`${fixture.name} node ${expected.id} name`);
      checks++;
    }
    const screenshot=await call('Page.captureScreenshot',{format:'png'});
    if(process.argv[3])await writeFile(`${process.argv[3]}-${fixture.name}.png`,Buffer.from(screenshot.data,'base64'));
    const [width,height]=fixture.raster_size;
    assert.equal(width,Math.round(viewport.width*scale),`${fixture.name}: raster viewport width`);
    assert.equal(height,Math.round(viewport.height*scale),`${fixture.name}: raster viewport height`);
    assert.equal(fixture.raster.length,width*height*4,`${fixture.name}: invalid native raster size`);
    const pixels=await evaluate(`(async()=>{const image=new Image();image.src=${JSON.stringify('data:image/png;base64,'+screenshot.data)};await image.decode();const canvas=document.createElement('canvas');canvas.width=${width};canvas.height=${height};const c=canvas.getContext('2d');c.drawImage(image,0,0);return [...c.getImageData(0,0,${width},${height}).data];})()`);
    // Mean and maximum RGB difference over a device-pixel region, with the
    // native raster composited over the page's white background.
    const compare=r=>{let sum=0,maximum=0,count=0;
      for(let y=Math.max(0,Math.floor(r.y));y<Math.min(height,Math.ceil(r.y+r.height));y++)for(let x=Math.max(0,Math.floor(r.x));x<Math.min(width,Math.ceil(r.x+r.width));x++){
        const at=(y*width+x)*4,white=255-fixture.raster[at+3];
        for(let channel=0;channel<3;channel++){const delta=Math.abs(pixels[at+channel]-Math.min(255,fixture.raster[at+channel]+white));sum+=delta;maximum=Math.max(maximum,delta);count++;}
      }
      return {mean:count?sum/count:0,maximum,pixels:count/3};};
    const whole=compare(fixture.roi);
    assert(whole.pixels>0,`${fixture.name}: empty raster comparison`);
    const images=fixture.accessibility.map(({id})=>{const b=actual.boxes[id];
      const result=compare({x:b.x*scale,y:b.y*scale,width:b.width*scale,height:b.height*scale});return {id,...result};});
    const worst=Math.max(...images.map(i=>i.mean));
    console.log(JSON.stringify({case:fixture.name,rgbMeanDelta:whole.mean,rgbMaxDelta:whole.maximum,imageMeanDeltas:images.map(i=>[i.id,+i.mean.toFixed(3),i.maximum])}));
    summary.push({case:fixture.name,mean:whole.mean,imageMean:worst});
    assert(whole.mean<=MAX_MEAN_RGB_DELTA,`${fixture.name}: RGB mean ${whole.mean}, max ${whole.maximum}`);checks++;
    for(const image of images){assert(image.mean<=MAX_MEAN_RGB_DELTA,`${fixture.name} image ${image.id}: RGB mean ${image.mean}, max ${image.maximum}`);checks++;}
  }
});
const means=summary.map(s=>s.mean),imageMeans=summary.map(s=>s.imageMean);
console.log(`Native/Chrome images (scale ${scale}): ${checks} checks passed; viewport RGB mean max ${Math.max(...means).toFixed(3)}, image RGB mean max ${Math.max(...imageMeans).toFixed(3)}`);
