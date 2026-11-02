import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
const revision='0'.repeat(64),url=`http://127.0.0.1:9443/__varta/${revision}/varta-host.js`;
const source=(await readFile(new URL('./dev/client.js',import.meta.url),'utf8')).replaceAll('import.meta.url',JSON.stringify(url));
function host(fetcher){
  let next=0,options;const timers=new Map(),events=new Map(),elements=new Map(),effects=[];
  const document={querySelector:id=>elements.get(id),body:{append(element){elements.set('#'+element.id,element);}},createElement(){return{setAttribute(){},remove(){elements.delete('#'+this.id);}};}};
  const setTimer=(fn,delay)=>{timers.set(++next,{fn,delay});return next;};
  const bootstrap=value=>{options=value;value.signal.addEventListener('abort',()=>effects.push('stop'));};
  const start=new Function('bootstrap','document','fetch','location','addEventListener','setTimeout','clearTimeout',`${source}\nreturn devBootstrap;`)(bootstrap,document,fetcher,{replace:value=>effects.push(value)},(name,fn)=>events.set(name,fn),setTimer,id=>timers.delete(id));
  start({application:true});
  return{timers,elements,effects,get signal(){return options.signal;},hide(){events.get('pagehide')();},async step(){const item=[...timers].find(([,timer])=>timer.delay===750);assert(item,'next poll');timers.delete(item[0]);await item[1].fn();}};
}
const response=(error=null,id=revision)=>new Response(JSON.stringify({revision:id,error}));
test('dev polling renders diagnostics as text, clears errors and stays serial',async()=>{
  let count=0;const runtime=host(async(address,options)=>{assert.equal(address.href,'http://127.0.0.1:9443/__varta/status');assert.equal(options.credentials,'omit');assert.equal(options.redirect,'error');assert.equal(options.cache,'no-store');return response(++count===1?'<script>unsafe</script>':null);});
  await runtime.step();assert.equal(runtime.elements.get('#varta-dev-errors').textContent,'<script>unsafe</script>');
  assert.equal(runtime.timers.size,1);await runtime.step();assert.equal(runtime.elements.size,0);assert.deepEqual(runtime.effects,[]);
  runtime.hide();assert(runtime.signal.aborted);assert.equal(runtime.timers.size,0);
});
test('dev reload aborts the owned application before navigating to the trusted root',async()=>{
  const runtime=host(async()=>response(null,'1'.repeat(64)));await runtime.step();
  assert.deepEqual(runtime.effects,['stop','http://127.0.0.1:9443/']);assert.equal(runtime.timers.size,0);
});
test('pagehide cancels pending status and ignores late responses',async()=>{
  let answer,signal;const runtime=host((_,options)=>{signal=options.signal;return new Promise(resolve=>answer=resolve);});
  const pending=runtime.step();runtime.hide();assert(signal.aborted);answer(response(null,'1'.repeat(64)));await pending;
  assert.deepEqual(runtime.effects,['stop']);assert.equal(runtime.elements.size,0);assert.equal(runtime.timers.size,0);
});
test('dev failures and malformed or oversized status stop without navigation or unbounded retention',async()=>{
  for(const fetcher of [async()=>{throw new Error('offline');},async()=>new Response('broken'),async()=>new Response('{}'),async()=>new Response('x'.repeat(128*1024+1)),async()=>new Response('',{status:503})]){
    const runtime=host(fetcher);await runtime.step();assert.deepEqual(runtime.effects,['stop']);assert.equal(runtime.timers.size,0);assert(runtime.elements.get('#varta-dev-errors').textContent.includes('stopped'));
  }
});
