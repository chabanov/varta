// Level R2: the scheduler and timer imports, the microtask checkpoint and the event
// loop of embedding.md "Timers". Misuse raises the reference scheduler errors.
import { authorize } from "./policy.js";
import { ExecutionFailure, Recoverable } from "./errors.js";
import { MS_PER_SECOND, LIMIT, hostLimit, now } from "./context.js";
import { checkException } from "./exception.js";
import { requestKeepsAlive, deliverCompletion, waitForCompletion } from "./net.js";

// Profiles whose runs are event turns: host calls and microtasks renew per turn
// (embedding.md "Run lifecycle", step 2, v38 `turn_budgets`).
const TURN_PROFILES = new Set(["checked-f64-utf16-v38"]);
const TIMER_DENIED_TEXT = "<capability denied: time.timer> (timer)";

/** Scheduler fields of one execution; loop time is in ms from the execution start. */
export function schedulerState(deadlineMs) {
  return {
    queue: [],
    held: new Set(),
    reactions: new Map(),
    settled: new Set(),
    nextSerial: 1,
    unhandled: new Map(),
    nextUnhandled: 1,
    jobs: 0,
    startedAt: now(),
    deadlineMs,
    loopTime: 0,
    timers: new Map(),
    timerBySerial: new Map(),
    nextTimerId: 1,
    nextTimerSeq: 0,
    wakeOnReact: new Set(),
  };
}

/** Bodies of the scheduler and timer imports for one execution. */
export function schedulerImports(ctx, exec, policies, profileName) {
  const { C, schedulerError } = ctx;
  // Frozen profiles and v38 hold different numbers of tokens and timers (embedding.md "Scheduler limits").
  const scheduled = profileName !== null && ctx.contract.hostLimits.some((r) => r.limit === LIMIT.heldTokens && r.profiles?.includes(profileName));
  const maxHeldTokens = scheduled ? hostLimit(ctx.contract, LIMIT.heldTokens, profileName) : 0;
  const timed = profileName !== null && ctx.contract.hostLimits.some((r) => r.limit === LIMIT.pendingTimers && r.profiles?.includes(profileName));
  const maxPendingTimers = timed ? hostLimit(ctx.contract, LIMIT.pendingTimers, profileName) : 0;

  // embedding.md "Which error each misuse raises": "not issued" means not an
  // integer, outside 0 to MAX_PROMISE_SERIALS, or above the last value issued.
  function requireIssued(serial) {
    if (!Number.isInteger(serial) || serial < 0 || serial > C.maxSerials || serial > exec.nextSerial - 1) throw schedulerError("UnknownSerial");
  }
  function hold(token) {
    if (exec.held.size >= maxHeldTokens) throw schedulerError("QueueFull");
    if (exec.held.has(token)) throw schedulerError("DuplicateToken");
    exec.held.add(token);
  }

  /** Releases a timer frame's reactions as a clear does: they run as microtasks. */
  function release(serial) {
    const reactions = exec.reactions.get(serial) ?? [];
    if (reactions.length === 0) exec.wakeOnReact.add(serial);
    exec.queue.push(...reactions);
    exec.reactions.delete(serial);
  }

  function startTimer(serial, delay, flags) {
    if (flags & ~(C.timer.interval | C.timer.unref)) throw schedulerError("TimerState");
    requireIssued(serial);
    if (serial === 0 || (exec.reactions.get(serial) ?? []).length !== 1 || exec.timerBySerial.has(serial)) throw schedulerError("TimerState");
    if (exec.timers.size >= maxPendingTimers) throw schedulerError("TimerBudget");
    if (exec.nextTimerId > C.maxSerials) throw schedulerError("SerialBudget");
    const normalized = delay >= C.timer.minDelay && delay <= C.timer.maxDelay ? delay : C.timer.minDelay;
    const id = exec.nextTimerId++;
    exec.timers.set(id, {
      serial,
      delay: normalized,
      interval: Boolean(flags & C.timer.interval),
      unref: Boolean(flags & C.timer.unref),
      due: exec.loopTime + normalized,
      seq: exec.nextTimerSeq++,
      armed: true,
      fired: false,
      cleared: false,
    });
    exec.timerBySerial.set(serial, id);
    return id;
  }

  return {
    scheduler_enqueue: (token) => {
      hold(token);
      exec.queue.push(token);
    },
    promise_react: (serial, token) => {
      requireIssued(serial);
      if (serial === 0) {
        if (exec.nextSerial > C.maxSerials) throw schedulerError("SerialBudget");
        serial = exec.nextSerial++;
      }
      if (exec.settled.has(serial)) throw schedulerError("UnknownSerial");
      hold(token);
      if (exec.wakeOnReact.delete(serial)) {
        // An interval cleared from its own callback: queue the frame at once.
        exec.queue.push(token);
        return serial;
      }
      if (!exec.reactions.has(serial)) exec.reactions.set(serial, []);
      exec.reactions.get(serial).push(token);
      return serial;
    },
    promise_settle: (serial) => {
      requireIssued(serial);
      // Serial 0 names no promise; the misuse table has no row for it (reported).
      if (serial === 0 || exec.settled.has(serial)) throw schedulerError("UnknownSerial");
      exec.settled.add(serial);
      exec.queue.push(...(exec.reactions.get(serial) ?? []));
      exec.reactions.delete(serial);
    },
    promise_unhandled: (reason) => {
      if (exec.nextUnhandled > C.maxSerials) throw schedulerError("SerialBudget");
      const id = exec.nextUnhandled++;
      exec.unhandled.set(id, reason);
      return id;
    },
    promise_handled: (id) => {
      if (!exec.unhandled.delete(id)) throw schedulerError("UnknownSerial");
    },
    timer_start: (serial, delay, flags) => {
      // v36: a denial is fatal, not a status (embedding.md "Timers").
      if (!authorize(policies.package, policies.ceiling, C.timer.capability)) throw schedulerError("TimerDenied");
      return startTimer(serial, delay, flags);
    },
    timer_start_v2: (serial, delay, flags) => {
      // v37+: a denial arms nothing, releases the frame as if cleared, logs and returns 0
      // (architecture.md "Permission states", denied timer start).
      if (!authorize(policies.package, policies.ceiling, C.timer.capability)) {
        // The frame takes it like a cleared timer: `timer_take` answers 0 and forgets it.
        // Its key is outside the issued ids, which start at 1, so no guest id names it.
        requireIssued(serial);
        const key = -serial;
        exec.timers.set(key, { serial, armed: false, cleared: true, fired: false });
        exec.timerBySerial.set(serial, key);
        release(serial);
        throw new Recoverable(C.status.Denied, "time.timer denied", TIMER_DENIED_TEXT);
      }
      return startTimer(serial, delay, flags);
    },
    timer_clear: (id) => {
      const timer = Number.isInteger(id) ? exec.timers.get(id) : undefined;
      if (!timer || !timer.armed) return;
      timer.armed = false;
      timer.cleared = true;
      release(timer.serial);
    },
    timer_take: (serial) => {
      requireIssued(serial);
      const id = exec.timerBySerial.get(serial);
      const timer = id === undefined ? undefined : exec.timers.get(id);
      if (!timer) throw schedulerError("TimerState");
      const forget = () => {
        exec.timers.delete(id);
        exec.timerBySerial.delete(serial);
      };
      if (timer.cleared) {
        forget();
        return 0;
      }
      if (!timer.fired) throw schedulerError("TimerState");
      timer.fired = false;
      if (!timer.interval) forget();
      return 1;
    },
    timer_uncaught: (reason) => {
      throw new ExecutionFailure(`uncaught guest exception from a timer (opaque value 0x${BigInt.asUintN(64, reason).toString(16)})`);
    },
  };
}

