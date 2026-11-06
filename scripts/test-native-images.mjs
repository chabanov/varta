// Real Chrome geometry/hit/accessibility/pixel oracle for native_image_probe:
// every case's tree is built in Chrome with the probe's own PNG bytes as
// `img` sources, and its boxes, hit targets, image roles and names, and its
// pixels (the whole viewport and each image's box separately) are compared
// with the native renderer's.
// Usage: cargo run -p varta-view --example native_image_probe -- SCALE > images.json
//        node scripts/test-native-images.mjs images.json [SCREENSHOT_PREFIX]
import assert from 'node:assert/strict';
import {readFile,writeFile} from 'node:fs/promises';
import {createServer} from 'node:http';
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
// MIME types by the fixtures' extensions.
const TYPES={png:'image/png',jpg:'image/jpeg',gif:'image/gif',webp:'image/webp',bmp:'image/bmp',ico:'image/x-icon'};
const BOX_TOLERANCE=0.02;
const summary=[];
// WebCodecs' ImageDecoder needs a secure context: a blank page served from
// the loopback origin (potentially trustworthy) instead of about:blank.
// Images are served from it too, uncached and under a fresh URL for every
// element: Chromium's srcset selection prefers a higher-density candidate
// already in its memory cache (a user agent choice HTML allows), which the
// native renderer does not model.
const server=createServer((request,response)=>{
  const path=new URL(request.url,'http://127.0.0.1').pathname;
  if(path.startsWith('/image/')){const name=path.slice('/image/'.length);
    if(!(name in oracle.images)){response.writeHead(404);response.end();return;}
    response.writeHead(200,{'content-type':TYPES[name.split('.').pop()],'cache-control':'no-store'});response.end(Buffer.from(oracle.images[name]));return;}
  response.writeHead(200,{'content-type':'text/html'});response.end('<!doctype html><title>images</title>');});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const origin=`http://127.0.0.1:${server.address().port}/`;
