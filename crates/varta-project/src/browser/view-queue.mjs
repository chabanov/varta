import { registry } from "../../../varta-common/browser/registry.mjs";
import { ViewProtocolError } from "./view-codec.mjs";

/** Native pending-view semantics: stable replacement, bounded owned records. */
export function createViewQueue() {
  const pending = [];
  return {
    push(record) {
      if (record.coalescible) {
        const previous = pending.findIndex((item) => item.coalescible && item.handler === record.handler);
        if (previous >= 0) { pending[previous] = { ...record, ...(pending[previous].arrived===undefined?{}:{arrived:pending[previous].arrived}), gesture: record.gesture || pending[previous].gesture }; return "coalesced"; }
      }
      if (pending.length >= registry.view.limits.events) {
        if (record.coalescible) return "dropped";
        throw new ViewProtocolError("event queue overflow");
      }
      pending.push(record); return "queued";
    },
    shift() { return pending.shift(); },
    clear() { pending.length = 0; },
    get length() { return pending.length; },
  };
}
