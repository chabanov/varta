// One bounded FIFO checkpoint. Promise values remain entirely in guest memory.
function createScheduler(profile, renderReason) {
  const S = registry.scheduler, E = S.errors;
  const L = profile.schedulerLimits;
  // Outstanding tracking ids in issue order, so the first is the earliest.
  const queue = [], held = new Set(), reactions = new Map(), reasons = new Map(), outstanding = new Set();
  let head = 0, serial = 0, tracking = 0, unhandled = 0, jobs = 0, totalJobs = 0;
  let timerCleared = () => false;
  let timerOwned = () => false;
  const fail = (message) => { throw new Error(message); };
  const issued = (value, last) => {
    if (!Number.isInteger(value) || value < 0 || value > last) fail(E.unknown);
    return value;
  };
  const hold = (token) => {
    if (held.size >= L.held) fail(L.queueError);
    if (held.has(token)) fail(E.duplicate);
    held.add(token);
  };
  const push = (token) => { queue.push(token); };
  const implementations = {
    scheduler_enqueue(token) { hold(token); push(token); },
    promise_react(value, token) {
      let id = issued(value, serial);
      if (id && !reactions.has(id) && timerCleared(id)) { hold(token); push(token); return id; }
      if (!id) {
        if (serial >= L.serials) fail(L.serialError);
        id = ++serial; reactions.set(id, []);
      }
      if (!reactions.has(id)) fail(E.unknown);
      hold(token); reactions.get(id).push(token); return id;
    },
    promise_settle(value) {
      const id = issued(value, serial), waiting = reactions.get(id);
      if (!waiting) fail(E.unknown);
      // No spread: a legal v38 batch can exceed the JS argument-count limit.
      for (const token of waiting) push(token);
      reactions.delete(id);
    },
    promise_unhandled(value) {
      // Validate even a reason whose text cannot be retained, like native.
      const reason = renderReason(value);
      if (tracking >= L.serials) fail(L.serialError);
      if (outstanding.size >= L.rejections) fail(L.rejectionError);
      const id = ++tracking;
      outstanding.add(id); unhandled++;
      if (reasons.size < S.retainedReasons) reasons.set(id, reason);
      return id;
    },
    promise_handled(value) {
      const id = issued(value, tracking);
      if (!outstanding.delete(id)) fail(E.unknown);
      unhandled--; reasons.delete(id);
    },
  };
  // V38 (`weak_references`): once the queue is empty, the module's weak
  // checkpoint export clears the kept objects and may queue finalization
  // cleanup jobs, which this checkpoint drains too.
  const drain = (instance, checkException) => {
    for (;;) {
      drainQueue(instance, checkException);
      const checkpoint = instance.exports?.[S.weakCheckpointExport];
      if (typeof checkpoint !== "function") return;
      checkpoint(); checkException();
      if (head >= queue.length) return;
    }
  };
  const drainQueue = (instance, checkException) => {
    while (head < queue.length) {
      const token = queue[head++]; held.delete(token);
      if (jobs >= S.maxJobs) fail(E.jobs);
      jobs++; totalJobs++;
      // Compact consumed storage without shifting every job. This also bounds
      // storage while a recursive chain continually queues its successor.
      if (head * 2 >= queue.length) { queue.splice(0, head); head = 0; }
      const resume = instance.exports[S.resumeExport];
      if (typeof resume !== "function") fail(E.outcome);
      const outcome = resume(token, S.normal, 0n);
      if (!S.outcomes.includes(outcome)) fail(E.outcome);
      checkException();
    }
    if (unhandled) {
      const earliest = outstanding.values().next().value;
      fail(`unhandled promise rejection: ${reasons.get(earliest) ?? S.notRetained} (${unhandled} rejected promise(s) had no handler after the microtask checkpoint)`);
    }
  };
  return {
    implementations, drain, get jobs() { return jobs; }, get totalJobs() { return totalJobs; },
    renew() { jobs = 0; },
    issued(value) { return issued(value, serial); },
    watchable(value) {
      const id=issued(value,serial);
      if(!id||reactions.get(id)?.length!==1||timerOwned(id))fail(E.unknown);
      return id;
    },
    reactionCount(id) { return reactions.get(id)?.length ?? -1; },
    wake(id) {
      const waiting = reactions.get(id); if (!waiting) fail(E.unknown);
      for (const token of waiting) push(token); reactions.delete(id);
    },
    release(id) { if (reactions.has(id)) this.wake(id); },
    pending(id) { reactions.set(id, []); },
    setTimerCleared(callback) { timerCleared = callback; },
    setTimerOwner(callback) { timerOwned = callback; },
  };
}
