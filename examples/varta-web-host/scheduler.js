// Level R2: the scheduler and timer imports, the microtask checkpoint and the event
// loop of embedding.md "Timers". Misuse raises the reference scheduler errors.
import { authorize } from "./policy.js";
import { ExecutionFailure } from "./errors.js";
import { MS_PER_SECOND, now } from "./context.js";
import { checkException } from "./exception.js";

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
export function schedulerImports(ctx, exec, policies) {
  const { C, schedulerError } = ctx;

  // embedding.md "Which error each misuse raises": "not issued" means not an
  // integer, outside 0 to MAX_PROMISE_SERIALS, or above the last value issued.
  function requireIssued(serial) {
    if (!Number.isInteger(serial) || serial < 0 || serial > C.maxSerials || serial > exec.nextSerial - 1) throw schedulerError("UnknownSerial");
  }
  function hold(token) {
    if (exec.held.size >= C.maxHeldTokens) throw schedulerError("QueueFull");
    if (exec.held.has(token)) throw schedulerError("DuplicateToken");
    exec.held.add(token);
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
      // A denial is fatal, not a status (embedding.md "Timers").
      if (!authorize(policies.package, policies.ceiling, C.timer.capability)) throw schedulerError("TimerDenied");
      if (flags & ~(C.timer.interval | C.timer.unref)) throw schedulerError("TimerState");
      requireIssued(serial);
      if (serial === 0 || (exec.reactions.get(serial) ?? []).length !== 1 || exec.timerBySerial.has(serial)) throw schedulerError("TimerState");
      if (exec.timers.size >= C.timer.maxPending) throw schedulerError("TimerBudget");
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
    },
    timer_clear: (id) => {
      const timer = Number.isInteger(id) ? exec.timers.get(id) : undefined;
      if (!timer || !timer.armed) return;
      timer.armed = false;
      timer.cleared = true;
      const reactions = exec.reactions.get(timer.serial) ?? [];
      if (reactions.length === 0) exec.wakeOnReact.add(timer.serial);
      exec.queue.push(...reactions);
      exec.reactions.delete(timer.serial);
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

/** Microtask checkpoint, extended into the event loop of embedding.md "Timers". */
export async function eventLoop(ctx, exec, info) {
  const { schedulerError } = ctx;
  if (!info.profile?.scheduler || !info.hasFrames) return;
  for (;;) {
    drainMicrotasks(ctx, exec, info);
    if (exec.unhandled.size > 0) {
      // Rendering the boxed reason needs the unstable value layout: it is reported as opaque.
      const reason = [...exec.unhandled.values()][0];
      throw schedulerError("UnhandledRejection", { reason: `opaque value 0x${BigInt.asUintN(64, reason).toString(16)}`, count: exec.unhandled.size });
    }
    const armed = [...exec.timers.values()].filter((t) => t.armed);
    if (!armed.some((t) => !t.unref)) return;
    const next = armed.reduce((a, b) => (b.due < a.due || (b.due === a.due && b.seq < a.seq) ? b : a));
    if (next.due >= exec.deadlineMs) throw schedulerError("TimerDeadline", { due_ms: Math.ceil(next.due).toFixed(0), deadline_seconds: Math.floor(exec.deadlineMs / MS_PER_SECOND) });
    const wait = exec.startedAt + next.due - now();
    if (wait > 0) await sleep(wait);
    if (now() > exec.deadline) throw new ExecutionFailure("execution deadline exceeded");
    exec.loopTime = Math.max(exec.loopTime, next.due);
    // A fired timer whose serial has no reaction list at all is fatal; an empty list
    // (an interval whose frame has not registered again) coalesces the firing:
    // it keeps the fired mark and wakes nothing (embedding.md "Timers").
    const reactions = exec.reactions.get(next.serial);
    if (reactions === undefined) throw schedulerError("TimerState");
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
}
