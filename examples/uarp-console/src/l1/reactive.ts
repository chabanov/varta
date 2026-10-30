// L1 reactive core: fine-grained signals, lazy cached computeds, effects and
// batches over an explicit runtime object (no module state). Deterministic: the
// order of effect runs is the order in which effects were first scheduled, and a
// change propagates in one glitch-free flush (computeds recompute lazily on read,
// so an effect never observes a half-updated graph).

export type NodeKind = "signal" | "computed" | "effect";

/** One vertex of the dependency graph; values live in closures, not here. */
export interface GraphNode {
  id: number;
  kind: NodeKind;
  sources: GraphNode[];
  observers: GraphNode[];
  dirty: boolean;
  scheduled: boolean;
  disposed: boolean;
  run: () => void;
}

/** A tracked read, as in SolidJS: reading inside a computation subscribes it. */
export type Accessor<T> = () => T;

export interface Signal<T> {
  get: Accessor<T>;
  set: (value: T) => void;
}

export interface Disposer {
  dispose: () => void;
}

export class Runtime {
  observer: GraphNode | null = null;
  batchDepth: number = 0;
  nextId: number = 1;
  queue: GraphNode[] = [];
  flushing: boolean = false;
}

function graphNode(rt: Runtime, kind: NodeKind): GraphNode {
  const id = rt.nextId;
  rt.nextId = id + 1;
  return { id: id, kind: kind, sources: [], observers: [], dirty: false, scheduled: false, disposed: false, run: (): void => {} };
}

function contains(list: GraphNode[], n: GraphNode): boolean {
  for (let i = 0; i < list.length; i++) if (list[i] === n) return true;
  return false;
}

function without(list: GraphNode[], n: GraphNode): GraphNode[] {
  const out: GraphNode[] = [];
  for (let i = 0; i < list.length; i++) {
    const item = list[i];
    if (item !== undefined && item !== n) out.push(item);
  }
  return out;
}

/** Records that the current observer read `source`. */
function track(rt: Runtime, source: GraphNode): void {
  const observer = rt.observer;
  if (observer === null || observer.disposed) return;
  if (!contains(observer.sources, source)) observer.sources.push(source);
  if (!contains(source.observers, observer)) source.observers.push(observer);
}

/** Drops every edge from `n` to its sources before it re-tracks. */
function untrackSources(n: GraphNode): void {
  for (let i = 0; i < n.sources.length; i++) {
    const source = n.sources[i];
    if (source !== undefined) source.observers = without(source.observers, n);
  }
  n.sources = [];
}

/** Marks everything downstream of `source` stale and queues effects. */
function invalidate(rt: Runtime, source: GraphNode): void {
  const observers = source.observers;
  for (let i = 0; i < observers.length; i++) {
    const n = observers[i];
    if (n === undefined || n.disposed) continue;
    if (n.kind === "computed") {
      if (!n.dirty) {
        n.dirty = true;
        invalidate(rt, n);
      }
    } else if (!n.scheduled) {
      n.scheduled = true;
      rt.queue.push(n);
    }
  }
}

function flush(rt: Runtime): void {
  if (rt.flushing || rt.batchDepth > 0) return;
  rt.flushing = true;
  let at = 0;
  while (at < rt.queue.length) {
    const n = rt.queue[at];
    at = at + 1;
    if (n === undefined) continue;
    n.scheduled = false;
    if (!n.disposed) n.run();
  }
  rt.queue = [];
  rt.flushing = false;
}

/** Runs `body` with `n` as the current observer, re-tracking its sources. */
function observe(rt: Runtime, n: GraphNode, body: () => void): void {
  untrackSources(n);
  const previous = rt.observer;
  rt.observer = n;
  body();
  rt.observer = previous;
}

export function signal<T>(rt: Runtime, initial: T): Signal<T> {
  const n = graphNode(rt, "signal");
  let value = initial;
  return {
    get: (): T => {
      track(rt, n);
      return value;
    },
    set: (next: T): void => {
      if (next === value) return;
      value = next;
      invalidate(rt, n);
      flush(rt);
    },
  };
}

export function computed<T>(rt: Runtime, compute: () => T): Accessor<T> {
  const n = graphNode(rt, "computed");
  n.dirty = true;
  let cached: T | null = null;
  let has = false;
  const refresh = (): void => {
    if (!n.dirty && has) return;
    observe(rt, n, () => {
      cached = compute();
    });
    has = true;
    n.dirty = false;
  };
  return (): T => {
    track(rt, n);
    refresh();
    return cached as T;
  };
}

export function effect(rt: Runtime, body: () => void): Disposer {
  const n = graphNode(rt, "effect");
  n.run = (): void => observe(rt, n, body);
  n.run();
  return {
    dispose: (): void => {
      n.disposed = true;
      untrackSources(n);
    },
  };
}

/** Applies several writes and flushes effects once at the end. */
export function batch(rt: Runtime, body: () => void): void {
  rt.batchDepth = rt.batchDepth + 1;
  body();
  rt.batchDepth = rt.batchDepth - 1;
  flush(rt);
}

/** Reads without tracking (a peek). */
export function untracked<T>(rt: Runtime, body: () => T): T {
  const previous = rt.observer;
  rt.observer = null;
  const result = body();
  rt.observer = previous;
  return result;
}
