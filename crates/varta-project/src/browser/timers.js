// Logical due time and stable arm order match native. Only the event-loop wait
// uses a platform timer; guest starts never expose a native timer handle.
function createTimers(profile, scheduler, allowed, emit, renderReason) {
  const S = registry.scheduler, T = S.timer, entries = new Map(), bySerial = new Map(), heap = [];
  let time = 0, nextId = 0, sequence = 0, alive = 0;
  const fail = () => { throw new Error(T.stateError); };
  const before = (a, b) => a.due < b.due || a.due === b.due && a.sequence < b.sequence;
  const swap = (a, b) => { [heap[a], heap[b]] = [heap[b], heap[a]]; heap[a].index = a; heap[b].index = b; };
  const up = (i) => { while (i > 0) { const parent = (i - 1) >>> 1; if (!before(heap[i], heap[parent])) break; swap(i, parent); i = parent; } return i; };
  const down = (i) => {
    while (2 * i + 1 < heap.length) {
      let child = 2 * i + 1;
      if (child + 1 < heap.length && before(heap[child + 1], heap[child])) child++;
      if (!before(heap[child], heap[i])) break;
      swap(i, child); i = child;
    }
  };
  const arm = (timer, due) => {
    timer.due = due; timer.sequence = sequence++; timer.index = heap.length;
    heap.push(timer); up(timer.index); if (timer.keepAlive) alive++;
  };
  const remove = (timer) => {
    if (timer.index < 0) return false;
    const index = timer.index, last = heap.pop();
    if (index < heap.length) { heap[index] = last; last.index = index; down(up(index)); }
    timer.index = -1; if (timer.keepAlive) alive--; return true;
  };
  const valid = (value) => {
    const serial = scheduler.issued(value);
    if (!serial || scheduler.reactionCount(serial) !== 1 || bySerial.has(serial)) fail();
    return serial;
  };
  const capacity = () => { if (entries.size >= profile.schedulerLimits.timers) throw new Error(profile.schedulerLimits.timerError); };
  const flagsValid = (flags) => { if (flags & ~(T.interval | T.unref)) fail(); };
  const start = (value, delay, flags) => {
    flagsValid(flags); const serial = valid(value); capacity();
    if (nextId >= S.maxSerials) throw new Error(S.errors.serials);
    const id = ++nextId;
    const normalized = delay >= T.minimumDelay && delay <= T.maximumDelay ? Math.trunc(delay) : T.minimumDelay;
    const timer = { id, serial, interval: Boolean(flags & T.interval), delay: normalized, keepAlive: !(flags & T.unref), index: -1, fired: false, cleared: false };
    entries.set(id, timer); bySerial.set(serial, id); arm(timer, time + normalized); return id;
  };
  scheduler.setTimerCleared((serial) => entries.get(bySerial.get(serial))?.cleared === true);
  scheduler.setTimerOwner((serial)=>bySerial.has(serial));
  const implementations = {
    timer_start(serial, delay, flags) {
      if (!allowed()) throw new Error(T.deniedError);
      return start(serial, delay, flags);
    },
    timer_start_v2(value, delay, flags) {
      flagsValid(flags);
      if (allowed()) return start(value, delay, flags);
      const serial = valid(value); capacity();
      const id = S.maxSerials + serial;
      entries.set(id, { id, serial, interval: false, keepAlive: false, index: -1, fired: false, cleared: true });
      bySerial.set(serial, id); scheduler.wake(serial); emit(T.denialText); return 0;
    },
    timer_clear(id) {
      if (!Number.isInteger(id) || id < 1 || id > S.maxSerials) return;
      const timer = entries.get(id);
      if (!timer || !remove(timer)) return;
      timer.cleared = true; scheduler.release(timer.serial);
    },
    timer_take(value) {
      const serial = scheduler.issued(value), id = bySerial.get(serial), timer = entries.get(id);
      if (!timer) fail();
      let fired;
      if (timer.fired) { timer.fired = false; if (timer.interval && !timer.cleared) return 1; fired = 1; }
      else if (timer.cleared) fired = 0;
      else fail();
      entries.delete(id); bySerial.delete(serial); return fired;
    },
    timer_uncaught(reason) { throw new Error(`uncaught guest exception: ${renderReason(reason, false)}`); },
  };
  return {
    implementations,
    observe(elapsed) { time = Math.max(time, elapsed); },
    nextDue(otherWork = false) { return alive || otherWork ? heap[0]?.due : undefined; },
    fire() {
      const timer = heap[0]; if (!timer) fail();
      const due = timer.due; remove(timer); time = Math.max(time, due); timer.fired = true;
      scheduler.wake(timer.serial);
      if (timer.interval) { scheduler.pending(timer.serial); arm(timer, due + timer.delay); }
    },
  };
}
