import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createUiTiming} from './ui-timing.mjs';

function clock(limit=2){
  let time=0,id=0;
  const frames=new Map(),samples=[];
  const timing=createUiTiming({now:()=>time,request:f=>{frames.set(++id,f);return id;},cancel:id=>frames.delete(id),emit:s=>samples.push(s),limit});
  return {timing,samples,frames,at:t=>{time=t;},frame:t=>{time=t;const batch=[...frames.values()];frames.clear();for(const f of batch)f();}};
}
test('timing correlates a worker checkpoint with two frame opportunities',()=>{
  const c=clock();c.at(2);c.timing.begin(1,'input',1);c.timing.applied(3);
  c.at(8);c.timing.end(2);assert.equal(c.frames.size,0);
  c.timing.end(1);c.frame(16);assert.equal(c.samples.length,0);c.frame(32);
  const s=c.samples[0];assert.equal(s.queueMs,1);assert.equal(s.workerRoundtripMs,6);
  assert.equal(s.applyMs,3);assert.equal(s.totalMs,31);assert.equal(s.afterCheckpointMs,24);
  assert.equal(s.boundary,'enqueue-to-frame-opportunity');
});
test('background tabs retain only bounded samples; shutdown cancels callbacks',()=>{
  const c=clock(1);
  for(let i=1;i<=3;i++){c.timing.begin(i,'click',0);c.timing.end(i);}
  assert.equal(c.frames.size,1);c.frame(16);c.frame(32);
  assert.equal(c.samples.length,1);assert.equal(c.samples[0].dropped,2);
  c.timing.begin(4,'click',32);c.timing.end(4);c.timing.close();
  assert.equal(c.frames.size,0);c.frame(48);assert.equal(c.samples.length,1);
});

test('a checkpoint between frame callbacks waits for its own second opportunity',()=>{
  const c=clock();c.timing.begin(1,'click',0);c.timing.end(1);c.frame(16);
  c.at(20);c.timing.begin(2,'input',20);c.timing.end(2);c.frame(32);
  assert.equal(c.samples.length,1);assert.equal(c.samples[0].id,1);
  c.frame(48);assert.equal(c.samples.length,2);assert.equal(c.samples[1].id,2);
});

test('host visual feedback is measured without creating a guest turn',()=>{
  const c=clock();c.timing.visual('pointerover',0);c.frame(16);c.frame(32);
  assert.equal(c.samples[0].kind,'visual:pointerover');
  assert.equal(c.samples[0].workerRoundtripMs,0);assert.equal(c.samples[0].totalMs,32);
});
