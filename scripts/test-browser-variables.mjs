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
  }finally{await browser.close();}
 }
}finally{server.closeAllConnections();await new Promise(done=>server.close(done));}
