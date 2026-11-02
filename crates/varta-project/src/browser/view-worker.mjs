import { registry } from "../../../varta-common/browser/registry.mjs";
import { ViewProtocolError, decodeViewEvent } from "./view-codec.mjs";
import { createViewModel } from "./view-model.mjs";

/** One commit in flight; one parent event in flight; no reentrant guest calls. */
export function createWorkerView({ send, memory, application, linkRequested = () => false, assetRequested = () => false }) {
  const model = createViewModel({ linkRequested, assetRequested });
  let committed = false, batch = 0, lastEvent=0, pending, event, wake, fault, closed = false;
  const pulse = () => { wake?.(); wake = undefined; };
  const fail = (rule) => { throw new ViewProtocolError(rule); };
  return {
    model, pulse,
    get error() { return fault; },
    attached() { return !closed&&batch>0; },
    begin() { committed = false; },
    commit(ptr, length, gesture) {
      const buffer = memory().buffer;
      if (ptr < 0 || length < 0 || length > registry.view.limits.batchBytes || ptr + length > buffer.byteLength) fail("batch bounds");
      if (committed) fail("second commit in one turn");
      const bytes = new Uint8Array(buffer, ptr, length).slice();
      const {effects}=model.commit(bytes); committed = true;
      if(!application)for(const effect of effects)if(effect.name==="copy_text")model.complete(effect.request);
      if (pending) fail("unacknowledged commit");
      const id = ++batch; if (!Number.isSafeInteger(id)) fail("view batch sequence exhausted");
      let acknowledge; const promise = new Promise((resolve) => { acknowledge = resolve; });
      pending = { id, promise, resolve: acknowledge };
      send({ type: "view", id, bytes, gesture }, [bytes.buffer]); return registry.view.statuses.ok;
    },
    receive(message) {
      try {
        if (closed) return;
        if (message?.type === "view_ack") {
          if (!pending || message.id !== pending.id) fail("unexpected view acknowledgement");
          const resolve = pending.resolve; pending = undefined; resolve(); pulse(); return;
        }
        if (message?.type !== "view_event" || !application || event || !Number.isSafeInteger(message.id) || message.id !== lastEvent+1) fail("unexpected view event");
        const bytes = message.bytes;
        const record = decodeViewEvent(bytes);
        if(typeof message.gesture!=="boolean" || message.arrived!==undefined&&(!Number.isFinite(message.arrived)||message.arrived<0))fail("invalid event envelope");
        model.validateChoice(record,message.selectedOption);
        lastEvent=message.id;
        event = { selectedOption:message.selectedOption, arrived:message.arrived??((globalThis.performance?.timeOrigin??0)+(globalThis.performance?.now()??0)), id: message.id, bytes, record, gesture: Boolean(message.gesture && registry.view.events.find((kind) => kind.name === record.kind).gesture) }; pulse();
      } catch (error) { fault = error; pending?.resolve(); pulse(); }
    },
    arrival() { return event?.arrived; },
    take() { if (fault) throw fault; const next = event; event = undefined; if (next) model.syncEvent(next.record,next.selectedOption); return next; },
    async flush() { if (pending) await pending.promise; if (fault) throw fault; },
    wait(milliseconds) {
      if (event || fault) return Promise.resolve();
      return new Promise((resolve) => {
        let timer;
        if (Number.isFinite(milliseconds)) timer = setTimeout(() => { wake = undefined; resolve(); }, Math.max(0, milliseconds));
        wake = () => { clearTimeout(timer); resolve(); };
      });
    },
    close() { closed = true; event = undefined; pending?.resolve(); pending = undefined; pulse(); model.close(); },
  };
}
