import { registry } from '../../../varta-common/browser/registry.mjs';

export function snapshotHostEvent(value) {
  const E=registry.permissions.events,source=value?.source,record=value?.record;
  if(typeof source!=='string'||!source.isWellFormed()||typeof record!=='string'||!record.isWellFormed())throw new TypeError(E.stateError);
  if(record.length>E.recordBytes||new TextEncoder().encode(record).length>E.recordBytes)throw new Error(E.recordError);
  const gesture=value.gesture??false,coalesce=value.coalesce??false;
  if(typeof gesture!=='boolean'||typeof coalesce!=='boolean')throw new TypeError(E.stateError);
  return Object.freeze({source,record,gesture,coalesce});
}

/** Generation-bearing ids are never reused. The permission answer uses this
 * same bounded source state; no source or Promise retains a guest handle. */
export function createEventSources({scheduler,readText,writeResult}) {
  const E=registry.permissions.events,sources=new Map(),names=new Map(),tasks=[];let next=0,taken,notify,lastGesture=false;
  const fail=()=>{throw new Error(E.stateError);};
  const source=(id)=>{if(!Number.isInteger(id)||id<1||!sources.has(id))fail();return sources.get(id);};
  const insert=(name,phase,record)=>{
    if(sources.size>=E.sources)throw new Error(E.queueError);
    if(next>=E.maxId)fail();const id=++next;
    sources.set(id,{name,phase,record,watcher:undefined});if(name!==undefined)names.set(name,id);return id;
  };
  const implementations={
    event_source_open(ptr,length){const name=readText(ptr,length);if(name.length>E.nameBytes||!(/^[A-Za-z][A-Za-z0-9._-]*$/).test(name)||names.has(name))return E.invalid;return insert(name,'armed');},
    event_watch(value,id){
      const serial=scheduler.watchable(value),item=source(id);
      if(item.watcher!==undefined)fail();if(item.phase==='armed')item.watcher=serial;else scheduler.wake(serial);
    },
    event_take(id){
      const item=source(id);if(item.phase!=='ready')fail();item.phase='armed';
      const closed=item.record===undefined;taken={id,text:item.record??''};item.record=undefined;
      if(item.name===undefined||closed)sources.delete(id);
      return closed?E.closed:E.event;
    },
    event_text(id){if(!taken||taken.id!==id){taken=undefined;fail();}const text=taken.text;taken=undefined;return writeResult(text);},
    event_source_close(id){
      const item=Number.isInteger(id)?sources.get(id):undefined;if(!item||item.name===undefined)return;
      names.delete(item.name);item.name=undefined;item.phase='ready';item.record=undefined;
      if(item.watcher!==undefined){scheduler.wake(item.watcher);item.watcher=undefined;}
    },
  };
  return{implementations,
    answer(state){if(state!==undefined&&!Object.hasOwn(registry.permissions.states,state))fail();return insert(undefined,state===undefined?'armed':'ready',state);},
    complete(id,state,arrived){
      if(!Object.hasOwn(registry.permissions.states,state)||!Number.isFinite(arrived)||arrived<0)fail();
      const item=sources.get(id);if(!item)return false;
      if(item.name!==undefined||item.phase!=='armed'||tasks.some(task=>task.id===id))fail();if(tasks.length>=E.tasks)throw new Error(E.queueError);
      tasks.push({id,record:state,arrived,gesture:false,coalesce:false});notify?.();return true;
    },
    push({source:name,record,gesture=false,coalesce=false},expectedId,arrived){
      if(typeof record!=='string'||!record.isWellFormed()||typeof gesture!=='boolean'||typeof coalesce!=='boolean'||!Number.isFinite(arrived)||arrived<0)fail();
      if(new TextEncoder().encode(record).length>E.recordBytes)throw new Error(E.recordError);
      const id=names.get(name);if(id===undefined||expectedId!==undefined&&id!==expectedId)throw new Error(E.noListener.replace('{name}',name));
      if(coalesce){const pending=tasks.find(task=>task.id===id&&task.coalesce);if(pending){pending.record=record;pending.gesture||=gesture;return'coalesced';}}
      if(tasks.length>=E.tasks){if(coalesce)return'dropped';throw new Error(E.queueError);}
      tasks.push({id,record,gesture,coalesce,arrived});notify?.();return'queued';
    },
    arrival(){while(tasks.length&&!sources.has(tasks[0].id))tasks.shift();return tasks[0]?.arrived;},
    deliver(){
      while(tasks.length&&!sources.has(tasks[0].id))tasks.shift();lastGesture=false;
      const task=tasks.shift();if(!task)return false;const item=sources.get(task.id);if(item.phase!=='armed')fail();
      item.phase='ready';item.record=task.record;lastGesture=task.gesture;if(item.watcher!==undefined){scheduler.wake(item.watcher);item.watcher=undefined;}return true;
    },
    get gesture(){return lastGesture;},
    catalog(){return[...names].map(([name,id])=>({name,id}));},
    listening(){return[...sources.values()].some(item=>item.name!==undefined&&item.watcher!==undefined);},
    onReady(callback){notify=callback;},
    close(){sources.clear();names.clear();tasks.length=0;taken=undefined;notify=undefined;},
    get size(){return sources.size;},
  };
}
