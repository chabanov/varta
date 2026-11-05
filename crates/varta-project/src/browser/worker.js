// One message, one instance. This Worker has no ambient effect adapters.
let started = false, incoming, earlyPlatform, startupFailure;
// An authorized `process.exit`: unwinds guest code and ends the instance.
class GuestExit { constructor(code) { this.code = code; } }
self.onmessage = async ({ data }) => {
  if (started) {
    if(incoming)incoming(data);
    else if(data?.type==='permission_platform'&&data.id===registry.view.capabilities.clipboard&&typeof data.state==='string'&&Object.hasOwn(registry.permissions.states,data.state))earlyPlatform={id:data.id,state:data.state};
    else startupFailure=new Error('invalid browser startup message');
    return;
  }
  started = true;
  const { descriptor, image, permissions, decisions, platform={} } = data;
  const application = data.mode === "application";
  const prompting=application&&descriptor.turnBudgets&&data.prompting===true,prompts=new Map();
  const logs = [];
  let tank, scheduler, network, view, events, calls = 0, totalCalls = 0, outputBytes = 0, instance, closedFuel = 0, turns = 1, kvStorage;
  const maintenance = [];
  let maintenanceFuel = 0, maintenancePeriods = 0;
  try {
    const profile = registry.profiles.find((profile) => profile.name === descriptor.profile);
    if (!profile || descriptor.schemaVersion !== bundleSchemaVersion || descriptor.fairFuel !== profile.fairFuel) throw new Error("invalid prepared descriptor");
    tank = new FuelTank(descriptor.budgets.fuelPerTurn);
    const module = await WebAssembly.compile(image);
    if(startupFailure)throw startupFailure;
    if(earlyPlatform)platform[earlyPlatform.id]=earlyPlatform.state;
    const imports = WebAssembly.Module.imports(module);
    const expected = [...descriptor.imports.map((item) => ({ ...item, kind: "function" })),
      { module: registry.meter.module, name: registry.meter.counter, kind: "global" }];
    if (imports.length !== expected.length || imports.some((item, i) => item.module !== expected[i].module || item.name !== expected[i].name || item.kind !== expected[i].kind)) throw new Error("prepared import mismatch");
    const sections = WebAssembly.Module.customSections(module, registry.meter.section);
    if (sections.length !== 1 || sections[0].byteLength !== 4 || new DataView(sections[0]).getUint32(0, true) !== registry.meter.version) throw new Error("invalid private meter version");
    const encoder = new TextEncoder();
    const tick = () => {
      if (profile.fairFuel) tank.charge("call");
      totalCalls++;
      if (++calls > descriptor.budgets.hostCallsPerTurn) throw new Error("host-call budget exhausted");
    };
    const emit = (text) => {
      const line = `varta log: ${text}`;
      outputBytes += encoder.encode(line).byteLength + 1;
      if (outputBytes > descriptor.budgets.outputBytes) throw new Error("output budget exhausted");
      logs.push(line);
      self.postMessage({ type: "log", line });
    };
    const renderReason = reasonReader(() => instance.exports[descriptor.memoryExport], module, profile, descriptor);
    scheduler = createScheduler(profile, renderReason);
    const checkException = (initial = false) => {
      const flag=instance.exports[descriptor.pendingExport]?.value;
      if (flag === undefined) return;
      const value=instance.exports[registry.values.exceptionValue]?.value;
      const root=new DataView(instance.exports[descriptor.memoryExport].buffer).getBigUint64(registry.values.exceptionRoot,true);
      if (flag===0 && value===0n && root===0n) return;
      if (initial) throw new Error("nonzero initial exception state");
      if (flag!==1 || value!==root) throw new Error("noncanonical exception channel state");
      throw new Error(`uncaught guest exception: ${renderReason(value, false)}`);
    };
    const implementations = {
      ...scheduler.implementations,
      log_i32(value) { emit(String(value)); },
      log_f64_v2(value) { emit(String(value)); },
      log_undefined(value) { if (value !== 0) throw new Error("noncanonical undefined"); emit("undefined"); },
      log_string16(ptr, len) {
        if (profile.fairFuel) tank.charge("byte", Math.max(len, 0));
        const memory = instance.exports[descriptor.memoryExport];
        if (!(memory instanceof WebAssembly.Memory) || ptr < 0 || len < 0 || len % 2 || len > descriptor.maxStringBytes || ptr + len > memory.buffer.byteLength) throw new Error("invalid guest string");
        // Decode diagnostics in bounded chunks, preserving BOMs and replacing
        // lone units like native. Count the entire attempted UTF-8 line, while
        // retaining text only within the output budget. Never allocate a whole
        // guest-sized string merely to reject an oversized output.
        const decoder = new TextDecoder("utf-16le", { ignoreBOM: true });
        const chunks = [];
        let size = encoder.encode("varta log: ").byteLength + 1;
        const add = (text) => {
          size += encoder.encode(text).byteLength;
          if (outputBytes + size <= descriptor.budgets.outputBytes) chunks.push(text);
        };
        for (let offset = 0; offset < len; offset += descriptor.decodeChunkBytes) {
          add(decoder.decode(new Uint8Array(memory.buffer, ptr + offset, Math.min(descriptor.decodeChunkBytes, len - offset)), { stream: true }));
        }
        add(decoder.decode());
        outputBytes += size;
        if (outputBytes > descriptor.budgets.outputBytes) throw new Error("output budget exhausted");
        const line = "varta log: " + chunks.join("");
        logs.push(line);
        self.postMessage({ type: "log", line });
      },
    };
    const manifest = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(data.manifest));
    const environmentValues=environment.snapshotEnvironment(data.environmentInputs);
    const environmentPolicy=(id)=>environment.createEnvironmentPolicy(
      manifest.capabilities.find(grant=>grant.id===id)?.patterns??[],
      permissions===undefined?undefined:(permissions[id]?.patterns??[]));
    const environmentScopes={'env.get':environmentPolicy('env.get'),'env.has':environmentPolicy('env.has')};
    const broker=permissionBroker.createPermissionBroker({manifest,ceiling:permissions,artifact:descriptor.original.identity,decisions,platform,v1:descriptor.turnBudgets,prompt:prompting,audit:false,
      covers:(id,resource)=>id===registry.network.capability?policy.covers(resource):id===registry.view.capabilities.openUrl?uiPolicy.covers(resource):Object.hasOwn(environmentScopes,id)?environmentScopes[id].allows(resource):false,
      onRecord:(record)=>self.postMessage({type:'permission_audit',record})});
    const allowed = () => {
      const id = registry.scheduler.timer.capability;
      const request = manifest.capabilities.find((grant) => grant.id === id);
      return broker.operation(id,'',request?.enabled === true
        && (permissions === undefined || Object.hasOwn(permissions, id) && permissions[id] === true));
    };
    const timers = createTimers(profile, scheduler, allowed, emit, renderReason);
    Object.assign(implementations, timers.implementations);
    const rawPolicy = networkPolicy(manifest, permissions, false);
    const policy={...rawPolicy,allows:(url,method)=>broker.operation(registry.network.capability,`${method} ${url}`,rawPolicy.allows(url,method))};
    const readDescriptor = (ptr, length) => {
      const memory = instance.exports[descriptor.memoryExport];
      if (!Number.isInteger(ptr) || ptr < 0 || !Number.isInteger(length) || length < 0 || length % 2 || length > registry.network.maxRequestBytes || ptr + length > memory.buffer.byteLength) throw new Error(registry.network.errors.descriptor);
      return new TextDecoder("utf-16le", {ignoreBOM:true}).decode(new Uint8Array(memory.buffer, ptr, length));
    };
    const writeResult = (text, onGrowFailure) => {
      const R = registry.network.result, length = text.length * 2;
      if (encoder.encode(text).length > R.maxBytes || length > R.maxBytes) throw new Error("host result exceeds limit");
      if (profile.fairFuel) tank.charge("byte", length);
      const memory = instance.exports[descriptor.memoryExport], wanted = R.base + length;
      if (wanted > memory.buffer.byteLength) {
        const pages=Math.ceil((wanted - memory.buffer.byteLength) / R.pageBytes);
        try { memory.grow(pages); } catch(error) {
          if (onGrowFailure) return onGrowFailure(pages);
          throw error;
        }
      }
      const view = new DataView(memory.buffer);
      for (let i = 0; i < text.length; i++) view.setUint16(R.base + i * 2, text.charCodeAt(i), true);
      instance.exports[R.length].value = length; return R.base;
    };
    // Instantiated lazily: result globals and memory only exist after Wasm
    // instantiation.
    let environmentImports;
    const environmentImport=(id,ptr,len)=>{
      if(!environmentImports){
        const E=registry.environment, exports=instance.exports;
        const access=Object.fromEntries(Object.entries(environmentScopes).map(([name,scope])=>[name,environment.createEnvironmentAccess(environmentValues,scope,broker,name)]));
        const snapshotId=registry.effects.environment.capability;
        const snapshot=environment.createEnvironmentSnapshot(environmentValues,environmentScopes[snapshotId],()=>broker.granted(snapshotId));
        environmentImports=environmentAbi.createEnvironmentAbi({memory:()=>exports[descriptor.memoryExport],resultLength:exports[E.resultLength],resultStatus:exports[E.resultStatus],access,snapshot,chargeBytes:n=>{if(profile.fairFuel)tank.charge('byte',n);},writeResult,emit});
      }
      return environmentImports[id](ptr,len);
    };
    implementations.env_get16=(ptr,len)=>environmentImport('env_get16',ptr,len);
    implementations.env_has16=(ptr,len)=>environmentImport('env_has16',ptr,len);
    const snapshotOperation=registry.effects.environment.operation;
    implementations[snapshotOperation]=(ptr,len)=>environmentImport(snapshotOperation,ptr,len);
    // V38 KV storage (kvStore): the shared value layer compiled to wasm32
    // and, when a grant covers kv.read, the artifact's OPFS container,
    // acquired here, before instantiation. A Worker without sync access
    // handles refuses such a program by name instead of denying silently.
    let kvHost, kvTurnStarted = performance.now();
    const usesKv = descriptor.imports.some((item) => item.name === registry.kv.operations.open);
    if (usesKv && !(data.kvCodec instanceof Uint8Array)) throw new Error("missing KV codec module");
    const kvCodec = usesKv ? await WebAssembly.compile(data.kvCodec) : undefined;
    if (usesKv && broker.coveredGrant(registry.kv.capabilities.read)) {
      if (!kvStore.syncHandlesSupported(self)) throw new Error(registry.kv.storageUnavailable + registry.kv.capabilities.read);
      kvStorage = await kvStore.openKvStorage(navigator.storage, descriptor.original.identity);
    }
    // Exact codec fuel, charged at the unit (`byte`) rate.
    const chargeKvFuel = (amount) => { if (profile.fairFuel && amount > 0) tank.charge('byte', Number(amount)); };
    const kvDeadline = () => kvTurnStarted + descriptor.budgets.deadlinePerTurnMs;
    // Created by the first KV call, like the environment imports.
    const kvHostFor = () => kvHost ??= kvStore.createKvHost({memory: () => instance.exports[descriptor.memoryExport], broker,
      codec: kvStore.createCodec({codec: kvCodec, guest: module, memory: () => instance.exports[descriptor.memoryExport], exports: () => instance.exports,
        deadlinePassed: () => performance.now() >= kvDeadline(), chargeFuel: chargeKvFuel, version: profile.exceptionVersion}),
      policy: kvStore.createKvPolicy(manifest, permissions), storage: kvStorage, emit, writeResult,
      chargeBytes: (n) => { if (profile.fairFuel) tank.charge('byte', n); }, chargeFuel: chargeKvFuel, turnDeadline: kvDeadline});
    for(const name of Object.values(registry.kv.operations))implementations[name]=(...args)=>kvHostFor()[name](...args);
    // Pilot effects, each equal to its native handler after the shared call
    // charge. The clock is an audited `time.now` operation (exact package
    // request and the ceiling's own `true`, decided before the clock is read);
    // a denial is recoverable: logged, and NaN, which no clock reading gives.
    const {clock,random,exit}=registry.effects;
    implementations[clock.operation]=()=>{
      if(!broker.operation(clock.capability,'',broker.coveredGrant(clock.capability))){emit(clock.denialText);return NaN;}
      return Date.now();
    };
    // Ungated entropy: one fresh 64-bit word per sample, never a seeded
    // generator. Its top sampleBits bits scaled by 2^-sampleBits, exactly as
    // native: every sample is in [0, 1) and zero has a positive sign.
    const entropyUnitBits=Uint32Array.BYTES_PER_ELEMENT*8,entropy=new Uint32Array(random.wordBits/entropyUnitBits);
    const discardedBits=random.wordBits-random.sampleBits;
    implementations[random.operation]=()=>{
      crypto.getRandomValues(entropy);
      const [high,low]=entropy;
      return (high*2**(entropyUnitBits-discardedBits)+(low>>>discardedBits))*2**-random.sampleBits;
    };
    // `process.exit`: reset the shared result, then the unaudited boolean
    // check native makes (request, ceiling and broker state). A denied or
    // non-integer code is a recoverable failure; an authorized exit ends the
    // instance with its code and no later turn runs.
    implementations[exit.operation]=(value)=>{
      const E=registry.environment,result=(status)=>{
        if(instance.exports[E.resultLength])instance.exports[E.resultLength].value=0;
        if(instance.exports[E.resultStatus])instance.exports[E.resultStatus].value=status;
      };
      result(E.statuses.ok);
      if(!(broker.coveredGrant(exit.capability)&&broker.granted(exit.capability))){result(E.statuses.denied);emit(exit.denialText);return;}
      if(!Number.isInteger(value)||value<exit.minimum||value>exit.maximum){result(E.statuses.invalid);emit(registry.effects.failures.invalid);return;}
      throw new GuestExit(value|0);
    };
    const readEventText=(ptr,length)=>{
      const memory=instance.exports[descriptor.memoryExport],P=registry.permissions;
      if(ptr<0||length<0||length>P.descriptor.maxBytes)throw new Error(P.events.stateError);
      if(profile.fairFuel)tank.charge('byte',length);
      if(length%2||ptr+length>memory.buffer.byteLength)throw new Error(P.events.stateError);
      let text;try{text=new TextDecoder('utf-16le',{ignoreBOM:true,fatal:true}).decode(new Uint8Array(memory.buffer,ptr,length));}catch{throw new Error(P.events.stateError);}
      if(text.includes('\0')||encoder.encode(text).length>P.descriptor.maxBytes)throw new Error(P.events.stateError);
      return text;
    };
    events=eventSources.createEventSources({scheduler,readText:readEventText,writeResult});
    Object.assign(implementations,events.implementations,{
      permission_query:(ptr,length)=>broker.query(readEventText(ptr,length)),
      permission_request:(ptr,length)=>{
        const text=readEventText(ptr,length),code=broker.query(text);if(code===registry.permissions.descriptor.invalid)return code;
        const {name}=permissionBroker.parsePermissionDescriptor(text);
        if(code===registry.permissions.states.prompt&&broker.needsPrompt(name)&&gesture&&prompts.size<registry.permissions.limits.prompts){
          const source=events.answer(undefined);prompts.set(source,name);return source;
        }
        return events.answer(Object.keys(registry.permissions.states).find(name=>registry.permissions.states[name]===code));
      },
    });
    network = createNetwork(scheduler, policy, emit, readDescriptor, writeResult, (plan) => browserTransport({...plan,timeoutMs:descriptor.budgets.deadlinePerTurnMs}));
    Object.assign(implementations, network.implementations);
    const uiPolicy = viewPolicy.createViewPolicy(manifest, permissions,broker.granted);
    const assetNames=new Set(descriptor.resources.filter((resource)=>!resource.font).map((resource)=>resource.key));
    view = viewWorker.createWorkerView({ send: (message, transfer) => self.postMessage(message, transfer), memory: () => instance.exports[descriptor.memoryExport], application, linkRequested:uiPolicy.linkRequested,assetRequested:(name)=>assetNames.has(name) });
    let checkpoint,checkpointId=0,controlId=0,deliveryId=0,pauseId=0,pause,protocolFailure;
    const protocolFault=(error)=>{protocolFailure=error;checkpoint?.resolve();pause?.resolve();view.pulse();};
    incoming = (message) => {
      try{
        if(message?.type==='turn_ack'){
          if(!checkpoint||!Number.isSafeInteger(message.id)||message.id!==checkpoint.id)throw new Error('invalid browser turn acknowledgement');
          const answers=message.answers??[];
          if(!Array.isArray(answers)||answers.length!==prompts.size)throw new Error('invalid permission answers');
          for(const answer of answers){
            if(!answer||prompts.get(answer.source)!==answer.capability||typeof answer.capability!=='string'||!Number.isFinite(answer.arrived)||answer.arrived<0)throw new Error('invalid permission answer');
            const {state}=broker.resolve(answer.capability,answer.answer);events.complete(answer.source,state,answer.arrived);prompts.delete(answer.source);
          }
          const resolve=checkpoint.resolve;checkpoint=undefined;resolve();
        }else if(message?.type==='host_pause'){
          if(!application||pause||!Number.isSafeInteger(message.id)||message.id!==pauseId+1)throw new Error('invalid host pause');
          pauseId=message.id;let resolve;const promise=new Promise(done=>resolve=done);pause={id:message.id,promise,resolve,acknowledged:false};view.pulse();
        }else if(message?.type==='host_resume'){
          if(!pause||!pause.acknowledged||message.id!==pause.id)throw new Error('invalid host resume');
          const held=pause;pause=undefined;held.resolve();
        }else if(message?.type==='host_events'){
          if(!application||!Number.isSafeInteger(message.id)||message.id!==deliveryId+1||!Array.isArray(message.events)||!message.events.length||message.events.length>registry.permissions.events.tasks)throw new Error('invalid host event batch');
          deliveryId=message.id;const results=[];
          for(const item of message.events){
            if(!item||!Number.isSafeInteger(item.sourceId)||item.sourceId<1||item.sourceId>registry.permissions.events.maxId||typeof item.event?.source!=='string')throw new Error('invalid host event delivery');
            try{results.push({delivery:events.push(item.event,item.sourceId,item.arrived)});}
            catch(error){if(error.message===registry.permissions.events.queueError)throw error;results.push({error:error.message});}
          }
          self.postMessage({type:'host_events_queued',id:message.id,results});
        }else if(message?.type==='permission_revoke'){
          if(!application||!Number.isSafeInteger(message.id)||message.id!==controlId+1||typeof message.capability!=='string'||!registry.permissions.capabilities.some(({id})=>id===message.capability))throw new Error('invalid permission revocation');
          controlId=message.id;const revoked=broker.revoke(message.capability);
          self.postMessage({type:'permission_revoked',id:message.id,capability:message.capability,revoked});
        }else if(message?.type==='permission_platform'&&message.id===registry.view.capabilities.clipboard&&typeof message.state==='string'&&Object.hasOwn(registry.permissions.states,message.state))broker.setPlatform(message.id,message.state);
        else{view.receive(message);if(view.error)protocolFault(view.error);}
      }catch(error){protocolFault(error);}
    };
    network.onReady(view.pulse);
    events.onReady(view.pulse);
    implementations[registry.view.wire.commitImport] = (ptr,length) => view.commit(ptr,length,gesture);
    let gesture = false;
    const varta = Object.create(null);
    // Capabilities whose checks this Worker implements; any other gated
    // import is refused before instantiation.
    const checkedCapabilities = new Set([registry.scheduler.timer.capability, registry.network.capability, 'env.get', 'env.has', clock.capability, exit.capability, registry.kv.capabilities.read, registry.kv.capabilities.write]);
    for (const item of descriptor.imports) {
      const operation = registry.operations.find((operation) => operation.name === item.name);
      if (!operation || !operation.admittedUnder.includes(profile.name) || operation.capability !== null && !checkedCapabilities.has(operation.capability) || !Object.hasOwn(implementations, item.name)) throw new Error(`unsupported browser effect: ${item.name}`);
      varta[item.name] = (...args) => {
        if (operation.scheduler || operation.timerBookkeeping || operation.networkBookkeeping || operation.instanceBookkeeping || operation.kvBookkeeping) { if (profile.fairFuel) tank.charge("call"); }
        else tick();
        return implementations[item.name](...args);
      };
    }
    instance = await WebAssembly.instantiate(module, {
      [descriptor.hostModule]: varta, [registry.meter.module]: { [registry.meter.counter]: tank.global },
    });
    const start = instance.exports[descriptor.startExport];
    if (typeof start !== "function") throw new Error("missing start export");
    if (profile.exceptionVersion && instance.exports[profile.exceptionVersion.global]?.value !== profile.exceptionVersion.value) throw new Error("invalid exception channel version");
    if(protocolFailure)throw protocolFailure;
    checkException(true);
    const loopStarted = performance.now();
    self.postMessage({ type: "entered" });
    kvTurnStarted = performance.now();
    start();
    checkException();
    let turnOpen = true, activeEvent;
    const usage = () => ({fuelRemaining:tank.remaining,hostCalls:totalCalls,outputBytes,microtasks:scheduler.totalJobs,fuelUsed:closedFuel+descriptor.budgets.fuelPerTurn-tank.remaining,turns});
    let maintained = false;
    const renew = () => {
      if (descriptor.turnBudgets) {
        closedFuel += descriptor.budgets.fuelPerTurn - tank.remaining;
        tank.beginTurn(descriptor.budgets.fuelPerTurn); calls = 0; scheduler.renew(); turns++;
      }
      view.begin(); gesture = false; turnOpen = true; maintained = false;
      // As natively, only a long-lived instance renews its deadline per turn.
      if (application) kvTurnStarted = performance.now();
      if (application) self.postMessage({type:"turn_begin"});
    };
    // Idle collection (v38), as the native runtime's maintenance: once per
    // idle period of a long-lived instance, when no task is runnable, the
    // host-free idle export runs in steps under a fresh maintenance allowance,
    // never a turn's fuel, while a whole step's fuel remains. The closed
    // turn's remaining fuel is restored afterwards. The loader's watchdog
    // bounds the period's wall time; between steps the Worker checks it too.
    const C = registry.effects.collector, idle = instance.exports[C.idleExport];
    const maintain = () => {
      maintained = true;
      const fuelBudget = descriptor.budgets[C.fuelBudget], deadline = descriptor.budgets[C.deadlineBudget];
      if (!application || !descriptor.turnBudgets || !fuelBudget || typeof idle !== "function") return;
      const exhausted = (template, limit) => new Error(template.replace(C.turnPlaceholder, String(turns)).replace(C.limitPlaceholder, String(limit)));
      const turnLeft = tank.remaining, started = performance.now();
      self.postMessage({ type: "maintenance_begin", turn: turns });
      tank.beginTurn(fuelBudget);
      let steps = 0, completed = false, failure;
      try {
        while (tank.remaining >= C.stepFuel) {
          let status;
          try { status = idle(C.stepUnits); }
          catch (error) { steps++; throw tank.remaining === 0 ? exhausted(C.fuelExhausted, fuelBudget) : error; }
          if (performance.now() - started >= deadline) { steps++; throw exhausted(C.deadlineExhausted, deadline); }
          if (status === C.none) break;
          if (status === C.more) steps++;
          else if (status === C.done) { steps++; completed = true; break; }
          else throw new Error(C.invalidStatus);
        }
      } catch (error) { failure = error; }
      const fuel = fuelBudget - tank.remaining;
      tank.beginTurn(turnLeft);
      // Like native: the fuel always counts; a period that did work is listed.
      maintenanceFuel += fuel;
      if (steps > 0 && maintenancePeriods++ < C.reportedPeriods) maintenance.push({ afterTurn: turns, fuel, steps, completed, memoryBytes: instance.exports[descriptor.memoryExport].buffer.byteLength });
      if (failure) throw failure;
      self.postMessage({ type: "maintenance_end", turn: turns });
    };
    while (true) {
      if(protocolFailure)throw protocolFailure;
      scheduler.drain(instance, checkException);
      await view.flush();
      if (turnOpen) {
        const id=++checkpointId;if(!Number.isSafeInteger(id))throw new Error('browser turn sequence exhausted');
        let acknowledge;const promise=new Promise(resolve=>acknowledge=resolve);checkpoint={id,resolve:acknowledge};
        self.postMessage({type:"turn_end",id,usage:usage(),event:activeEvent,sources:events.catalog(),prompts:[...prompts].map(([source,capability])=>({source,capability}))});activeEvent=undefined;turnOpen=false;
        await promise;if(protocolFailure)throw protocolFailure;
      }
      if(pause){const held=pause;held.acknowledged=true;self.postMessage({type:'host_paused',id:held.id});await held.promise;if(protocolFailure)throw protocolFailure;}
      const busy = network.alive();
      // Drive transfers before choosing a task, even if an interval is overdue.
      // The parent checkpoint has already accepted the preceding audit records.
      if(busy||application)network.pump();
      const holding=application&&(view.attached()||events.listening());
      const due = timers.nextDue(busy || holding);
      const limit = application ? Infinity : descriptor.budgets.deadlinePerTurnMs;
      const elapsed = () => performance.now() - loopStarted;
      if (application) {
        if(!holding&&!busy&&due===undefined&&events.arrival()===undefined)break;
        const eventAt=view.arrival()??Infinity,networkAt=network.arrival()??Infinity,hostAt=events.arrival()??Infinity,timerAt=due===undefined?Infinity:performance.timeOrigin+loopStarted+due;
        if(due!==undefined&&due<=elapsed()&&timerAt<=eventAt&&timerAt<=networkAt&&timerAt<=hostAt){timers.fire();renew();continue;}
        if(eventAt!==Infinity&&eventAt<=networkAt&&eventAt<=hostAt){
          const task=view.take();renew();gesture=task.gesture;activeEvent=task.id;timers.observe(elapsed());
          const memory=instance.exports[descriptor.memoryExport],R=registry.network.result,wanted=R.base+task.bytes.length;
          if(wanted>memory.buffer.byteLength)memory.grow(Math.ceil((wanted-memory.buffer.byteLength)/R.pageBytes));
          new Uint8Array(memory.buffer,R.base,task.bytes.length).set(task.bytes);
          instance.exports[registry.view.wire.eventExport](task.bytes.length);checkException();continue;
        }
        if(networkAt!==Infinity&&networkAt<=hostAt&&network.deliver()){timers.observe(elapsed());renew();continue;}
        if(hostAt!==Infinity&&events.deliver()){const trusted=events.gesture;timers.observe(elapsed());renew();gesture=trusted;continue;}
        if(!maintained){maintain();continue;}
        await view.wait(due===undefined?Infinity:due-elapsed());continue;
      }
      if (due === undefined && !busy) break;
      if (busy) {
        if (due !== undefined && due < limit && due <= elapsed()) { timers.fire(); renew(); continue; }
        await network.wait(Math.max(0, Math.min(due ?? limit, limit) - elapsed()));
        if (network.deliver()) { timers.observe(elapsed()); renew(); continue; }
        if (due !== undefined && due < limit && due <= elapsed()) { timers.fire(); renew(); continue; }
        if (elapsed() < limit) continue;
      }
      if (due !== undefined && due >= limit) throw new Error(registry.scheduler.timer.deadlineTemplate.replace("{due}", String(Math.ceil(due))).replace("{seconds}", String(Math.ceil(limit / 1000))));
      if (busy) throw new Error("browser execution deadline exceeded");
      const wait = due - elapsed();
      if (wait > 0) await new Promise((resolve) => setTimeout(resolve, wait));
      timers.fire(); renew();
    }
    view.close();
    network.close();
    events.close();
    kvStorage?.container?.close();
    self.postMessage({ type: "result", status: "ok", logs, exitCode: 0, fuelRemaining: tank.remaining, hostCalls: totalCalls, outputBytes, microtasks: scheduler?.totalJobs ?? 0, fuelUsed: closedFuel + descriptor.budgets.fuelPerTurn - (tank?.remaining ?? descriptor.budgets.fuelPerTurn), turns, maintenance, maintenanceFuel, maintenancePeriods });
  } catch (error) {
    view?.close();
    network?.close();
    events?.close();
    kvStorage?.container?.close();
    // An authorized exit is not a failure: the instance ends with its code.
    const exited = error instanceof GuestExit;
    self.postMessage({ type: "result", status: exited ? "ok" : "failed", logs, ...(exited ? { exitCode: error.code } : { error: String(error?.message ?? error) }), fuelRemaining: tank?.remaining ?? 0, hostCalls: totalCalls, outputBytes, microtasks: scheduler?.totalJobs ?? 0, fuelUsed: closedFuel + descriptor.budgets.fuelPerTurn - (tank?.remaining ?? descriptor.budgets.fuelPerTurn), turns, maintenance, maintenanceFuel, maintenancePeriods });
  }
};
