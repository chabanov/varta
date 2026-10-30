// Keyed list tests. Each case builds its own small list, and the groups run as
// separate executions, so every case stays within the per-execution budget.
import { Runtime, signal, effect, untracked, type Signal } from "../l1/reactive.js";
import { mapKeyed, type KeyedList } from "../l1/list.js";
import { equal, type Report } from "./check.js";

interface Message {
  id: string;
  text: string;
}

interface Row {
  id: string;
  rendered: string[];
}

interface Fixture {
  rt: Runtime;
  source: Signal<Message[]>;
  list: KeyedList<Row>;
  mapped: string[];
  released: string[];
}

function message(id: string, text: string): Message {
  return { id: id, text: text };
}

function fixture(initial: Message[]): Fixture {
  const rt = new Runtime();
  const source = signal<Message[]>(rt, initial);
  const mapped: string[] = [];
  const released: string[] = [];
  const list = mapKeyed<Message, Row>(
    rt,
    source.get,
    (m: Message): string => m.id,
    (item, index) => {
      const row: Row = { id: untracked<string>(rt, () => item().id), rendered: [] };
      mapped.push(row.id);
      effect(rt, () => {
        row.rendered.push(String(index()) + ":" + item().text);
      });
      return row;
    },
    (row: Row): void => {
      released.push(row.id);
    },
  );
  return { rt: rt, source: source, list: list, mapped: mapped, released: released };
}

function ids(f: Fixture): string {
  return untracked<Row[]>(f.rt, f.list.results).map((row: Row): string => row.id).join("");
}

function firstRow(f: Fixture): Row {
  return untracked<Row[]>(f.rt, f.list.results)[0]!;
}

export function listMappingTests(r: Report): void {
  const f = fixture([message("a", "hi"), message("b", "yo")]);
  equal(r, "list: initial mapping in order", ids(f) + " " + JSON.stringify(f.mapped), 'ab ["a","b"]');
  f.source.set([message("a", "hi"), message("b", "yo"), message("c", "new")]);
  equal(r, "list: append maps only the new key", ids(f) + " " + JSON.stringify(f.mapped), 'abc ["a","b","c"]');
}

export function listUpdateTests(r: Report): void {
  const f = fixture([message("a", "hi")]);
  f.source.set([message("a", "hi!")]);
  equal(r, "list: a changed item updates in place", JSON.stringify(firstRow(f).rendered) + " " + JSON.stringify(f.mapped), '["0:hi","0:hi!"] ["a"]');
}

export function listMoveTests(r: Report): void {
  const f = fixture([message("a", "x"), message("b", "y")]);
  f.source.set([message("b", "y"), message("a", "x")]);
  equal(r, "list: a move reuses results and updates positions", ids(f) + " " + JSON.stringify(firstRow(f).rendered) + " " + JSON.stringify(f.mapped), 'ba ["1:y","0:y"] ["a","b"]');
}

export function listRemoveTests(r: Report): void {
  const f = fixture([message("a", "x"), message("b", "y")]);
  f.source.set([message("b", "y")]);
  equal(r, "list: a removed key is released once", ids(f) + " " + JSON.stringify(f.released), 'b ["a"]');
  f.list.stop.dispose();
  equal(r, "list: stopping releases the rest", JSON.stringify(f.released), '["a","b"]');

  const dup = new Runtime();
  const dupSource = signal<Message[]>(dup, [message("x", "1"), message("x", "2")]);
  let failed = "no error";
  try {
    mapKeyed<Message, string>(dup, dupSource.get, (m: Message): string => m.id, (item) => untracked<string>(dup, () => item().id), (_s: string): void => {});
  } catch (e) {
    failed = "error";
  }
  equal(r, "list: duplicate keys fail deterministically", failed, "error");
}
