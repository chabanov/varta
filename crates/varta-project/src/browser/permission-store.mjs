import { snapshotDecisionFile } from './permission-broker.mjs';

const DATABASE='varta-permissions-v1',VERSION=1,STORE='choices',KEY='decisions';
const empty=()=>snapshotDecisionFile({applications:[]});
const aborted=()=>new DOMException('permission storage aborted','AbortError');

/** A single-capability update preserves other artifacts and is validated against
 * the same bounds as native decision files. Inputs and results never alias. */
export function updateDecisionFile(value,artifact,id,decision) {
  const file=snapshotDecisionFile(value)??empty();
  // Validate the update even if an existing record will be replaced.
  snapshotDecisionFile({applications:[{artifact,decisions:[{id,decision}]}]});
  const applications=file.applications.map(app=>({artifact:app.artifact,decisions:app.decisions.map(record=>({...record}))}));
  let app=applications.find(app=>app.artifact===artifact);
  if(!app){app={artifact,decisions:[]};applications.push(app);}
  const record=app.decisions.find(record=>record.id===id);
  if(record)record.decision=decision;else app.decisions.push({id,decision});
  return snapshotDecisionFile({applications});
}

/** Runtime-only, origin-local storage. One read/write transaction owns the whole
 * update, so separate sessions/tabs cannot overwrite each other's decisions.
 * Storage conveys no authority: the broker still intersects package and ceiling.
 * Connections close on every path; unavailable/corrupt/full storage fails closed. */
export function createDecisionStore({indexedDB=globalThis.indexedDB,name=DATABASE}={}) {
  if(!indexedDB||typeof indexedDB.open!=='function'||typeof name!=='string'||!name.length)throw new TypeError('permission storage unavailable');
  const transact=(write,change,{signal}={},reset=false)=>new Promise((resolve,reject)=>{
    let db,transaction,ended=false,value,failure;
    const finish=(error)=>{
      if(ended)return;ended=true;signal?.removeEventListener('abort',abort);db?.close();
      if(error)reject(error);else resolve(value);
    };
    const abort=()=>{try{transaction?.abort();}catch{}finish(aborted());};
    if(signal?.aborted){abort();return;}signal?.addEventListener('abort',abort,{once:true});
    let request;
    try{request=indexedDB.open(name,VERSION);}catch(error){finish(error);return;}
    request.onblocked=()=>finish(new Error('permission storage blocked'));
    request.onerror=()=>finish(request.error??new Error('permission storage unavailable'));
    request.onupgradeneeded=()=>{
      if(ended){request.transaction.abort();return;}
      request.result.createObjectStore(STORE);
    };
    request.onsuccess=()=>{
      db=request.result;if(ended){db.close();return;}
      db.onversionchange=()=>{try{transaction?.abort();}catch{}finish(new Error('permission storage changed'));};
      try{
        transaction=db.transaction(STORE,write?'readwrite':'readonly');
        transaction.onabort=()=>finish(failure??transaction.error??new Error('permission storage transaction aborted'));
        transaction.onerror=()=>{failure??=transaction.error;};
        transaction.oncomplete=()=>finish();
        const store=transaction.objectStore(STORE),read=store.get(KEY);
        read.onsuccess=()=>{
          try{const current=reset?empty():snapshotDecisionFile(read.result)??empty();value=change(current);if(write)store.put(value,KEY);}
          catch(error){failure=error;transaction.abort();}
        };
      }catch(error){try{transaction?.abort();}catch{}finish(error);}
    };
  });
  return Object.freeze({
    read(options){return transact(false,file=>file,options);},
    set(artifact,id,decision,options){
      // Snapshot/validate before asynchronous database work starts.
      try{updateDecisionFile(undefined,artifact,id,decision);}catch(error){return Promise.reject(error);}
      return transact(true,file=>updateDecisionFile(file,artifact,id,decision),options);
    },
    clear(artifact,options){
      try{if(artifact!==undefined)snapshotDecisionFile({applications:[{artifact,decisions:[]}]});}catch(error){return Promise.reject(error);}
      return transact(true,file=>artifact===undefined?empty():snapshotDecisionFile({applications:file.applications.filter(app=>app.artifact!==artifact)}),options,artifact===undefined);
    },
  });
}
