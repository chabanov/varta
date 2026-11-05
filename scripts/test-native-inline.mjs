// Real DOM geometry/hit oracle, using exactly the native fixture font.
import assert from 'node:assert/strict';
import {readFile,writeFile} from 'node:fs/promises';
import {withBrowserPage} from './browser-probe.mjs';
import {createFontNamespace,packagedFamilies,faceDescriptors} from '../crates/varta-project/src/browser/view-fonts.mjs';
assert(process.argv[2], 'pass native_inline_probe JSON');
const oracle=JSON.parse(await readFile(process.argv[2],'utf8'));
const scale=oracle.scale??1;
assert(Number.isFinite(scale)&&scale>=0.25&&scale<=8,'invalid oracle scale');
const viewport=oracle.viewport??{width:100,height:100};
assert(Number.isInteger(viewport.width)&&viewport.width>0&&viewport.width<=4096&&Number.isInteger(viewport.height)&&viewport.height>0&&viewport.height<=4096,'invalid oracle viewport');
// One face ({family, font}) or, from native_font_probe, every packaged face
// with its weight, style and generic families ({fonts: [{family, weight,
// style, generics, font}]}).
const faces=oracle.fonts??[{family:oracle.family,font:oracle.font}];
assert(faces.length>0&&faces.every(face=>typeof face.family==='string'&&Array.isArray(face.font)),'invalid oracle fonts');
// Packaged faces resolve exactly as the browser applier resolves them: its
// own font namespace (private family names, generic aliases, the default
// family on the view root) and its FontFace descriptors.
let registered=faces.map(face=>({family:face.family,font:face.font,descriptors:{}})),fontFamily=value=>value,defaultFamily;
if(oracle.fonts!==undefined){
  const descriptors=faces.map(face=>({key:face.family,weight:face.weight,style:face.style,generics:face.generics??[]}));
  const namespace=createFontNamespace('oracle',...packagedFamilies(descriptors));
  registered=faces.map((face,index)=>({family:namespace.family(face.family),font:face.font,descriptors:faceDescriptors(descriptors[index])}));
  fontFamily=value=>namespace.value(value);defaultFamily=namespace.defaultFamily;
}
const applied=node=>({...node,...(node.style?{style:Object.fromEntries(Object.entries(node.style).map(([property,value])=>[property,property==='font-family'?fontFamily(value):value]))}:{}),...(node.children?{children:node.children.map(applied)}:{})});
let checks=0;
const MAX_MEAN_RGB_DELTA=2; // Same bound as the native/GPU scene probes.
await withBrowserPage('about:blank',async call=>{
  async function evaluate(expression) {
    const r=await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});
    assert(!r.exceptionDetails,JSON.stringify(r.exceptionDetails));return r.result.value;
  }
  await call('Emulation.setDeviceMetricsOverride',{...viewport,deviceScaleFactor:scale,mobile:false});
  await evaluate(`(async()=>{document.open();document.write("<!doctype html><html><head></head><body></body></html>");document.close();for(const face of ${JSON.stringify(registered)})document.fonts.add(await new FontFace(face.family,Uint8Array.from(face.font),face.descriptors).load());${defaultFamily===undefined?'':`document.body.style.fontFamily=${JSON.stringify(defaultFamily)};`}document.body.style.margin='0';document.body.style.background='white';document.documentElement.style.colorScheme='light';})()`);
  for(const fixture of oracle.cases) {
    const actual=await evaluate(`(()=>{
      document.body.replaceChildren();const nodes=new Map();
      function create(n){const e=n.text!==undefined?document.createTextNode(n.text):document.createElement(n.tag==='root'?'div':n.tag);nodes.set(n.id,e);if(e.nodeType===1){e.dataset.node=n.id;for(const [p,v]of Object.entries(n.style??{}))e.style.setProperty(p,v);for(const c of n.children??[])e.append(create(c));}return e;}
      document.body.append(create(${JSON.stringify(applied(fixture.tree))}));
      const box=r=>({x:r.x,y:r.y,width:r.width,height:r.height}),boxes={};
      for(const [id,e]of nodes)if(e.nodeType===1&&id!==1&&e.tagName!=='BR')boxes[id]=box(e.getBoundingClientRect());
      const rects=[...nodes.values()].filter(e=>e.nodeType===1).flatMap(e=>[...e.getClientRects()]);
      // Chrome's footprint also differs in the one-pixel band just above a line
      // box, where an earlier line's decorated fragment overlaps it.
      // Approximate each line box by its text fragments' common vertical extent
      // and their horizontal hull.
      const lines=new Map();
      for(const e of nodes.values())if(e.nodeType===3){const r=document.createRange();r.selectNodeContents(e);for(const t of r.getClientRects()){const k=t.top+':'+t.bottom,l=lines.get(k);lines.set(k,l?{top:t.top,bottom:t.bottom,left:Math.min(l.left,t.left),right:Math.max(l.right,t.right)}:{top:t.top,bottom:t.bottom,left:t.left,right:t.right});}}
      const hits=${JSON.stringify(fixture.hits)}.map(p=>{const x=p.x+.25,y=p.y+.25;
        // Chrome tests a physical pixel footprint at fractional coordinates.
        // Compare interiors here; native tests separately check exact edges.
        const edge=rects.some(r=>x>=r.left-1&&x<=r.right+1&&y>=r.top-1&&y<=r.bottom+1&&Math.min(Math.abs(x-r.left),Math.abs(x-r.right),Math.abs(y-r.top),Math.abs(y-r.bottom))<=1)
          ||[...lines.values()].some(l=>x>=l.left-1&&x<=l.right+1&&y>=l.top-1&&y<l.top);
        return {x:p.x,y:p.y,id:Number(document.elementFromPoint(x,y)?.dataset.node)||null,edge};});
      return {boxes,hits};
    })()`);
    for(const [id,box]of Object.entries(actual.boxes))for(const key of ['x','y','width','height']){
      assert(Math.abs(box[key]-fixture.boxes[id][key])<0.02,`${fixture.name} node ${id} ${key}: chrome ${box[key]}, native ${fixture.boxes[id][key]}`);checks++;
    }
    for(let i=0;i<actual.hits.length;i++) {
      if(actual.hits[i].edge||fixture.hits[i].boundary)continue;
      // Compare guest elements; document/implicit-root background is host-owned.
      assert.equal(actual.hits[i].id > 1 ? actual.hits[i].id : null, fixture.hits[i].id > 1 ? fixture.hits[i].id : null,`${fixture.name} hit ${JSON.stringify(actual.hits[i])}`);checks++;
    }
    const screenshot=await call('Page.captureScreenshot',{format:'png'});
    if(process.argv[3])await writeFile(`${process.argv[3]}-${fixture.name}.png`,Buffer.from(screenshot.data,'base64'));
    if(fixture.raster){
      const [width,height]=fixture.raster_size;
      assert.equal(width,Math.round(viewport.width*scale),`${fixture.name}: raster viewport width`);
      assert.equal(height,Math.round(viewport.height*scale),`${fixture.name}: raster viewport height`);
      assert.equal(fixture.raster.length,width*height*4,`${fixture.name}: invalid native raster size`);
      const pixels=await evaluate(`(async()=>{const image=new Image();image.src=${JSON.stringify('data:image/png;base64,'+screenshot.data)};await image.decode();const canvas=document.createElement('canvas');canvas.width=${width};canvas.height=${height};const c=canvas.getContext('2d');c.drawImage(image,0,0);return [...c.getImageData(0,0,${width},${height}).data];})()`);
      const r=fixture.roi;let sum=0,maximum=0,count=0;
      for(let y=Math.max(0,Math.floor(r.y));y<Math.min(height,Math.ceil(r.y+r.height));y++)for(let x=Math.max(0,Math.floor(r.x));x<Math.min(width,Math.ceil(r.x+r.width));x++){
        const at=(y*width+x)*4,white=255-fixture.raster[at+3];
        for(let channel=0;channel<3;channel++){const delta=Math.abs(pixels[at+channel]-Math.min(255,fixture.raster[at+channel]+white));sum+=delta;maximum=Math.max(maximum,delta);count++;}
      }
      assert(count>0,`${fixture.name}: empty raster comparison`);
      const mean=sum/count;assert(mean<=MAX_MEAN_RGB_DELTA,`${fixture.name}: RGB mean ${mean}, max ${maximum}`);
      console.log(JSON.stringify({case:fixture.name,rgbMeanDelta:mean,rgbMaxDelta:maximum,pixelCount:count/3}));checks++;
    }
    const samples=await evaluate(`(async()=>{const image=new Image();image.src=${JSON.stringify('data:image/png;base64,'+screenshot.data)};await image.decode();const canvas=document.createElement('canvas');canvas.width=image.width;canvas.height=image.height;const c=canvas.getContext('2d');c.drawImage(image,0,0);return ${JSON.stringify(fixture.samples)}.map(p=>[...c.getImageData(p.x,p.y,1,1).data]);})()`);
    for(let i=0;i<samples.length;i++) {
      const rgba=fixture.samples[i].rgba,white=255-rgba[3];
      const expected=[...rgba.slice(0,3).map(c=>Math.min(c+white,255)),255];
      assert.deepEqual(samples[i],expected,`${fixture.name} ink ${JSON.stringify(fixture.samples[i])}`);checks++;
    }
  }
});
console.log(`Native/Chrome inline: ${checks} checks passed`);
