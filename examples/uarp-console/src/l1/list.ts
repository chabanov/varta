// Keyed list mapping for L1: a list of results that follows a source array by key.
// An existing key keeps its result; its item and position change through signals,
// so the mapped view updates in place. New keys are mapped once, removed keys are
// disposed. Duplicate keys fail deterministically.
import { Runtime, signal, effect, untracked, type Accessor, type Signal, type Disposer } from "./reactive.js";

interface Entry<T, R> {
  key: string;
  item: Signal<T>;
  index: Signal<number>;
  result: R;
}

export interface KeyedList<R> {
  results: Accessor<R[]>;
  stop: Disposer;
}

/**
 * @param map called once per new key, without tracking, with signals for the
 *   item and its position
 * @param release called once when a key leaves the list (or the list stops)
 */
export function mapKeyed<T, R>(
  rt: Runtime,
  source: Accessor<T[]>,
  key: (item: T) => string,
  map: (item: Accessor<T>, index: Accessor<number>) => R,
  release: (result: R) => void,
): KeyedList<R> {
  const results = signal<R[]>(rt, []);
  const live: { [key: string]: Entry<T, R> } = {};
  let order: string[] = [];

  const stop = effect(rt, () => {
    const items = source();
    untracked<number>(rt, () => {
      const seen: { [key: string]: boolean } = {};
      const nextOrder: string[] = [];
      const nextResults: R[] = [];
      for (let i = 0; i < items.length; i++) {
        const item = items[i]!;
        const k = key(item);
        if (seen[k] === true) throw new Error("mapKeyed: duplicate key " + k);
        seen[k] = true;
        const existing = live[k];
        if (existing !== undefined) {
          existing.item.set(item);
          existing.index.set(i);
          nextResults.push(existing.result);
        } else {
          const itemSignal = signal<T>(rt, item);
          const indexSignal = signal<number>(rt, i);
          const result = map(itemSignal.get, indexSignal.get);
          live[k] = { key: k, item: itemSignal, index: indexSignal, result: result };
          nextResults.push(result);
        }
        nextOrder.push(k);
      }
      for (const old of order) {
        if (seen[old] !== true) {
          const gone = live[old];
          if (gone !== undefined) release(gone.result);
          delete live[old];
        }
      }
      order = nextOrder;
      results.set(nextResults);
      return nextResults.length;
    });
  });

  return {
    results: results.get,
    stop: {
      dispose: (): void => {
        stop.dispose();
        for (const k of order) {
          const entry = live[k];
          if (entry !== undefined) release(entry.result);
        }
        order = [];
      },
    },
  };
}
