// Compare native resolved declarations with the browser's independent cascade.
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {readFile} from 'node:fs/promises';
import {resolve,sep} from 'node:path';
import {pathToFileURL} from 'node:url';
const [oraclePath,modulePath,selection='all']=process.argv.slice(2);
assert(oraclePath&&modulePath,'Usage: node scripts/test-browser-variables.mjs <native-oracle> <playwright-module> [all|chrome|firefox|webkit]');
const oracle=JSON.parse(await readFile(oraclePath,'utf8'));assert(oracle.variableCases?.length,'fresh native variable cases required');
const {chromium,firefox,webkit}=await import(pathToFileURL(resolve(modulePath)).href);
const channel=process.env.VARTA_CHROMIUM_CHANNEL??'chrome';
const engines=[['chrome',chromium,channel==='chromium'?{chromiumSandbox:true}:{channel,chromiumSandbox:true}],['firefox',firefox,{}],['webkit',webkit,{}]];
assert(['all',...engines.map(([name])=>name)].includes(selection),'unknown browser selection');
const root=resolve(import.meta.dirname,'..');
async function liveVariables(page,name){
 await page.evaluate(async()=>{
  const {createViewDom}=await import('/crates/varta-project/src/browser/view-dom.mjs');
  const {encodeViewBatch}=await import('/crates/varta-project/src/browser/view-codec.mjs');
  const {createFontNamespace}=await import('/crates/varta-project/src/browser/view-fonts.mjs');
  const container=document.createElement('div');container.style.setProperty('--ambient','#f00');container.style.setProperty('--unknown','#000');document.body.append(container);
  const originalMatchMedia=window.matchMedia,mediaLinks=new Map();
  window.matchMedia=function(text){
   const query=originalMatchMedia.call(window,text),add=query.addEventListener.bind(query),remove=query.removeEventListener.bind(query);
   query.addEventListener=(kind,listener,options)=>{if(kind==='change'){if(!mediaLinks.has(query))mediaLinks.set(query,new Set());mediaLinks.get(query).add(listener);}return add(kind,listener,options);};
   query.removeEventListener=(kind,listener,options)=>{if(kind==='change'){mediaLinks.get(query)?.delete(listener);if(!mediaLinks.get(query)?.size)mediaLinks.delete(query);}return remove(kind,listener,options);};
   return query;
  };
  const view=createViewDom({container,fonts:createFontNamespace('variableLive',[':root'])});
  const css=':root{--base:#123;--alias:var(--base);--a:var(--b,var(--c));--b:var(--a);--c:var(--b,#abc);--font:":root";--scheme:#111}.left{--base:#456}.right{--base:#789}.target{--state:#111;position:fixed;top:100px;left:100px;width:20px;height:20px;padding:0;margin:0;color:var(--alias);background-color:var(--state);outline-color:var(--focus,#789);opacity:var(--alpha,.25);font-family:var(--font)}.target:hover{--state:#222}.target:active{--state:#333}.target:checked{--state:#444}.target:disabled{--state:#555}.target:focus-visible{--focus:#def}.root-only{--root-only-literal:#f00;color:var(--ambient,#abc)}.focus-probe{outline-color:var(--focus,#789)}.focus-probe:focus-visible{--focus:#def}.probe{color:var(--base);border:2px solid var(--c)}.scheme{color:var(--scheme)}.ambient{--local:var(--ambient,#def);color:var(--local);background-color:var(--unknown,#abc);border:2px solid var(--ambient,#abc)}@media (min-width:700px){:root{--alpha:.5}}@media (max-width:699px){:root{--alpha:.75}}@media (prefers-color-scheme:dark){:root{--scheme:#222}}';
  const ops=[{name:'stylesheet',sheet:1,text:css}];
  for(const [node,tag,classes,parent] of [[2,'div','left',1],[4,'div','right',1],[3,'input','target',2],[5,'div','probe',2],[6,'span','scheme',4],[7,'div','left',0],[8,'span','probe',7],[9,'div','ambient',1],[10,'input','focus-probe',1]]){
   ops.push({name:'create_element',node,tag},{name:'set_class',node,text:classes});
   if(node===3)ops.push({name:'set_attribute',node,attribute:'type',text:'checkbox'});
   if(parent)ops.push({name:'insert',node,parent,before:0});
  }
  view.apply(encodeViewBatch(ops));globalThis.variableLive={view,container,batch:encodeViewBatch,css,mediaLinks,originalMatchMedia};
 });
 let observations=0;
 const apply=ops=>page.evaluate(ops=>variableLive.view.apply(variableLive.batch(ops)),ops);
 const mediaCount=()=>page.evaluate(()=>[...variableLive.mediaLinks.values()].reduce((sum,listeners)=>sum+listeners.size,0));
 const inspect=async(id,expected)=>{
  await page.evaluate(()=>new Promise(done=>requestAnimationFrame(()=>requestAnimationFrame(done))));
  const observed=await page.evaluate(({id,properties})=>{const style=getComputedStyle(variableLive.view.inspectNode(id));return Object.fromEntries(properties.map(property=>[property,style.getPropertyValue(property)]));},{id,properties:Object.keys(expected)});
  assert.deepEqual(observed,expected,`${name}: live node ${id}`);observations+=Object.keys(expected).length;
 };
 try{
  const media=await mediaCount();assert(media>0);observations++;
  await inspect(3,{color:'rgb(17, 34, 51)','background-color':'rgb(17, 17, 17)','outline-color':'rgb(119, 136, 153)',opacity:'0.5'});
  const font=await page.evaluate(()=>getComputedStyle(variableLive.view.inspectNode(3)).fontFamily);
  assert(font.includes('varta_font_variableLive_0'),`${name}: quoted :root font must retain its packaged name`);observations++;
  await inspect(5,{color:'rgb(68, 85, 102)','border-top-color':'rgb(170, 187, 204)'});
  const rootStyle=await page.evaluate(()=>{const {view,batch}=variableLive,root=view.inspectNode(1),before=getComputedStyle(root).color;view.apply(batch([{name:'set_class',node:1,text:'root-only'}]));return {before,after:getComputedStyle(root).color,custom:getComputedStyle(root).getPropertyValue('--root-only-literal')};});
  assert.equal(rootStyle.after,rootStyle.before,`${name}: class selectors exclude the implicit root`);assert.equal(rootStyle.custom,'');observations+=2;
  await inspect(9,{color:'rgb(221, 238, 255)','background-color':'rgb(170, 187, 204)','border-top-color':'rgb(170, 187, 204)'});
  await page.keyboard.press('Tab');
  await apply([{name:'command',node:10,command:'focus',argument:0}]);
  assert.equal(await page.evaluate(()=>variableLive.view.inspectNode(10).matches(':focus-visible')),true);
  await inspect(10,{'outline-color':'rgb(221, 238, 255)'});
  await apply([{name:'command',node:10,command:'blur',argument:0}]);
  await inspect(10,{'outline-color':'rgb(119, 136, 153)'});
  await page.mouse.move(110,110);await inspect(3,{'background-color':'rgb(34, 34, 34)'});
  await page.mouse.down();await inspect(3,{'background-color':'rgb(51, 51, 51)'});
  await page.mouse.up();await inspect(3,{'background-color':'rgb(68, 68, 68)'});
  await apply([{name:'set_attribute',node:3,attribute:'disabled',text:''}]);
  await inspect(3,{'background-color':'rgb(85, 85, 85)'});
  await apply([{name:'remove_attribute',node:3,attribute:'disabled'}]);
  await page.evaluate(()=>{const {view,batch}=variableLive;view.apply(batch([{name:'set_checked',node:3,checked:false,base_seq:view.snapshot().nodes.find(node=>node.id===3).seq}]));});
  await page.mouse.move(800,600);await inspect(3,{'background-color':'rgb(17, 17, 17)'});
  await apply([{name:'insert',node:5,parent:4,before:0},{name:'insert',node:3,parent:4,before:0},{name:'insert',node:7,parent:4,before:0}]);
  await inspect(5,{color:'rgb(119, 136, 153)'});await inspect(3,{color:'rgb(17, 34, 51)'});await inspect(8,{color:'rgb(68, 85, 102)'});
  await apply([{name:'set_class',node:7,text:'probe'}]);await inspect(7,{color:'rgb(119, 136, 153)'});await inspect(8,{color:'rgb(119, 136, 153)'});
  await page.setViewportSize({width:512,height:768});await inspect(3,{opacity:'0.75'});
  await page.setViewportSize({width:1024,height:768});await inspect(3,{opacity:'0.5'});
  await page.emulateMedia({colorScheme:'dark'});await inspect(6,{color:'rgb(34, 34, 34)'});
  await page.evaluate(()=>variableLive.view.apply(variableLive.batch([{name:'stylesheet',sheet:1,text:variableLive.css.replace('--base:#123','--base:#135')}])));
  assert.equal(await mediaCount(),media);observations++;
  await inspect(3,{color:'rgb(17, 51, 85)'});
  const rejected=await page.evaluate(()=>{try{variableLive.view.apply(variableLive.batch([{name:'stylesheet',sheet:1,text:variableLive.css.replace('--base:#123','--base:#999')},{name:'remove',node:99999}]));return false;}catch{return true;}});
  assert.equal(rejected,true);assert.equal(await mediaCount(),media);observations++;await apply([]);await inspect(3,{color:'rgb(17, 51, 85)'});
  await apply([{name:'stylesheet',sheet:1,text:''},{name:'set_style',node:3,property:'color',text:'var(--alias,#abc)'}]);
  await inspect(3,{color:'rgb(170, 187, 204)'});
  assert.equal(await mediaCount(),0);observations++;
  return observations;
 }finally{
  const remaining=await page.evaluate(()=>{variableLive.view.close();const count=variableLive.mediaLinks.size;window.matchMedia=variableLive.originalMatchMedia;variableLive.container.remove();delete globalThis.variableLive;return count;});assert.equal(remaining,0);
  await page.emulateMedia({colorScheme:'light'});
 }
}
const server=createServer(async(request,response)=>{
 try{
  const name=new URL(request.url,'http://loopback').pathname;
  if(name==='/'){response.setHeader('Content-Type','text/html');response.end('<!doctype html><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src \'none\'; script-src \'self\'; style-src \'none\'"><main></main>');return;}
  if(!['/crates/varta-common/browser/','/crates/varta-project/src/browser/'].some(prefix=>name.startsWith(prefix)))throw Error('unknown asset');
  const path=resolve(root,`.${decodeURIComponent(name)}`);
  const directories=['crates/varta-common/browser','crates/varta-project/src/browser'].map(directory=>resolve(root,directory)+sep);
  if(!path.endsWith('.mjs')||!directories.some(directory=>path.startsWith(directory)))throw Error('unknown module');
  const bytes=await readFile(path);response.setHeader('Content-Type','text/javascript');response.end(bytes);
 }catch{response.writeHead(404).end();}
});
await new Promise(done=>server.listen(0,'127.0.0.1',done));const base=`http://127.0.0.1:${server.address().port}`;
try{
 for(const [name,type,options] of engines){
  if(selection!=='all'&&selection!==name)continue;
  const browser=await type.launch({headless:true,timeout:45000,...options});
  try{
   const page=await browser.newPage({viewport:{width:1024,height:768},colorScheme:'light'}),errors=[];
   page.on('pageerror',error=>errors.push(error.message));
   await page.route('**/*',route=>new URL(route.request().url()).origin===base?route.continue():route.abort());
   await page.goto(base+'/');
   const result=await page.evaluate(async fixtures=>{
    const {createViewDom}=await import('/crates/varta-project/src/browser/view-dom.mjs');
    const {encodeViewBatch}=await import('/crates/varta-project/src/browser/view-codec.mjs');
    const properties=['color','background-color','padding-top','padding-right','width','opacity','display','font-family','border-top-width','border-top-style','border-top-color'];
    const controls=[];
    for(const [name,source] of [
     ['unused-self','--safe:#111;--a:var(--safe,var(--a));'],
     ['cycle-escape','--a:var(--b,var(--c));--b:var(--a);--c:var(--b,#abc);'],
     ['cycle-escape-renamed','--a:var(--c,#abc);--b:var(--c,var(--a));--c:var(--b);'],
     ['invalid-prefix','--a:var(--missing) var(--b);--b:var(--a,#abc);'],
     ['empty-fallback','--a:var(--missing,);--b:var(--a,#abc);'],
    ]){
     const host=document.createElement('div'),shadow=host.attachShadow({mode:'closed'}),child=document.createElement('div');
     const sheet=new CSSStyleSheet();sheet.replaceSync(':host{'+source+'}');shadow.adoptedStyleSheets=[sheet];shadow.append(child);document.body.append(host);
     const computed=getComputedStyle(child);controls.push({name,a:computed.getPropertyValue('--a').trim(),b:computed.getPropertyValue('--b').trim(),c:computed.getPropertyValue('--c').trim()});host.remove();
    }
    const mismatches=[];let checks=0;
    for(const fixture of fixtures){
     const a=document.createElement('div'),b=document.createElement('div');document.body.append(a,b);
     const actual=createViewDom({container:a}),reference=createViewDom({container:b});
     try{
      actual.apply(Uint8Array.from(fixture.batch));
      const operations=[{name:'create_element',node:2,tag:'div'},{name:'set_style',node:2,property:'width',text:'200px'},
       {name:'insert',parent:1,node:2,before:0},{name:'create_element',node:3,tag:'span'},{name:'insert',parent:2,node:3,before:0}];
      for(const [property,text] of Object.entries(fixture.style))operations.push({name:'set_style',node:3,property,text});
      reference.apply(encodeViewBatch(operations));
      const left=getComputedStyle(actual.inspectNode(3)),right=getComputedStyle(reference.inspectNode(3));
      for(const property of properties){
       const normalize=value=>property==='font-family'?value.replace(/varta_font_[A-Za-z0-9_]+_missing/g,'varta_font_missing'):value;
       const observed=normalize(left.getPropertyValue(property)),expected=normalize(right.getPropertyValue(property));
       if(observed!==expected)mismatches.push({case:fixture.name,property,observed,expected,native:fixture.style});checks++;
      }
     }finally{actual.close();reference.close();a.remove();b.remove();}
    }
    return {checks,mismatches,controls};
   },oracle.variableCases);
   console.log(JSON.stringify({browser:name,controls:result.controls}));
   assert.deepEqual(result.mismatches,[],`${name}: native/browser variable resolution`);assert.deepEqual(errors,[]);
   console.log(`${name}: ${oracle.variableCases.length} native variable fixtures, ${result.checks} computed style observations passed`);
   const live=await liveVariables(page,name);assert.deepEqual(errors,[]);
   console.log(`${name}: ${live} live custom-property observations passed (pseudo/media/input/reparent/rollback/removal/ambient/font)`);
  }finally{await browser.close();}
 }
}finally{server.closeAllConnections();await new Promise(done=>server.close(done));}
