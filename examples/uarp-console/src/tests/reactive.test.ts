import { Runtime, signal, computed, effect, batch, untracked } from "../l1/reactive.js";
import { equal, type Report } from "./check.js";

export function reactiveTests(r: Report): void {
  {
    const rt = new Runtime();
    const count = signal<number>(rt, 1);
    count.set(2);
    equal(r, "reactive: signal holds the last value", String(count.get()), "2");
  }
  {
    const rt = new Runtime();
    const name = signal<string>(rt, "a");
    const seen: string[] = [];
    effect(rt, () => {
      seen.push(name.get());
    });
    name.set("b");
    name.set("b");
    name.set("c");
    equal(r, "reactive: effect runs at once, on change, not on equal writes", JSON.stringify(seen), '["a","b","c"]');
  }
  {
    const rt = new Runtime();
    const base = signal<number>(rt, 2);
    let runs = 0;
    const double = computed<number>(rt, () => {
      runs += 1;
      return base.get() * 2;
    });
    const first = double();
    const second = double();
    base.set(5);
    const third = double();
    equal(r, "reactive: computed is lazy and cached", JSON.stringify([first, second, third, runs]), "[4,4,10,2]");
  }
  {
    const rt = new Runtime();
    const a = signal<number>(rt, 1);
    const b = computed<number>(rt, () => a.get() * 2);
    const c = computed<number>(rt, () => a.get() + 1);
    const d = computed<number>(rt, () => b() + c());
    const seen: number[] = [];
    effect(rt, () => {
      seen.push(d());
    });
    a.set(10);
    equal(r, "reactive: diamond is glitch-free with one run per change", JSON.stringify(seen), "[4,31]");
  }
  {
    const rt = new Runtime();
    const first = signal<string>(rt, "Ada");
    const last = signal<string>(rt, "Lovelace");
    const seen: string[] = [];
    effect(rt, () => {
      seen.push(first.get() + " " + last.get());
    });
    batch(rt, () => {
      first.set("Grace");
      last.set("Hopper");
    });
    equal(r, "reactive: batch flushes once", JSON.stringify(seen), '["Ada Lovelace","Grace Hopper"]');
  }
  {
    const rt = new Runtime();
    const value = signal<number>(rt, 1);
    const seen: number[] = [];
    const handle = effect(rt, () => {
      seen.push(value.get());
    });
    value.set(2);
    handle.dispose();
    value.set(3);
    equal(r, "reactive: disposed effects stop", JSON.stringify(seen), "[1,2]");
  }
  {
    const rt = new Runtime();
    const useA = signal<boolean>(rt, true);
    const a = signal<string>(rt, "a1");
    const b = signal<string>(rt, "b1");
    const seen: string[] = [];
    effect(rt, () => {
      seen.push(useA.get() ? a.get() : b.get());
    });
    useA.set(false);
    a.set("a2");
    b.set("b2");
    equal(r, "reactive: dependencies follow the branch taken", JSON.stringify(seen), '["a1","b1","b2"]');
  }
  {
    const rt = new Runtime();
    const tracked = signal<number>(rt, 1);
    const quiet = signal<number>(rt, 10);
    const seen: number[] = [];
    effect(rt, () => {
      seen.push(tracked.get() + untracked<number>(rt, () => quiet.get()));
    });
    quiet.set(20);
    tracked.set(2);
    equal(r, "reactive: untracked reads do not subscribe", JSON.stringify(seen), "[11,22]");
  }
  {
    const rt = new Runtime();
    const items = signal<string[]>(rt, ["a"]);
    const count = computed<number>(rt, () => items.get().length);
    const seen: number[] = [];
    effect(rt, () => {
      seen.push(count());
    });
    items.set(["a", "b"]);
    items.set(["a", "b", "c"]);
    equal(r, "reactive: arrays are replaced, not mutated", JSON.stringify(seen), "[1,2,3]");
  }
}
