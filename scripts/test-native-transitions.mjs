// Real Chrome oracle for native_transition_probe: CSS transitions on the
// animation clock. Chrome rebuilds each case's first tree, applies the same
// style changes at the same times and keeps every CSS transition paused at
// the probe's time, measured from the style change that created it (its
// currentTime), so no wall clock is involved. Each probe's expected pixel is
// Chrome's computed background or border color over white, under the box's
// computed opacity; the native pixel is the probe's premultiplied raster
// pixel over white.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {withBrowserPage} from './browser-probe.mjs';
assert(process.argv[2], 'pass native_transition_probe JSON');
const oracle=JSON.parse(await readFile(process.argv[2],'utf8'));
// 8-bit rounding of an interpolated channel on either side.
const MAX_CHANNEL_DELTA=2;
let checks=0;
await withBrowserPage('about:blank',async call=>{
  async function evaluate(expression) {
    const r=await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});
    assert(!r.exceptionDetails,JSON.stringify(r.exceptionDetails));return r.result.value;
  }
  await call('Emulation.setDeviceMetricsOverride',{...oracle.viewport,deviceScaleFactor:1,mobile:false});
  for(const fixture of oracle.cases) {
    await evaluate(`(()=>{
      document.open();document.write('<!doctype html><html><head></head><body style="margin:0;background:white"></body></html>');document.close();
      const nodes=new Map();
      function create(n){const e=n.text!==undefined?document.createTextNode(n.text):document.createElement(n.tag==='root'?'div':n.tag);nodes.set(n.id,e);if(e.nodeType===1){for(const [p,v]of Object.entries(n.style??{}))e.style.setProperty(p,v);for(const c of n.children??[])e.append(create(c));}return e;}
      document.body.append(create(${JSON.stringify(fixture.tree)}));
      window.nodes=nodes;window.created=new Map();
      // Every CSS transition stays paused at its own time since creation.
      window.setTime=at=>{for(const a of document.getAnimations()){if(!created.has(a)){created.set(a,at);a.pause();}a.currentTime=at-created.get(a);}};
      window.flush=()=>{for(const e of nodes.values())if(e.nodeType===1)getComputedStyle(e).color;};
      flush();setTime(0);
    })()`);
    const steps=[...fixture.steps];
    for(const sample of fixture.samples) {
      while(steps.length&&steps[0].at<=sample.at) {
        const step=steps.shift();
        await evaluate(`(()=>{setTime(${step.at});
          for(const c of ${JSON.stringify(step.changes)}){const e=nodes.get(c.node);if(c.value===null)e.style.removeProperty(c.property);else e.style.setProperty(c.property,c.value);}
          flush();setTime(${step.at});})()`);
      }
      const expected=await evaluate(`(()=>{setTime(${sample.at});
        const rgba=t=>{const m=t.match(/rgba?\\(([^)]*)\\)/);const p=m[1].split(/[ ,\\/]+/).filter(Boolean).map(Number);return [p[0],p[1],p[2],p.length>3?p[3]:1];};
        const over=(c,base)=>c.slice(0,3).map((v,i)=>v*c[3]+base[i]*(1-c[3]));
        const box=nodes.get(${oracle.probes[0].node}),opacity=Number(getComputedStyle(box).opacity);
        const out={};
        for(const probe of ${JSON.stringify(oracle.probes)}){const style=getComputedStyle(nodes.get(probe.node));
          const own=rgba(probe.kind==='background'?style.backgroundColor:style.borderLeftColor);
          // The border sits on the box's background, both under the box's opacity.
          const base=probe.kind==='background'?[255,255,255]:over(rgba(getComputedStyle(box).backgroundColor),[255,255,255]);
          const ink=over(own,base);out[probe.node]=ink.map((v,i)=>v*opacity+255*(1-opacity));}
        return {pixels:out,running:document.getAnimations().filter(a=>a.currentTime<a.effect.getComputedTiming().endTime).length>0};
      })()`);
      // A moving case compares positions only (see the probe).
      for(const probe of fixture.moves?[]:oracle.probes) {
        const raster=sample.pixels[probe.node],white=255-raster[3];
        const native=raster.slice(0,3).map(v=>Math.min(255,v+white));
        const chrome=expected.pixels[probe.node];
        for(let channel=0;channel<3;channel++) {
          assert(Math.abs(native[channel]-chrome[channel])<=MAX_CHANNEL_DELTA,
            `${fixture.name} at ${sample.at}ms ${probe.kind}: native ${native}, chrome ${chrome.map(v=>v.toFixed(2))}`);
          checks++;
        }
      }
      assert.equal(sample.animating,expected.running,`${fixture.name} at ${sample.at}ms: native running ${sample.animating}, chrome ${expected.running}`);
      checks++;
    }
    console.log(JSON.stringify({case:fixture.name,samples:fixture.samples.length}));
  }
});
console.log(`Native/Chrome transitions: ${checks} checks passed`);