function drainMicrotasks(ctx, exec, info) {
  const { C, schedulerError } = ctx;
  const resume = exec.instance.exports.varta_resume;
  while (exec.queue.length > 0) {
    const token = exec.queue.shift();
    exec.held.delete(token);
    exec.jobs += 1;
    if (exec.jobs > C.maxJobs) throw schedulerError("JobBudget");
    if (now() > exec.deadline) throw new ExecutionFailure("execution deadline exceeded");
    const outcome = resume(token, C.resume["completion.Normal"], 0n);
    if (outcome !== C.resume["outcome.Suspended"] && outcome !== C.resume["outcome.Returned"]) {
      throw schedulerError("Outcome");
    }
    checkException(C, exec, info, "a job");
  }
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/**
 * Starts a turn (embedding.md "Run lifecycle", step 2): v38 renews host calls,
 * microtasks and the one-commit allowance; an application instance also renews
 * its deadline. Memory and output are never renewed.
 */
export function beginTurn(exec, info, application) {
  exec.viewCommitted = false;
  if (!TURN_PROFILES.has(info.profile?.name)) return;
  exec.hostCalls = 0;
  exec.jobs = 0;
  if (application) exec.deadline = now() + exec.deadlineMs;
}

/** Ends a turn: drain microtasks, then fail on an outstanding unhandled rejection. */
function endTurn(ctx, exec, info) {
  drainMicrotasks(ctx, exec, info);
  if (exec.unhandled.size > 0) {
    const reason = [...exec.unhandled.values()][0];
    throw ctx.schedulerError("UnhandledRejection", { reason: `opaque value 0x${BigInt.asUintN(64, reason).toString(16)}`, count: exec.unhandled.size });
  }
}

/**
 * One view record as one turn (architecture.md "Event delivery"): the host writes
 * the record at the result base and calls the event export, which runs the handler
 * and commits the turn's operations; the exception channel and microtasks follow.
 */
function viewTurn(ctx, exec, info, record) {
  const { C } = ctx;
  beginTurn(exec, info, true);
  const memory = exec.instance.exports.memory;
  if (C.resultBase + record.length > memory.buffer.byteLength) throw new ExecutionFailure("view record exceeds guest memory");
  new Uint8Array(memory.buffer).set(record, C.resultBase);
  exec.instance.exports[`varta.${C.view.event.name}`](record.length);
  checkException(C, exec, info, "a view event");
  endTurn(ctx, exec, info);
}

/**
 * The task queue of a long-lived application instance: view records arrive from
 * the renderer between turns. A coalescible record (input, scroll) replaces the
 * pending record of the same listener; a full queue drops a coalescible record,
 * and any other overflow ends the instance.
 */
export function createInbox(C) {
  const pending = [];
  let wake = null;
  let failure = null;
  const header = (record) => {
    const dv = new DataView(record.buffer, record.byteOffset, record.byteLength);
    return { kind: dv.getUint16(C.view.kindOffset, true), handler: dv.getUint32(C.view.handlerOffset, true) };
  };
  return {
    pending,
    get failure() {
      return failure;
    },
    deliver(record) {
      if (record.length < C.view.eventHeaderBytes || record.length > C.view.maxRecordBytes) {
        failure ??= new ExecutionFailure("malformed view record");
      } else {
        const { kind, handler } = header(record);
        const coalescible = C.view.coalescible.has(kind);
        const same = coalescible ? pending.findIndex((p) => header(p.record).kind === kind && header(p.record).handler === handler) : -1;
        if (same >= 0) pending[same] = { record, arrived: pending[same].arrived };
        else if (pending.length < C.view.pendingEvents) pending.push({ record, arrived: now() });
        else if (!coalescible) failure ??= new ExecutionFailure("view event queue overflow");
      }
      wake?.();
    },
    /** Ends the instance from outside (the renderer refused a batch, or the page stops it). */
    fail(error) {
      failure ??= error;
      wake?.();
    },
    /** Sets the one function a record or a failure calls (null: none). */
    listen: (fn) => {
      wake = fn;
    },
  };
}

/**
 * The loop of a long-lived application instance: view records and fired timers
 * are turns in arrival order; the instance lives while a committed view or an armed
 * timer without the unref bit keeps it, until it fails or `stop` is requested.
 */
export async function applicationLoop(ctx, exec, info, inbox, stopped) {
  if (info.profile?.scheduler && info.hasFrames) endTurn(ctx, exec, info);
  for (;;) {
    // Before choosing, transfers make the progress they can without waiting, so
    // a stream of due timers cannot starve a completion.
    await letTransfersProgress();
    if (inbox.failure) throw inbox.failure;
    if (stopped()) return;
    // One ready task per turn: the earliest instant wins, ties in the order host
    // task, timer, completion (embedding.md "Long-lived application instance
    // (v38)"); a completion's instant is when its transfer reported it.
    const armed = [...exec.timers.values()].filter((t) => t.armed);
    const next = earliest(armed);
    const at = now();
    const ready = [];
    if (inbox.pending.length > 0) ready.push({ kind: "host", instant: inbox.pending[0].arrived });
    if (next !== null && exec.startedAt + next.due <= at) ready.push({ kind: "timer", instant: exec.startedAt + next.due });
    if (exec.net.queue.length > 0) ready.push({ kind: "completion", instant: exec.net.queue[0].arrived });
    if (ready.length > 0) {
      const task = ready.reduce((a, b) => (b.instant < a.instant ? b : a));
      if (task.kind === "host") viewTurn(ctx, exec, info, inbox.pending.shift().record);
      else {
        if (task.kind === "timer") fireTimer(ctx, exec, info, next, true);
        else completionTurn(ctx, exec, info, true);
        endTurn(ctx, exec, info);
      }
      continue;
    }
    const keeps = exec.viewAlive || armed.some((t) => !t.unref) || requestKeepsAlive(exec);
    if (!keeps) return;
    await idle(exec, inbox, next === null ? null : exec.startedAt + next.due - at);
  }
}

/**
 * One macrotask turn of the embedder's event loop, in which network callbacks
 * run (`setImmediate` follows Node's I/O poll; a browser posts a message).
 */
function letTransfersProgress() {
  if (typeof globalThis.setImmediate === "function") return new Promise((resolve) => globalThis.setImmediate(resolve));
  return new Promise((resolve) => {
    const channel = new MessageChannel();
    channel.port1.onmessage = () => {
      channel.port1.close();
      resolve();
    };
    channel.port2.postMessage(null);
  });
}

/**
 * Waits for the first of a host record, the next timer's due time (`sleepMs`,
 * or none) and a completion; the wake-ups that did not fire are withdrawn, so
 * no timer or listener outlives the wait.
 */
function idle(exec, inbox, sleepMs) {
  return new Promise((resolve) => {
    let timer = null;
    const wake = () => {
      if (timer !== null) clearTimeout(timer);
      inbox.listen(null);
      if (exec.net.notify === wake) exec.net.notify = null;
      resolve();
    };
    inbox.listen(wake);
    if (sleepMs !== null) timer = setTimeout(wake, Math.max(0, sleepMs));
    if (requestKeepsAlive(exec)) exec.net.notify = wake;
  });
}

/** Fires a due timer as one turn: its reaction moves to the microtask queue. */
function fireTimer(ctx, exec, info, next, application) {
  beginTurn(exec, info, application);
  exec.loopTime = Math.max(exec.loopTime, next.due);
  // A fired timer whose serial has no reaction list at all is fatal; an empty list
  // (an interval whose frame has not registered again) coalesces the firing:
  // it keeps the fired mark and wakes nothing (embedding.md "Timers").
  const reactions = exec.reactions.get(next.serial);
  if (reactions === undefined) throw ctx.schedulerError("TimerState");
  next.fired = true;
  exec.queue.push(...reactions);
  if (next.interval) {
    exec.reactions.set(next.serial, []);
    next.due += next.delay;
    next.seq = exec.nextTimerSeq++;
  } else {
    exec.reactions.delete(next.serial);
    next.armed = false;
  }
}

const earliest = (timers) => (timers.length === 0 ? null : timers.reduce((a, b) => (b.due < a.due || (b.due === a.due && b.seq < a.seq) ? b : a)));

/** Delivers the oldest queued completion as one turn; loop time follows the wall clock. */
function completionTurn(ctx, exec, info, application) {
  beginTurn(exec, info, application);
  deliverCompletion(exec, {
    wake: (serial) => {
      exec.queue.push(...(exec.reactions.get(serial) ?? []));
      exec.reactions.delete(serial);
    },
  });
  exec.loopTime = Math.max(exec.loopTime, now() - exec.startedAt);
}

/**
 * The event loop of a bounded run (embedding.md "Timers", "Completion ordering"):
 * while no request keeps it alive, the timer loop; while one does, a due timer
 * comes first, otherwise the oldest completion, one per turn, until the deadline.
 */
export async function eventLoop(ctx, exec, info) {
  const { schedulerError } = ctx;
  if (!info.profile?.scheduler || !info.hasFrames) return;
  for (;;) {
    endTurn(ctx, exec, info);
    const armed = [...exec.timers.values()].filter((t) => t.armed);
    const next = earliest(armed);
    if (!requestKeepsAlive(exec)) {
      if (!armed.some((t) => !t.unref)) return;
      if (next.due >= exec.deadlineMs) throw timerDeadline(schedulerError, exec, next);
      const wait = exec.startedAt + next.due - now();
      if (wait > 0) await sleep(wait);
      if (now() > exec.deadline) throw new ExecutionFailure("execution deadline exceeded");
      fireTimer(ctx, exec, info, next, false);
      continue;
    }
    const dueAt = next === null ? Infinity : exec.startedAt + next.due;
    if (next !== null && now() >= dueAt && dueAt < exec.deadline) {
      fireTimer(ctx, exec, info, next, false);
      continue;
    }
    const until = Math.min(dueAt < exec.deadline ? dueAt : Infinity, exec.deadline);
    if (await waitForCompletion(exec, until - now())) {
      completionTurn(ctx, exec, info, false);
      continue;
    }
    if (now() >= exec.deadline) {
      // v38: a timer still armed at or beyond the deadline fails as without a request.
      if (TURN_PROFILES.has(info.profile?.name) && next !== null && next.due >= exec.deadlineMs) throw timerDeadline(schedulerError, exec, next);
      throw new ExecutionFailure("execution deadline exceeded");
    }
  }
}

const timerDeadline = (schedulerError, exec, next) =>
  schedulerError("TimerDeadline", { due_ms: Math.ceil(next.due).toFixed(0), deadline_seconds: Math.floor(exec.deadlineMs / MS_PER_SECOND) });
