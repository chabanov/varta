// The generated descriptor above is part of this trusted module, not fetched
// authority. Deploy/pin this loader through the application's CSP/SRI boundary.
async function verifiedFile(base, item, signal) {
  const response = await fetch(new URL(item.name, base), { signal, credentials: "omit", redirect: "error" });
  if (!response.ok) throw new Error(`bundle file unavailable: ${item.name}`);
  const reader = response.body.getReader();
  const chunks = [];
  let size = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > item.bytes) throw new Error(`bundle length mismatch: ${item.name}`);
      chunks.push(value);
    }
  } finally { await reader.cancel().catch(() => {}); }
  if (size !== item.bytes) throw new Error(`bundle length mismatch: ${item.name}`);
  const bytes = new Uint8Array(size);
  let at = 0;
  for (const chunk of chunks) { bytes.set(chunk, at); at += chunk.byteLength; }
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  const identity = `sha256:${Array.from(digest, (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
  if (identity !== item.identity) throw new Error(`bundle integrity mismatch: ${item.name}`);
  return bytes;
}

/** One R1 execution. The entire instance is terminated on deadline or abort.
 * Host code and the descriptor are trusted; no arbitrary unprepared image API.
 * Deadline includes loading, verification, compilation and guest execution. */
const activeWorkerUrls = new Set();
let workerPolicy;
function trustedWorkerUrl(url) {
  if (!globalThis.trustedTypes) return url;
  workerPolicy ??= trustedTypes.createPolicy(descriptor.trustedTypesPolicy, {
    createScriptURL(value) {
      if (!activeWorkerUrls.has(value)) throw new Error("unverified worker URL");
      return value;
    },
  });
  return workerPolicy.createScriptURL(url);
}
function resolveBudgets(ceiling) {
  if (ceiling !== undefined && (ceiling === null || typeof ceiling !== "object" || Array.isArray(ceiling))) throw new TypeError("invalid host budget ceiling");
  const limits = Object.fromEntries(Object.entries(ceiling ?? {}));
  for (const [name, value] of Object.entries(limits)) {
    if (!Object.hasOwn(descriptor.budgetRules, name) || !Number.isSafeInteger(value) || value < 1 || value > descriptor.budgetRules[name].maximum) throw new RangeError("invalid host budget ceiling");
  }
  if (!descriptor.turnBudgets) return descriptor.budgets;
  const budgets = {};
  for (const [name, rule] of Object.entries(descriptor.budgetRules)) budgets[name] = Math.min(descriptor.requestedBudgets[name], Object.hasOwn(limits, name) ? limits[name] : rule.production);
  if (budgets.memoryBytes !== descriptor.budgets.memoryBytes) throw new Error("this private image cannot enforce a different memory ceiling");
  return budgets;
}
// A KV path-pattern ceiling (`kv.read`, `kv.write`), validated as a native
// host ceiling's grant: at most `maxPatterns` patterns, each at most
// `maxPatternBytes` UTF-8 bytes, not blank or wildcard-only, without NUL, an
// absolute path without a `..` component. Patterns that no virtual path can
// match are valid and match nothing, as natively.
const RUST_WHITESPACE=/^\p{White_Space}+|\p{White_Space}+$/gu,WILDCARDS=/^[*?]+|[*?]+$/g;
function kvCeiling(value){
  const K=descriptor.kv,invalid=()=>new TypeError("invalid KV permission ceiling");
  if(!value||typeof value!=="object"||Array.isArray(value)||Object.keys(value).some((key)=>key!=="patterns")||!Array.isArray(value.patterns)||value.patterns.length>K.maxPatterns)throw invalid();
  const patterns=Array.from(value.patterns);
  for(const pattern of patterns){
    if(typeof pattern!=="string"||new TextEncoder().encode(pattern).length>K.maxPatternBytes||pattern.replace(RUST_WHITESPACE,"")===""||pattern.replace(WILDCARDS,"")===""||pattern.includes("\0"))throw invalid();
    if(!pattern.startsWith(K.separator)||pattern.split(K.separator).includes(".."))throw invalid();
  }
  return {patterns};
}
function resolvePermissions(ceiling) {
  if (ceiling === undefined) return undefined;
  if (ceiling === null || typeof ceiling !== "object" || Array.isArray(ceiling)) throw new TypeError("invalid permission ceiling");
  const snapshot = Object.create(null);
  for (const [id, value] of Object.entries(ceiling)) {
    if (id === 'env.get' || id === 'env.has') {
      if (!value || typeof value !== 'object' || Array.isArray(value) || Object.keys(value).some(key=>key!=='patterns') || !Object.hasOwn(value,'patterns')) throw new TypeError('invalid environment permission ceiling');
      const input=value.patterns;
      if (!Array.isArray(input) || input.length>descriptor.environment.maxPatterns) throw new TypeError('invalid environment permission ceiling');
      const patterns=Array.from(input);
      environment.createEnvironmentPolicy(patterns);
      snapshot[id] = {patterns}; continue;
    }
    if (id === descriptor.network.capability) {
      snapshot[id] = networkPolicy({capabilities:[]}).scope(value); continue;
    }
    if(descriptor.kv&&Object.values(descriptor.kv.capabilities).includes(id)){snapshot[id]=kvCeiling(value);continue;}
    if(id===descriptor.view.capabilities.openUrl){
      if(!value||typeof value!=="object"||Array.isArray(value)||Object.keys(value).some((key)=>key!=="patterns")||!Array.isArray(value.patterns)||value.patterns.length>descriptor.network.maxPatterns)throw new TypeError("invalid view permission ceiling");
      const patterns=Array.from(value.patterns);networkPolicy({capabilities:[]}).scope({patterns:patterns.filter((pattern)=>pattern!==descriptor.view.capabilities.anyHttps)});
      snapshot[id]={patterns};continue;
    }
    if (!descriptor.booleanCapabilities.includes(id) || typeof value !== "boolean") throw new TypeError("invalid boolean permission ceiling");
    snapshot[id] = value;
  }
  return snapshot;
}
function execute({ signal, budgetCeiling, permissionCeiling, decisions, decisionStore, environmentInputs, networkReplay, onPrompt, onAudit, container, mode="run", onControl=()=>{}, onReady=()=>{}, onTurn=()=>{}, onLog=()=>{}, onFrameTiming } = {}) {
  return new Promise((resolve, reject) => {
    // A recorded network log (its JSON text) replaces Fetch; it is
    // validated before anything loads.
    const replayLog = networkReplay === undefined ? undefined : snapshotNetworkLog(networkReplay);
    if(onFrameTiming!==undefined&&typeof onFrameTiming!=="function")throw new TypeError("invalid UI timing callback");
    const timing=onFrameTiming===undefined?undefined:uiTiming.createUiTiming({now:()=>performance.now(),request:f=>requestAnimationFrame(f),cancel:id=>cancelAnimationFrame(id),emit:sample=>{try{onFrameTiming(sample);}catch(error){finish(error);}},limit:descriptor.view.limits.events});
    const budgets = resolveBudgets(budgetCeiling);
    const permissions = resolvePermissions(permissionCeiling);
    let storedDecisions=permissionBroker.snapshotDecisionFile(decisions);
    const environmentSnapshot=environment.snapshotEnvironment(environmentInputs);
    if(decisionStore!==undefined&&(decisions!==undefined||!decisionStore||typeof decisionStore.read!=='function'||typeof decisionStore.set!=='function'))throw new TypeError('invalid permission decision store');
    const store=decisionStore===undefined?undefined:{read:decisionStore.read.bind(decisionStore),set:decisionStore.set.bind(decisionStore)};
    if(onPrompt!==undefined&&typeof onPrompt!=='function')throw new TypeError('invalid permission prompt callback');
    const prompting=mode==='application'&&descriptor.turnBudgets&&onPrompt!==undefined;
    const controller = new AbortController();
    let worker, blobUrl, settled = false, guestStarted = false, renderer, headless, ready=false,consenting=false, inFlight, eventId=0, lastBatch=0,lastCheckpoint=0;
    const assetUrls = new Map();
    const fontFaces=[],fontData=[];
    let broker;
    let controlId=0;
    let storageWrites=Promise.resolve();
    const persist=(id,decision)=>{
      if(!store)return Promise.resolve();
      storageWrites=storageWrites.then(()=>{if(settled)throw new Error('browser application ended');return store.set(descriptor.original.identity,id,decision,{signal:controller.signal});});
      storageWrites.catch(()=>{});return storageWrites;
    };
    const controls=new Map(),revocations=new Map();
    let navigation,pauseId=0;
    const navigate=(url,ui)=>{
      const id=descriptor.view.capabilities.openUrl,S=descriptor.view.statuses;
      if(!ready||navigation||consenting||ui.openDecision(url)==='denied'){
        broker.operation(id,url,false);return S.not_allowed;
      }
      const open=()=>{if(settled)return S.not_allowed;try{window.open(url,'_blank','noopener,noreferrer');return S.ok;}catch{return S.refused_by_platform;}};
      if(ui.openDecision(url)==='granted')return broker.operation(id,url,true)&&!settled?open():S.not_allowed;
      if(!prompting){broker.operation(id,url,false);return S.not_allowed;}
      const request=++pauseId;if(!Number.isSafeInteger(request)){finish(new Error('host pause sequence exhausted'));return S.not_allowed;}
      let resolve,reject;const waiting=new Promise((done,failed)=>{resolve=done;reject=failed;});
      const held={id:request,resolve,reject,paused:false};navigation=held;
      return(async()=>{try{
        worker.postMessage({type:'host_pause',id:request});await waiting;if(settled)return S.not_allowed;
        if(ui.openDecision(url)==='denied'){broker.operation(id,url,false);return S.not_allowed;}
        const info=Object.freeze({...broker.promptInfo(id),scope:Object.freeze([url]),activation:true});
        const answer=await ask(info);if(settled)return S.not_allowed;
        const allowed=broker.activation(id,url,answer,ui.openDecision(url)!=='denied');
        if(onAudit)onAudit(broker.snapshot().audit.at(-1));
        return allowed&&!settled?open():S.not_allowed;
      }catch(error){finish(error);return S.not_allowed;}
      finally{
        if(navigation===held){navigation=undefined;if(held.paused&&!settled)worker.postMessage({type:'host_resume',id:request});schedulePump();}
      }})();
    };
    let sourceCatalog=new Map(),maxSourceId=0,deliveryId=0,deliveryPending=0,deliveryScheduled=false;
    const deliveryStaging=[],deliveryBatches=new Map();
    const flushDeliveries=()=>{
      deliveryScheduled=false;if(settled||!deliveryStaging.length)return;
      const id=++deliveryId;
      if(!Number.isSafeInteger(id)){finish(new Error('host event batch sequence exhausted'));return;}
      const items=deliveryStaging.splice(0);
      deliveryBatches.set(id,items);
      try{worker.postMessage({type:'host_events',id,events:items.map(({event,sourceId,arrived})=>({event,sourceId,arrived}))});}catch(error){finish(error);}
    };
    const platform={},permissionListeners=[];
    const queue=viewQueue.createViewQueue();
    let timer,deadlineAt,startupRemaining;
    const arm=(delay=budgets.deadlinePerTurnMs,message="browser execution deadline exceeded")=>{clearTimeout(timer);deadlineAt=performance.now()+delay;timer=setTimeout(()=>finish(new Error(message)),delay);};
    const ask=permissionPrompt.createPromptDriver({ask:onPrompt,signal:controller.signal,waiting:(value)=>{
      if(value){if(!ready)startupRemaining=Math.max(0,deadlineAt-performance.now());clearTimeout(timer);}
      else if(!ready&&!settled)arm(startupRemaining);
    }});
    const pump=()=>{
      if(mode!=="application"||!ready||consenting||navigation||inFlight||settled||!queue.length)return;
      const event=queue.shift(),record=viewCodec.decodeViewEvent(event.bytes);
      if(record.kind==="command_result"&&record.key==="copy_text")renderer.complete(record.seq);
      inFlight=++eventId;
      timing?.begin(inFlight,record.kind,event.arrived-performance.timeOrigin);
      worker.postMessage({type:"view_event",id:inFlight,bytes:event.bytes,gesture:event.gesture,arrived:event.arrived,selectedOption:event.selectedOption},[event.bytes.buffer]);
    };
    let pumpPending=false;
    const schedulePump=()=>{
      if(pumpPending)return;pumpPending=true;
      queueMicrotask(()=>{pumpPending=false;try{pump();}catch(error){finish(error);}});
    };
    const enqueue=(event)=>{
      if(settled)return;
      if(mode!=="application"){
        const record=viewCodec.decodeViewEvent(event.bytes);if(record.kind==="command_result"&&record.key==="copy_text")renderer?.complete(record.seq);return;
      }
      try{queue.push({...event,arrived:performance.timeOrigin+performance.now()});schedulePump();}catch(error){finish(error);}
    };
    const logs = [];
    let streamedBytes = 0;
    const finish = (error, result) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      signal?.removeEventListener("abort", aborted);
      controller.abort();
      queue.clear();timing?.close();renderer?.freeze();
      for(const url of assetUrls.values())URL.revokeObjectURL(url);assetUrls.clear();
      for(const face of fontFaces)document.fonts.delete(face);fontFaces.length=0;fontData.length=0;
      for(const remove of permissionListeners)remove();permissionListeners.length=0;
      worker?.terminate();
      for(const control of controls.values())control.reject(error??new Error('browser application ended'));controls.clear();
      navigation?.reject(error??new Error('browser application ended'));
      for(const item of [...deliveryStaging,...[...deliveryBatches.values()].flat()])item.reject(error??new Error('browser application ended'));
      deliveryStaging.length=0;deliveryBatches.clear();sourceCatalog.clear();
      if (blobUrl) { activeWorkerUrls.delete(blobUrl); URL.revokeObjectURL(blobUrl); }
      const report=broker?.snapshot()??{artifact:descriptor.original.identity,audit:[],auditDropped:0,decisions:{}};
      if (error) { error.guestStarted = guestStarted; error.logs = [...logs]; error.permissions=report; reject(error); } else resolve({...result,permissions:report});
    };
    const aborted = () => finish(new Error("browser execution aborted"));
    arm();
    if (signal?.aborted) { aborted(); return; }
    signal?.addEventListener("abort", aborted, { once: true });
    (async () => {
      if(store)storedDecisions=permissionBroker.snapshotDecisionFile(await store.read({signal:controller.signal}));
      if(settled)return;
      const base = new URL("./", import.meta.url);
      // Every response is bounded and verified before Worker creation. In
      // particular, Workers do not follow an unchecked URL/import graph.
      const [original, image, manifest, script, kvCodec, viewEngineBytes, ...resourceBytes] = await Promise.all(
        [descriptor.original, descriptor.image, descriptor.manifest, descriptor.worker, descriptor.kvCodec, descriptor.viewEngine, ...descriptor.resources.map((resource)=>resource.file)]
          .map((item) => item===undefined?undefined:verifiedFile(base, item, controller.signal)));
      if (settled) return;
      const packageManifest=JSON.parse(new TextDecoder("utf-8",{fatal:true}).decode(manifest));
      const clipboard=descriptor.view.capabilities.clipboard;
      if(descriptor.turnBudgets&&(permissions===undefined||permissions[clipboard]===true)&&packageManifest.capabilities.some(grant=>grant.id===clipboard&&grant.enabled===true)&&navigator.permissions?.query){
        try{
          const status=await navigator.permissions.query({name:'clipboard-write'});
          if(settled)return;
          if(typeof status.state==='string'&&Object.hasOwn(descriptor.permissions.states,status.state)){
            platform[clipboard]=status.state;
            const changed=()=>{if(settled||typeof status.state!=='string'||!Object.hasOwn(descriptor.permissions.states,status.state))return;try{platform[clipboard]=status.state;const updated=broker?.setPlatform(clipboard,status.state);if(updated&&onAudit)onAudit(broker.snapshot().audit.at(-1));if(!settled)worker?.postMessage({type:'permission_platform',id:clipboard,state:status.state});}catch(error){finish(error);}};
            if(status.addEventListener){status.addEventListener('change',changed);permissionListeners.push(()=>status.removeEventListener('change',changed));}
          }
        }catch{} // Unsupported browser permission names retain broker-only state.
      }
      if(settled)return;
      const observe=(entry=broker.snapshot().audit.at(-1))=>{if(onAudit)onAudit(entry);if(settled)throw new Error('browser application ended');};
      const save=async(id,decision)=>{
        if(settled)throw new Error('browser application ended');
        storedDecisions=permissionStore.updateDecisionFile(storedDecisions,descriptor.original.identity,id,decision);
        await persist(id,decision);
        if(settled)throw new Error('browser application ended');
      };
      broker=permissionBroker.createPermissionBroker({manifest:packageManifest,ceiling:permissions,artifact:descriptor.original.identity,decisions:storedDecisions,platform,v1:descriptor.turnBudgets,prompt:prompting,deferInit:prompting,onRecord:()=>{try{observe();}catch(error){finish(error);}}});
      if(prompting)await broker.initialize(ask,save,observe);
      else if(onAudit)for(const entry of broker.snapshot().audit)onAudit({...entry});
      if(settled)return;
      // Faces arrive in the resource section's canonical order: the first
      // family is the default family and each face's generic families map to
      // its family, exactly as the native shell resolves them.
      const fonts=viewFonts.createFontNamespace(crypto.randomUUID().replaceAll('-',''),...viewFonts.packagedFamilies(descriptor.resources.filter((resource)=>resource.font)));
      // Decode bounded resources before creating DOM nodes or entering guest code.
      // Neither a logical name nor an HTTP Content-Type supplies fetch authority.
      for(let index=0;index<descriptor.resources.length;index++){
        const resource=descriptor.resources[index];
        if(resource.font){
          const face=new FontFace(fonts.family(resource.key),resourceBytes[index],viewFonts.faceDescriptors(resource));await face.load();
          if(settled)return;fontFaces.push(face);fontData.push({family:resource.key,bytes:new Uint8Array(resourceBytes[index]),generics:resource.generics??[]});
        }else{
          const blob=new Blob([resourceBytes[index]],{type:resource.mediaType});
          const bitmap=await createImageBitmap(blob);bitmap.close();
          if(settled)return;
          assetUrls.set(resource.key,URL.createObjectURL(blob));
        }
      }
      // The view style engine (Rust `varta-style` as Wasm) decides every
      // style input of the page's view model, like the Worker's.
      const viewEngine=descriptor.hasView?await WebAssembly.compile(viewEngineBytes):undefined;
      if(settled)return;
      if(descriptor.hasView){
        const ui=viewPolicy.createViewPolicy(packageManifest,permissions,broker.granted);
        if(container){
          for(const face of fontFaces)document.fonts.add(face);
          renderer=viewDom.createViewDom({container,viewEngine,assets:assetUrls,fonts,fontFaces:fontData,interactive:mode==="application",onEvent:enqueue,onError:finish,linkRequested:ui.linkRequested,onTitle:(title)=>{document.title=title;},onVisualInput:timing===undefined?undefined:kind=>timing.visual(kind,performance.now()),
            copyText:async(text)=>{if(!broker.operation(descriptor.view.capabilities.clipboard,'',!consenting&&!navigation&&ui.clipboardAllowed())||settled)return descriptor.view.statuses.not_allowed;try{await navigator.clipboard.writeText(text);return descriptor.view.statuses.ok;}catch{return descriptor.view.statuses.refused_by_platform;}},
            openUrl:(url)=>navigate(url,ui)
          });
        }else{if(mode==="application")throw new Error("mount requires an attached container");headless=viewModel.createViewModel({viewEngine,linkRequested:ui.linkRequested,assetRequested:(name)=>assetUrls.has(name)});if(headless.engine){for(const face of fontData)headless.engine.face(face.family,face.bytes,face.generics);headless.engine.defaultFamily(fonts.defaultName);}}
      }
      blobUrl = URL.createObjectURL(new Blob([script], { type: "text/javascript" }));
      activeWorkerUrls.add(blobUrl);
      worker = new Worker(trustedWorkerUrl(blobUrl), { type: "module", name: "Varta" });
      worker.onmessage = ({ data }) => {
        if (settled) return;
        if(data?.type==='host_paused'){
          if(!navigation||navigation.paused||data.id!==navigation.id){finish(new Error('invalid host pause acknowledgement'));return;}
          navigation.paused=true;navigation.resolve();return;
        }
        if(data?.type==='host_events_queued'){
          const items=deliveryBatches.get(data.id),E=descriptor.permissions.events;
          if(!items||!Array.isArray(data.results)||data.results.length!==items.length){finish(new Error('invalid host event acknowledgement'));return;}
          for(let i=0;i<items.length;i++){
            const result=data.results[i],item=items[i];
            if(result&&['queued','coalesced','dropped'].includes(result.delivery)&&result.error===undefined)item.resolve(result.delivery);
            else if(result&&[E.recordError,E.noListener.replace('{name}',item.event.source)].includes(result.error)&&result.delivery===undefined)item.reject(new Error(result.error));
            else{finish(new Error('invalid host event result'));return;}
          }
          deliveryBatches.delete(data.id);deliveryPending-=items.length;return;
        }
        if(data?.type==='permission_revoked'){
          const control=controls.get(data.id);
          if(!control||data.capability!==control.capability||data.revoked!==true){finish(new Error('invalid permission revocation acknowledgement'));return;}
          controls.delete(data.id);control.resolve();return;
        }
        if(data?.type==='permission_audit'){
          try{const {resource}=data.record??{};if(typeof resource!=='string'||new TextEncoder().encode(resource).length>descriptor.permissions.descriptor.maxBytes)throw new Error('invalid permission audit message');broker.recordOperation(data.record);}catch(error){finish(error);}return;
        }
        if(data?.type==="view"){
          try{
            if(!descriptor.hasView||data.id!==lastBatch+1||!(data.bytes instanceof Uint8Array))throw new Error("invalid view commit message");
            const applyStarted=onFrameTiming===undefined?0:performance.now();
            if(renderer)renderer.apply(data.bytes,{gesture:Boolean(data.gesture)});
            else{const {effects}=headless.commit(data.bytes);for(const effect of effects)if(effect.name==="copy_text")headless.complete(effect.request);}
            if(onFrameTiming!==undefined)timing.applied(performance.now()-applyStarted);
            lastBatch=data.id;worker.postMessage({type:"view_ack",id:data.id});
          }catch(error){finish(error);}return;
        }
        if(data?.type==="turn_begin"&&mode==="application"){arm();return;}
        // Idle collection between turns: a hard wall-clock bound per idle
        // period (the Worker is terminated), with the native diagnostic.
        if((data?.type==="maintenance_begin"||data?.type==="maintenance_end")&&mode==="application"&&descriptor.turnBudgets&&Number.isSafeInteger(data.turn)&&data.turn>0){
          const C=descriptor.effects.collector;
          if(data.type==="maintenance_begin")arm(budgets[C.deadlineBudget],C.deadlineExhausted.replace(C.turnPlaceholder,String(data.turn)).replace(C.limitPlaceholder,String(budgets[C.deadlineBudget])));
          else clearTimeout(timer);
          return;
        }
        if(data?.type==="turn_end"){
          if(consenting){finish(new Error('unexpected browser checkpoint during consent'));return;}
          if(!Number.isSafeInteger(data.id)||data.id!==lastCheckpoint+1){finish(new Error('invalid browser turn checkpoint'));return;}lastCheckpoint=data.id;
          if(mode==="application")clearTimeout(timer);
          if(data.event!==undefined){if(data.event!==inFlight){finish(new Error("invalid view event acknowledgement"));return;}inFlight=undefined;timing?.end(data.event);}
          (async()=>{try{
          const catalog=data.sources??[],nextSources=new Map(),ids=new Set(),E=descriptor.permissions.events;
          if(!Array.isArray(catalog)||catalog.length>E.sources)throw new Error('invalid host source catalog');
          for(const item of catalog){
            if(!item||typeof item.name!=='string'||item.name.length>E.nameBytes||!(/^[A-Za-z][A-Za-z0-9._-]*$/).test(item.name)||!Number.isSafeInteger(item.id)||item.id<1||item.id>E.maxId||nextSources.has(item.name)||ids.has(item.id)||item.id<=maxSourceId&&sourceCatalog.get(item.name)!==item.id)throw new Error('invalid host source catalog');
            nextSources.set(item.name,item.id);ids.add(item.id);
          }
          sourceCatalog=nextSources;for(const id of ids)maxSourceId=Math.max(maxSourceId,id);
          const prompts=data.prompts??[];
          if(!Array.isArray(prompts)||prompts.length>descriptor.permissions.limits.prompts||prompts.length&&!prompting)throw new Error('invalid permission prompts');
          const answers=[];consenting=prompts.length>0;
          for(const prompt of prompts){
            if(!prompt||!Number.isSafeInteger(prompt.source)||prompt.source<1||prompt.source>descriptor.permissions.events.maxId||typeof prompt.capability!=='string'||!broker.coveredGrant(prompt.capability))throw new Error('invalid permission prompt');
            const answer=broker.needsPrompt(prompt.capability)?await ask(broker.promptInfo(prompt.capability)):'dismiss';if(settled)return;
            const result=broker.resolve(prompt.capability,answer);
            if(result.changed){observe();if(result.decision!==undefined)await save(prompt.capability,result.decision);}
            answers.push({...prompt,answer,arrived:performance.timeOrigin+performance.now()});
          }
          consenting=false;
          if(mode==="application"){
            onTurn(data.usage);if(!ready){ready=true;onReady(data.usage);}pump();
          }
          if(!settled)worker.postMessage({type:'turn_ack',id:data.id,answers});
          }catch(error){finish(error);}})();return;
        }
        if (data?.type === "entered") { guestStarted = true; return; }
        if (data?.type === "log" && typeof data.line === "string") {
          streamedBytes += new TextEncoder().encode(data.line).byteLength + 1;
          if (streamedBytes > budgets.outputBytes) { finish(new Error("invalid worker output")); return; }
          logs.push(data.line);
          // V38 standard error: forward the text to the page console method
          // of its level; the line itself stays in the ordered output.
          const stderr=registry.console.stderr.find((entry)=>data.line.startsWith(entry.prefix));
          if(stderr!==undefined)console[stderr.method](data.line.slice(stderr.prefix.length));
          try{onLog(data.line);}catch(error){finish(error);} return;
        }
        if (data?.type !== "result" || !["ok", "failed"].includes(data.status)) { finish(new Error("invalid browser worker message")); return; }
        finish(null, data);
      };
      worker.onerror = () => finish(new Error("browser worker failed"));
      worker.onmessageerror = () => finish(new Error("invalid browser worker message"));
      worker.postMessage({ descriptor: { ...descriptor, budgets }, mode, permissions, decisions:storedDecisions,prompting, platform, environmentInputs:environmentSnapshot.entries(), networkReplay:replayLog, original, image, manifest, kvCodec, viewEngine:viewEngineBytes }, [original.buffer, image.buffer, manifest.buffer, ...(kvCodec?[kvCodec.buffer]:[]), ...(viewEngineBytes?[viewEngineBytes.buffer]:[])]);
      onControl({deliver(event){
        const E=descriptor.permissions.events;
        if(settled)return Promise.reject(new Error('browser application ended'));
        const sourceId=sourceCatalog.get(event.source);if(sourceId===undefined)return Promise.reject(new Error(E.noListener.replace('{name}',event.source)));
        if(deliveryPending>=E.tasks){if(event.coalesce)return Promise.resolve('dropped');const error=new Error(E.queueError);finish(error);return Promise.reject(error);}
        deliveryPending++;
        const result=new Promise((resolve,reject)=>deliveryStaging.push({event,sourceId,arrived:performance.timeOrigin+performance.now(),resolve,reject}));
        if(!deliveryScheduled){deliveryScheduled=true;queueMicrotask(flushDeliveries);}return result;
      },revoke(capability){
        if(settled)return Promise.reject(new Error('browser application ended'));
        if(revocations.has(capability))return revocations.get(capability);
        if(!broker.revoke(capability))return Promise.resolve(false);
        try{
          if(onAudit)onAudit(broker.snapshot().audit.at(-1));
          if(settled)throw new Error('browser application ended');
          const id=++controlId;if(!Number.isSafeInteger(id))throw new Error('permission control sequence exhausted');
          const acknowledgement=new Promise((resolve,reject)=>controls.set(id,{capability,resolve,reject}));
          acknowledgement.catch(()=>{});
          worker.postMessage({type:'permission_revoke',id,capability});
          const saved=persist(capability,'deny');
          const result=Promise.all([acknowledgement,saved]).then(()=>true).catch(error=>{finish(error);throw error;}).finally(()=>revocations.delete(capability));
          revocations.set(capability,result);return result;
        }catch(error){finish(error);return Promise.reject(error);}
      }});
    })().catch((error) => finish(error));
  });
}

export function run(options={}) {return execute({...options,mode:"run"});}

/** Explicit origin-local persistence; no stored host authority enters a build. */
export function createDecisionStore(options) {return permissionStore.createDecisionStore(options);}

/** An owned modal outside the guest view; use as mount's onPrompt callback. */
export function createPermissionPrompt(options) {return permissionPrompt.createPermissionPrompt(options);}

/** A long-lived view session. ready resolves after main's checked checkpoint. */
export function mount(options={}) {
  const controller=new AbortController();let resolveReady,rejectReady,lastUsage,closed=false,control;
  const ready=new Promise((resolve,reject)=>{resolveReady=resolve;rejectReady=reject;});ready.catch(()=>{});
  const aborted=()=>controller.abort();options.signal?.addEventListener("abort",aborted,{once:true});
  if(options.signal?.aborted)controller.abort();
  const finished=execute({...options,signal:controller.signal,mode:"application",onControl:(value)=>{control=value;},onReady:(usage)=>{lastUsage=usage;resolveReady({usage});options.onReady?.({usage});},onTurn:(usage)=>{lastUsage=usage;options.onTurn?.(usage);}})
    // An instance that ends before its first checkpoint (an exit in main)
    // was never ready; a later end leaves the resolved readiness unchanged.
    .then((result)=>{rejectReady(new Error(result.status!=="ok"?result.error:`browser application exited with code ${result.exitCode}`));return result;},(error)=>{rejectReady(error);if(closed)return{status:"closed",logs:error.logs??[],usage:lastUsage,permissions:error.permissions};throw error;})
    .finally(()=>options.signal?.removeEventListener("abort",aborted));
  finished.catch(()=>{});
  return {ready,finished,deliver(event){let snapshot;try{snapshot=eventSources.snapshotHostEvent(event);}catch(error){return Promise.reject(error);}return ready.then(()=>control.deliver(snapshot));},async revoke(capability){await ready;return control.revoke(capability);},close(){closed=true;controller.abort();}};
}

function bootstrap(runtime={}){
  if(new URL(location.href).searchParams.get("ui-timing")==="1")runtime={...runtime,onFrameTiming:sample=>console.debug("varta-ui-timing:",JSON.stringify(sample))};
  const output=document.querySelector("#varta-output");
  if(output){
    const show=(usage)=>{
      output.dataset.usage=JSON.stringify(usage);
      if(runtime.reportBudgets){let budget=document.querySelector('#varta-budgets');if(!budget){budget=document.createElement('pre');budget.id='varta-budgets';output.after(budget);}budget.textContent=Object.entries(usage).map(([name,value])=>`${name}: ${value}`).join('\n');}
    };
    const done=(result)=>{output.textContent=result.logs.join("\n")+(result.status==="failed"?`\n${result.error}`:"");output.dataset.status=result.status;if(result.status==="ok")output.dataset.exitCode=String(result.exitCode);show({fuelRemaining:result.fuelRemaining,hostCalls:result.hostCalls,microtasks:result.microtasks,outputBytes:result.outputBytes,fuelUsed:result.fuelUsed,turns:result.turns});};
    const failed=(error)=>{output.textContent=[...(error.logs??[]),error.message].join("\n");output.dataset.status="failed";output.dataset.guestStarted=String(error.guestStarted);};
    if(descriptor.hasView||runtime.application){
      try{
        const consent=descriptor.turnBudgets&&descriptor.hasPermissions?{onPrompt:createPermissionPrompt(),...(runtime.decisions===undefined?{decisionStore:createDecisionStore()}:{})}:{};
        const logs=[];const app=mount({...consent,...runtime,container:descriptor.hasView?document.querySelector("#varta-view"):undefined,onLog:(line)=>{logs.push(line);output.textContent=logs.join("\n");},onReady:({usage})=>{output.textContent=logs.join("\n");output.dataset.status="ready";show(usage);},onTurn:show});
        app.finished.then(done,failed);
      }catch(error){error.guestStarted=false;failed(error);}
    }else run(runtime).then(done,failed);
  }
}
if(typeof document!=="undefined"&&[...document.querySelectorAll("script[data-varta-bootstrap]")].some((script)=>script.src===import.meta.url))bootstrap();
