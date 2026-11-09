
// Trusted runtime transport only. Admission, authority and lease policy live
// in Rust. Optional app bindings contain only scoped ceilings and a public URL.
function previewBootstrap(runtime) {
  const root = new URL("./", import.meta.url), output = document.querySelector("#varta-output");
  let app, lease, timer, watchdog, closed = false;
  const needsFrame=runtime.readyAfterFrame===true;
  let executionReady=false,frameReady=!needsFrame,readyReported=false;
  const close = (message) => {
    if (closed) return;
    closed = true; clearTimeout(timer); clearTimeout(watchdog); app?.close();
    if (output) { output.dataset.previewState = "stopped"; if (message) output.textContent = message; }
  };
  const call = async (path, body) => {
    const headers = { "content-type": "application/json" };
    if (lease) headers["x-varta-preview-client"] = lease;
    const response = await fetch(new URL(path,root), {
      method:"POST", credentials:"same-origin", redirect:"error", cache:"no-store",
      referrerPolicy:"no-referrer", headers, body:JSON.stringify(body ?? {}),
      signal:AbortSignal.timeout(runtime.pollMs)
    });
    if (!response.ok) throw new Error("Preview lease ended");
    return response.json();
  };
  const renew = async (claim = false) => {
    const start=performance.now(), reply=await call(claim ? "claim" : "lease");
    if (closed) return;
    if (claim) { lease=reply.leaseToken; if (reply.runtime) runtime=reply.runtime; }
    clearTimeout(watchdog);
    watchdog=setTimeout(()=>close("Preview lease ended"),Math.max(0,reply.leaseMs-(performance.now()-start)));
    timer=setTimeout(()=>renew().catch(()=>close("Preview lease ended")),runtime.pollMs);
  };
  const report = (guestState) => call("report",{guestState}).catch(()=>{});
  const ready=()=>{if(closed||readyReported||!executionReady||!frameReady)return;readyReported=true;if(output)output.dataset.previewState="ready";report("ready");};
  if(needsFrame)applicationFrame=()=>{frameReady=true;ready();};
  renew(true).then(()=>{
    if(closed)return;
    const logs=[];
    app=mount({
      permissionCeiling:runtime.permissionCeiling,budgetCeiling:runtime.budgetCeiling,
      sessionBinding:runtime.sessionBinding,environmentInputs:runtime.environmentInputs,
      container:descriptor.hasView?document.querySelector("#varta-view"):undefined,
      onLog(line){logs.push(line);if(output)output.textContent=logs.join("\n");}
    });
    app.ready.then(()=>{executionReady=true;ready();},()=>{});
    app.finished.then((result)=>{
      if(closed)return;
      report(result.status==="failed"?"failed":"closed");
      close();
      if(output){output.dataset.previewState=result.status;output.textContent=logs.join("\n")+(result.error?"\n"+result.error:"");}
    },(error)=>{if(!closed){report("failed");close(error.message);}});
  }).catch(()=>close("Preview activation unavailable"));
  addEventListener("pagehide",()=>close(),{once:true});
}
