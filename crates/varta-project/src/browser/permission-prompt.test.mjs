import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createPromptDriver} from './permission-prompt.mjs';
test('one prompt is awaited and its wait ends on answer, throw or rejection',async()=>{
  const controller=new AbortController(),waits=[];let answer;
  const ask=createPromptDriver({signal:controller.signal,waiting:value=>waits.push(value),ask:()=>new Promise(resolve=>answer=resolve)});
  const first=ask({capability:'time.timer'});await assert.rejects(ask({}),/already open/);assert.deepEqual(waits,[true]);answer('allow');assert.equal(await first,'allow');assert.deepEqual(waits,[true,false]);
  for(const callback of [()=>{throw new Error('failed');},async()=>{throw new Error('failed');}]){
    await assert.rejects(createPromptDriver({signal:controller.signal,ask:callback,waiting:value=>waits.push(value)})({}),/failed/);assert.deepEqual(waits.slice(-2),[true,false]);
  }
});
test('abort wins over late answers and removes the pending wait',async()=>{
  const controller=new AbortController(),waits=[];let answer;
  const ask=createPromptDriver({signal:controller.signal,waiting:value=>waits.push(value),ask:(_,options)=>{assert.equal(options.signal,controller.signal);return new Promise(resolve=>answer=resolve);}});
  const first=ask({});controller.abort();await assert.rejects(first,/aborted/);answer('allow');await Promise.resolve();assert.deepEqual(waits,[true,false]);await assert.rejects(ask({}),/aborted/);
});
