import { registry } from '../../../varta-common/browser/registry.mjs';
const P=registry.permissions, kinds=new Map(P.capabilities.map(({id,kind})=>[id,kind]));
const fail=()=>{throw new TypeError('invalid permission decisions');};
const object=(value,keys,required=[])=>value!==null&&typeof value==='object'&&!Array.isArray(value)&&Object.keys(value).every(key=>keys.includes(key))&&required.every(key=>Object.hasOwn(value,key));
const identity=(value)=>typeof value==='string'&&value.startsWith(P.artifactPrefix)&&value.length===P.artifactPrefix.length+P.artifactHex&&/^[0-9a-f]+$/.test(value.slice(P.artifactPrefix.length));

/** Validate all records (including other artifacts) before any artifact fetch.
 * The fresh frozen snapshot contains no mutable host-input aliases. */
export function snapshotDecisionFile(value) {
  if(value===undefined)return undefined;
  if(!object(value,['schemaVersion','applications'],['applications'])||(Object.hasOwn(value,'schemaVersion')?value.schemaVersion??P.schemaVersion:P.schemaVersion)!==P.schemaVersion||!Array.isArray(value.applications)||value.applications.length>P.limits.applications)fail();
  // An explicit null version is invalid, like the native JSON reader.
  if(Object.hasOwn(value,'schemaVersion')&&value.schemaVersion===null)fail();
  const artifacts=new Set(),applications=[];
  for(const application of Array.from(value.applications)){
    if(!object(application,['artifact','decisions'],['artifact','decisions'])||!identity(application.artifact)||artifacts.has(application.artifact)||!Array.isArray(application.decisions)||application.decisions.length>P.limits.decisions)fail();
    artifacts.add(application.artifact);const ids=new Set(),decisions=[];
    for(const record of Array.from(application.decisions)){
      if(!object(record,['id','decision'],['id','decision'])||!kinds.has(record.id)||ids.has(record.id)||!P.decisions.includes(record.decision))fail();
      ids.add(record.id);decisions.push(Object.freeze({id:record.id,decision:record.decision}));
    }
    applications.push(Object.freeze({artifact:application.artifact,decisions:Object.freeze(decisions)}));
  }
  return Object.freeze({schemaVersion:P.schemaVersion,applications:Object.freeze(applications)});
}

export function parsePermissionDescriptor(text) {
  const D=P.descriptor,parts=[];let offset=0;
  for(let i=1;i<D.fields;i++){
    const end=text.indexOf(D.separator,offset);if(end<0)throw new Error(P.events.stateError);
    parts.push(text.slice(offset,end));offset=end+D.separator.length;
  }
  const resource=text.slice(offset);
  if(parts[0]!==D.version||![D.none,D.resource].includes(parts[2])||parts[2]===D.none&&resource!=='')throw new Error(P.events.stateError);
  return {name:parts[1],resource:parts[2]===D.resource?resource:undefined};
}

/** Headless/native-equivalent states. Concrete matching stays with each policy
 * adapter; a stored allow can never widen package/host scope. */