try{
await withBrowserPage(origin,async call=>{
  // Animated images: Chrome's frame durations (with its short-frame rule)
  // and repetition count against the native timeline.
  for(const [name,native] of Object.entries(oracle.animations??{})){
    const r=await call('Runtime.evaluate',{expression:`(async()=>{const decoder=new ImageDecoder({data:Uint8Array.from(atob(${JSON.stringify(sources[name].split(',')[1])}),c=>c.charCodeAt(0)),type:${JSON.stringify(TYPES[name.split('.').pop()])}});await decoder.tracks.ready;const track=decoder.tracks.selectedTrack,durations=[];for(let i=0;i<track.frameCount;i++){const {image}=await decoder.decode({frameIndex:i});durations.push(image.duration/1000);image.close();}const repetitions=track.repetitionCount;decoder.close();return {durations,repetitions:repetitions===Infinity?null:repetitions};})()`,awaitPromise:true,returnByValue:true});
    assert(!r.exceptionDetails,JSON.stringify(r.exceptionDetails));
    // Chrome plays a frame of 10 ms or less for 100 ms (cc ImageAnimationController); its decoder reports the stored durations.
    const chrome=r.result.value;chrome.durations=chrome.durations.map(ms=>ms<=10?100:ms);
    assert.deepEqual(native.durations,chrome.durations,`${name} frame durations: chrome ${chrome.durations}, native ${native.durations}`);
    assert.equal(native.plays===null?null:native.plays-1,chrome.repetitions,`${name} repetitions: chrome ${chrome.repetitions}, native plays ${native.plays}`);
    console.log(JSON.stringify({animation:name,durations:chrome.durations,repetitions:chrome.repetitions}));checks++;
  }
  async function evaluate(expression) {
    const r=await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});
    assert(!r.exceptionDetails,JSON.stringify(r.exceptionDetails));return r.result.value;
  }
  await call('Emulation.setDeviceMetricsOverride',{...viewport,deviceScaleFactor:scale,mobile:false});
  await call('Accessibility.enable');
  await evaluate(`(async()=>{document.open();document.write("<!doctype html><html><head></head><body></body></html>");document.close();for(const {family,font} of ${JSON.stringify([{family:oracle.family,font:oracle.font},...(oracle.fonts??[])])})document.fonts.add(await new FontFace(family,Uint8Array.from(font)).load());document.body.style.margin='0';document.body.style.background='white';document.documentElement.style.colorScheme='light';})()`);
  for(const fixture of oracle.cases) {
    const actual=await evaluate(`(async()=>{
      document.body.replaceChildren();scrollTo(0,0);const nodes=new Map(),sources=${JSON.stringify(sources)},types=${JSON.stringify(TYPES)};let served=0;const fresh=name=>'/image/'+name+'?u='+(++served)+'-'+Math.random();
      function create(n){const e=n.text!==undefined?document.createTextNode(n.text):document.createElement(n.tag==='root'?'div':n.tag);nodes.set(n.id,e);if(e.nodeType===1){e.dataset.node=n.id;
        for(const [k,v]of Object.entries(n.attributes??{})){if(k==='src'){if(!(v in sources))throw new Error('unknown image '+v);e.setAttribute('src',fresh(v));}else if(k==='srcset')e.setAttribute(k,v.split(',').map(part=>{const [name,...rest]=part.trim().split(' ').filter(Boolean);if(!(name in sources))throw new Error('unknown image '+name);return [fresh(name),...rest].join(' ');}).join(', '));else e.setAttribute(k,v);}
        // Chrome does not implement image-rendering: crisp-edges (it drops the value); CSS Images 3 lets it be the nearest-texel sampling Chrome implements as pixelated, which the native renderer uses for both.
        for(const [p,v]of Object.entries(n.style??{}))e.style.setProperty(p,p==="image-rendering"&&v==="crisp-edges"?"pixelated":v);for(const c of n.children??[])e.append(create(c));}return e;}
      const root=create(${JSON.stringify(fixture.tree)});
      // The native view root is the root stacking context.
      root.style.isolation='isolate';
      document.body.append(root);
      await document.fonts.ready;
      await Promise.all([...document.images].map(i=>i.decode().catch(()=>null)));
      // CSS Images 3 §5.2: pixelated scales down to the closest integer multiple (1) and the rest as smooth; Chrome scales it down nearest-neighbor, so a scaled-down pixelated image is compared as Chrome draws auto.
      for(const image of document.images)if(image.style.imageRendering==="pixelated"&&image.getBoundingClientRect().width*devicePixelRatio<image.naturalWidth)image.style.imageRendering="auto";
      // A native image shown at a later frame: Chrome's own decoder draws
      // that frame into a canvas of the image's natural size, whose lossless
      // PNG the same img then shows, drawn by Chrome's image pipeline.
      for(const {id,image,frame} of ${JSON.stringify(fixture.frames??[])}){
        const element=nodes.get(id),bytes=await (await fetch(sources[image])).arrayBuffer();
        const decoder=new ImageDecoder({data:bytes,type:types[image.split('.').pop()]});
        const {image:videoFrame}=await decoder.decode({frameIndex:frame});
        const canvas=document.createElement('canvas');canvas.width=videoFrame.displayWidth;canvas.height=videoFrame.displayHeight;
        canvas.getContext('2d').drawImage(videoFrame,0,0);videoFrame.close();decoder.close();
        const png=await new Promise(r=>canvas.toBlob(r,'image/png'));element.src=URL.createObjectURL(png);await element.decode();
      }
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
    const replaced=new Set((fixture.frames??[]).map(({id})=>id));
    for(const expected of fixture.accessibility){
      if(replaced.has(expected.id))continue;
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
}finally{server.close();}
const means=summary.map(s=>s.mean),imageMeans=summary.map(s=>s.imageMean);
console.log(`Native/Chrome images (scale ${scale}): ${checks} checks passed; viewport RGB mean max ${Math.max(...means).toFixed(3)}, image RGB mean max ${Math.max(...imageMeans).toFixed(3)}`);
