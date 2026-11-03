// Host diagnostics only. A double rAF observes a paint opportunity, not scanout.
export function createUiTiming({now, request, cancel, emit, limit}) {
  let active, scheduled, closed=false, dropped=0;
  const pending=[],ready=[];
  const tick=()=>{
    scheduled=undefined;
    if(closed)return;
    const end=now(), batch=ready.splice(0), lost=dropped;if(batch.length)dropped=0;
    ready.push(...pending.splice(0));
    for(const sample of batch){if(closed)break;emit({...sample,boundary:'enqueue-to-frame-opportunity',totalMs:end-sample.queued,afterCheckpointMs:end-sample.checkpoint,dropped:lost});}
    if(!closed&&(ready.length||pending.length)&&scheduled===undefined)scheduled=request(tick);
  };
  const append=sample=>{
    if(pending.length+ready.length>=limit){dropped=Math.min(Number.MAX_SAFE_INTEGER,dropped+1);return;}
    pending.push(sample);
    if(scheduled===undefined)scheduled=request(tick);
  };
  return {
    visual(kind,queued){if(!closed)append({kind:'visual:'+kind,queued,checkpoint:now(),queueMs:0,workerRoundtripMs:0,applyMs:0});},
    begin(id,kind,queued){if(!closed)active={id,kind,queued,started:now(),applyMs:0};},
    applied(elapsed){if(active)active.applyMs+=elapsed;},
    end(id){
      if(closed||active?.id!==id)return;
      const sample={...active,checkpoint:now(),queueMs:active.started-active.queued};
      sample.workerRoundtripMs=sample.checkpoint-active.started;
      active=undefined;
      append(sample);
    },
    close(){closed=true;if(scheduled!==undefined)cancel(scheduled);scheduled=undefined;active=undefined;pending.length=0;ready.length=0;},
  };
}