export function createPermissionBroker({manifest,ceiling,artifact,decisions,platform={},v1=true,prompt=false,deferInit=false,covers=()=>false,clock=()=>performance.now(),audit=true,onRecord=()=>{}}) {
  const file=snapshotDecisionFile(decisions),stored=new Map(file?.applications.find(app=>app.artifact===artifact)?.decisions.map(({id,decision})=>[id,decision])??[]);
  const grants=new Map(manifest.capabilities.map(grant=>[grant.id,{...grant,patterns:grant.patterns?.slice(),methods:grant.methods?.slice()}])),states=new Map();
  const entries=[],dismissals=new Map();let sequence=0,dropped=0,initializing=deferInit;const started=clock();
  const platformStates=new Map(Object.entries(platform));
  const combined=(id,state)=>{
    const browser=platformStates.get(id);
    if(state==='denied'||browser==='denied')return 'denied';
    return state==='prompt'||browser==='prompt'?'prompt':state;
  };
  const open=(grant)=>typeof grant==='boolean'?grant:grant?.enabled!==undefined?grant.enabled===true:Array.isArray(grant?.patterns)&&grant.patterns.length>0&&(grant.methods===undefined||grant.methods.length>0);
  const coverage=new Map([...grants].map(([id,grant])=>[id,Boolean(open(grant)&&(ceiling===undefined||Object.hasOwn(ceiling,id)&&open(ceiling[id])))]));
  const coveredGrant=(id)=>coverage.get(id)??false;
  const append=(source,id,resource,outcome)=>{
    if(!audit)return;
    const bytes=new TextEncoder().encode(resource);let end=Math.min(bytes.length,P.limits.auditResourceBytes);
    while(end<bytes.length&&(bytes[end]&0xc0)===0x80)end--;
    const text=new TextDecoder('utf-8',{ignoreBOM:true}).decode(bytes.subarray(0,end));
    if(sequence>=Number.MAX_SAFE_INTEGER)throw new Error('permission audit sequence exhausted');
    const entry={sequence:++sequence,at_ms:Math.max(0,Math.floor(clock()-started)),source,operation:id,resource:text,outcome};
    if(entries.length>=P.limits.audit){entries.shift();dropped++;}entries.push(entry);
  };
  const initial=(id)=>{
    const grant=grants.get(id),decision=stored.get(id);let state='denied',source='policy';
    if(coveredGrant(id)){
      if(grant.required!==false&&decision==='deny')throw new Error(P.requiredDenied.replace('{id}',id));
      if(decision!==undefined){state=decision==='allow'?'granted':'denied';source='stored';}
      else if(prompt){if(grant.required!==false)return undefined;state='prompt';}
      else if(grant.required!==false)state='granted';
    }
    return{state,source};
  };
  const setInitial=(id,{state,source})=>{states.set(id,state);append(source,id,'',combined(id,state));};
  const promptInfo=(id,required=false)=>{
    const grant=grants.get(id);if(!grant||!coveredGrant(id))throw new Error(P.events.stateError);
    return Object.freeze({artifact,capability:id,scope:Object.freeze(grant.patterns?.slice()??[]),purpose:grant.purpose??null,required});
  };
  const answerValid=(answer)=>{if(!['allow','deny','dismiss'].includes(answer))throw new TypeError('invalid permission prompt answer');};
  if(v1&&!deferInit)for(const id of [...grants.keys()].sort()){
    const value=initial(id);if(value===undefined)throw new Error('required permission was not decided before instantiation');setInitial(id,value);
  }
  const granted=(id)=>(!v1||states.get(id)==='granted')&&platformStates.get(id)!=='denied';
  return {
    granted,coveredGrant,
    promptInfo,
    needsPrompt(id){return prompt&&states.get(id)==='prompt';},
    async initialize(ask,save=async()=>{},observe=()=>{}){
      if(!initializing||!prompt||!v1)throw new Error(P.events.stateError);initializing=false;
      for(const id of [...grants.keys()].sort()){
        const value=initial(id);
        if(value!==undefined)setInitial(id,value);
        else{
          const answer=await ask(promptInfo(id,true));answerValid(answer);
          if(answer!=='dismiss'){stored.set(id,answer);await save(id,answer);}
          setInitial(id,{state:answer==='allow'?'granted':'denied',source:'prompt'});
          observe();if(answer!=='allow')throw new Error(P.requiredRefused.replace('{id}',id));continue;
        }
        observe();
      }
    },
    resolve(id,answer){
      answerValid(answer);
      // A revocation while the host waited cannot be undone by a stale Allow.
      if(!prompt||states.get(id)!=='prompt')return{state:combined(id,states.get(id)??'denied'),changed:false};
      let state;
      if(answer==='dismiss'){
        const count=(dismissals.get(id)??0)+1;dismissals.set(id,count);
        state=count>=P.limits.dismissals?'denied':'prompt';if(state==='denied')stored.set(id,'deny');
      }else{stored.set(id,answer);state=answer==='allow'?'granted':'denied';}
      states.set(id,state);append('prompt',id,'',combined(id,state));return{state:combined(id,state),decision:stored.get(id),changed:true};
    },
    revoke(id){
      if(!v1||!states.has(id))return false;
      stored.set(id,'deny');states.set(id,'denied');append('stored',id,'','denied');return true;
    },
    activation(id,resource,answer,covered){
      answerValid(answer);if(!kinds.has(id)||typeof resource!=='string'||typeof covered!=='boolean')throw new Error(P.events.stateError);
      const allowed=answer==='allow'&&covered&&granted(id);
      append('prompt',id,resource,allowed?'granted':'denied');return allowed;
    },
    query(text){
      const {name,resource}=parsePermissionDescriptor(text),kind=kinds.get(name);
      if(kind===undefined||kind==='boolean'&&resource!==undefined)return P.descriptor.invalid;
      const covered=resource===undefined?coveredGrant(name):covers(name,resource);
      return covered?P.states[combined(name,states.get(name)??'denied')]:P.states.denied;
    },
    setPlatform(id,state){
      if(!kinds.has(id)||typeof state!=='string'||!Object.hasOwn(P.states,state))throw new Error(P.events.stateError);
      if(platformStates.get(id)===state)return false;
      platformStates.set(id,state);append('policy',id,'',combined(id,states.get(id)??'denied'));return true;
    },
    operation(id,resource,covered){
      if(!kinds.has(id)||typeof resource!=='string'||typeof covered!=='boolean')throw new Error(P.events.stateError);
      const allowed=covered&&granted(id);
      const source=states.has(id)&&stored.has(id)?'stored':'policy',outcome=allowed?'granted':'denied';
      append(source,id,resource,outcome);
      onRecord({id,resource,covered,source,outcome});return allowed;
    },
    recordOperation({id,resource,covered,source,outcome}){
      // Only the trusted Worker sends these observations. They are audit data,
      // never authority for a page effect. Preserve its decision at authorization
      // time even if revocation/platform changes precede message delivery.
      if(!kinds.has(id)||typeof resource!=='string'||typeof covered!=='boolean'||!['policy','stored'].includes(source)||!['granted','denied'].includes(outcome)||outcome==='granted'&&(!covered||!coveredGrant(id)))throw new Error(P.events.stateError);
      append(source,id,resource,outcome);onRecord({id,resource,covered,source,outcome});
    },
    snapshot(){return{artifact,audit:entries.map(entry=>({...entry})),auditDropped:dropped,decisions:Object.fromEntries([...stored].sort(([a],[b])=>a<b?-1:a>b?1:0))};},
  };
}
