// Engine prelude of checked-f64-utf16-v38 (`view_protocol`): the guest side
// of the L0 view protocol, draft v0. The compiler adds it to a program whose
// modules name `View.*` members or `ViewEvent`, or contain TSX templates
// (which compile to `View.*` calls); see docs/architecture.md (View protocol).
//
// It encodes one batch per turn in the draft v0 wire format into a string
// whose UTF-16 code units are the batch bytes (two per unit, little endian)
// and commits it with `varta.view.commit`, the one host import
// (`view_commit`). Event records arrive through the guest export
// `varta.view_abi.event`, which the compiler synthesizes to copy the record
// and call `dispatchViewEvent`. Templates bind their holes, getter functions
// `() => T`, through the effect runner that a reactive package (L1) installs
// with `View.setEffect`; without one a getter runs once.

// Wire constants mirror `varta_common::view` (checked by a compiler test).
const DRAFT_VERSION = 0;
const ROOT_NODE = 1;
const NO_NODE = 0;
const ANY_KEY = 0;
const OP_CREATE_ELEMENT = 1;
const OP_CREATE_TEXT = 2;
const OP_SET_TEXT = 3;
const OP_SET_ATTRIBUTE = 4;
const OP_REMOVE_ATTRIBUTE = 5;
const OP_SET_CLASS = 6;
const OP_SET_STYLE = 7;
const OP_REMOVE_STYLE = 8;
const OP_INSERT = 9;
const OP_REMOVE = 10;
const OP_LISTEN = 11;
const OP_UNLISTEN = 12;
const OP_SET_VALUE = 13;
const OP_SET_CHECKED = 14;
const OP_COMMAND = 15;
const OP_STYLESHEET = 16;
const OP_SET_TITLE = 17;
const OP_COPY_TEXT = 18;
const EVENT_INPUT = 2;
const EVENT_CHANGE = 3;
const EVENT_COMMAND_RESULT = 11;
const FLAG_SHIFT = 1;
const FLAG_CTRL = 2;
const FLAG_ALT = 4;
const FLAG_META = 8;
const FLAG_COMPOSING = 16;
const FLAG_CHECKED = 32;
const STATUS_OK = 0;
// Event record header offsets in code units (byte offset / 2).
const RECORD_KIND = 1;
const RECORD_HANDLER = 2;
const RECORD_TARGET = 4;
const RECORD_SEQ = 6;
const RECORD_FLAGS = 8;
const RECORD_STATUS = 10;
const RECORD_SCROLL_TOP = 12;
const RECORD_SCROLL_HEIGHT = 16;
const RECORD_CLIENT_HEIGHT = 20;
const RECORD_STRINGS = 24;
const COPY_TEXT_RESULT = 'copy_text';

// Code units of an open batch chunk before it joins the finished parts.
const CHUNK = 256;
const UNIT = 65536;
const MANTISSA_BITS = 52;
const EXPONENT_BIAS = 1023;
const EXPONENT_MAX = 2047;
const HIGH_MANTISSA = 1048576;

/// One event record, decoded.
export class ViewEvent {
  kind: number;
  handler: number;
  target: number;
  seq: number;
  flags: number;
  status: number;
  scrollTop: number;
  scrollHeight: number;
  clientHeight: number;
  key: string;
  value: string;
  constructor(kind: number, handler: number, target: number) {
    this.kind = kind;
    this.handler = handler;
    this.target = target;
    this.seq = 0;
    this.flags = 0;
    this.status = 0;
    this.scrollTop = 0;
    this.scrollHeight = 0;
    this.clientHeight = 0;
    this.key = '';
    this.value = '';
  }
  shiftKey(): boolean {
    return (this.flags & FLAG_SHIFT) !== 0;
  }
  ctrlKey(): boolean {
    return (this.flags & FLAG_CTRL) !== 0;
  }
  altKey(): boolean {
    return (this.flags & FLAG_ALT) !== 0;
  }
  metaKey(): boolean {
    return (this.flags & FLAG_META) !== 0;
  }
  isComposing(): boolean {
    return (this.flags & FLAG_COMPOSING) !== 0;
  }
  checked(): boolean {
    return (this.flags & FLAG_CHECKED) !== 0;
  }
}

type Handler = (event: ViewEvent) => void;
type Effect = (run: () => void) => void;
type CommandDone = (status: number) => void;

// The pending batch and every guest-side table of the view. Handler and
// node ids are allocated in increasing order and never reused.
interface ViewState {
  // The batch body so far: finished chunks, then the open chunk.
  parts: string[];
  chunk: string;
  count: number;
  nextNode: number;
  handlers: Handler[];
  // Copy requests: ids (from 1) and their callbacks, in parallel.
  requests: number[];
  done: CommandDone[];
  nextRequest: number;
  // The edit counter of each field, by node id (0 when not seen).
  seqs: number[];
  effects: Effect[];
  links: Handler[];
}

const state: ViewState = {
  parts: [],
  chunk: '',
  count: 0,
  nextNode: ROOT_NODE + 1,
  handlers: [],
  requests: [],
  done: [],
  nextRequest: 1,
  seqs: [],
  effects: [],
  links: [],
};

// Append encoded bytes (as code units) to the pending batch. Copies stay
// bounded: the open chunk is moved to the finished parts once it is long.
function append(piece: string): void {
  state.chunk += piece;
  if (state.chunk.length >= CHUNK) {
    state.parts.push(state.chunk);
    state.chunk = '';
  }
}
function low(value: number): number {
  return value % UNIT;
}
function high(value: number): number {
  return Math.floor(value / UNIT) % UNIT;
}
// One operation: its header and fixed operands, packed in one call.
function begin(header: string): void {
  state.count += 1;
  append(header);
}
// A string operand: byte length, UTF-16 units (the string itself) and the
// padding to 4 bytes.
function appendText(value: string): void {
  const bytes = value.length * 2;
  append(varta.view.units(low(bytes), high(bytes)));
  append(value);
  if (value.length % 2 !== 0) {
    append(varta.view.units(0));
  }
}
// The IEEE 754 binary64 bits of a finite nonnegative number, low unit first;
// any other number encodes as +0 (the host then rejects a nonzero argument).
function appendF64(value: number): void {
  if (!(value > 0) || !Number.isFinite(value)) {
    append(varta.view.units(0, 0, 0, 0));
    return;
  }
  let exponent = Math.floor(Math.log2(value));
  while (Math.pow(2, exponent) > value) {
    exponent -= 1;
  }
  while (Math.pow(2, exponent + 1) <= value) {
    exponent += 1;
  }
  const mantissa = (value / Math.pow(2, exponent) - 1) * Math.pow(2, MANTISSA_BITS);
  const top = (exponent + EXPONENT_BIAS) * HIGH_MANTISSA + Math.floor(mantissa / (UNIT * UNIT));
  const bottom = mantissa % (UNIT * UNIT);
  append(varta.view.units(low(bottom), high(bottom), low(top), high(top)));
}

export function allocateNode(): number {
  const id = state.nextNode;
  state.nextNode += 1;
  return id;
}
export function registerHandler(callback: Handler): number {
  state.handlers.push(callback);
  return state.handlers.length;
}
export function createElement(id: number, tag: number): void {
  begin(varta.view.units(OP_CREATE_ELEMENT, 0, low(id), high(id), tag, 0));
}
export function createText(id: number, value: string): void {
  begin(varta.view.units(OP_CREATE_TEXT, 0, low(id), high(id)));
  appendText(value);
}
export function setText(id: number, value: string): void {
  begin(varta.view.units(OP_SET_TEXT, 0, low(id), high(id)));
  appendText(value);
}
export function setAttribute(id: number, attribute: number, value: string): void {
  begin(varta.view.units(OP_SET_ATTRIBUTE, 0, low(id), high(id), attribute, 0));
  appendText(value);
}
export function removeAttribute(id: number, attribute: number): void {
  begin(varta.view.units(OP_REMOVE_ATTRIBUTE, 0, low(id), high(id), attribute, 0));
}
export function setClass(id: number, value: string): void {
  begin(varta.view.units(OP_SET_CLASS, 0, low(id), high(id)));
  appendText(value);
}
export function setStyle(id: number, property: number, value: string): void {
  begin(varta.view.units(OP_SET_STYLE, 0, low(id), high(id), property, 0));
  appendText(value);
}
export function removeStyle(id: number, property: number): void {
  begin(varta.view.units(OP_REMOVE_STYLE, 0, low(id), high(id), property, 0));
}
export function insert(parent: number, child: number, before: number): void {
  begin(
    varta.view.units(
      OP_INSERT,
      0,
      low(parent),
      high(parent),
      low(child),
      high(child),
      low(before),
      high(before),
    ),
  );
}
export function remove(id: number): void {
  begin(varta.view.units(OP_REMOVE, 0, low(id), high(id)));
}
export function listen(
  id: number,
  event: number,
  listener: number,
  options: number,
  key: number,
  required: number,
  forbidden: number,
): void {
  begin(
    varta.view.units(
      OP_LISTEN,
      0,
      low(id),
      high(id),
      event,
      0,
      low(listener),
      high(listener),
      low(options),
      high(options),
      key,
      0,
      low(required),
      high(required),
      low(forbidden),
      high(forbidden),
    ),
  );
}
export function unlisten(id: number, event: number, listener: number): void {
  begin(
    varta.view.units(OP_UNLISTEN, 0, low(id), high(id), event, 0, low(listener), high(listener)),
  );
}
export function setValue(id: number, value: string): void {
  begin(varta.view.units(OP_SET_VALUE, 0, low(id), high(id)));
  appendText(value);
  const seq = seqOf(id);
  append(varta.view.units(low(seq), high(seq)));
}
export function setChecked(id: number, checked: boolean): void {
  const seq = seqOf(id);
  begin(
    varta.view.units(OP_SET_CHECKED, 0, low(id), high(id), checked ? 1 : 0, 0, low(seq), high(seq)),
  );
}
export function command(id: number, which: number, argument: number): void {
  begin(varta.view.units(OP_COMMAND, 0, low(id), high(id), which, 0));
  appendF64(argument);
}
export function stylesheet(sheet: number, value: string): void {
  begin(varta.view.units(OP_STYLESHEET, 0, low(sheet), high(sheet)));
  appendText(value);
}
export function setTitle(value: string): void {
  begin(varta.view.units(OP_SET_TITLE, 0));
  appendText(value);
}
// Copy `value` to the clipboard (`clipboard.write`, user-gesture turns
// only); `done` receives the `command_result` status (0 = ok).
export function copyText(value: string, done: CommandDone): void {
  const request = state.nextRequest;
  state.nextRequest += 1;
  state.requests.push(request);
  state.done.push(done);
  begin(varta.view.units(OP_COPY_TEXT, 0, low(request), high(request)));
  appendText(value);
}
// Receive the `command_result` of every external-link activation.
export function onLinkResult(callback: Handler): void {
  state.links.push(callback);
}

function seqOf(id: number): number {
  return id < state.seqs.length ? state.seqs[id] : 0;
}

// Commit the pending batch, if it holds an operation (at most once a turn).
export function commit(): void {
  if (state.count === 0) {
    return;
  }
  state.parts.push(state.chunk);
  const header = varta.view.units(DRAFT_VERSION, 0, low(state.count), high(state.count));
  const batch = header + state.parts.join('');
  state.parts = [];
  state.chunk = '';
  state.count = 0;
  varta.view.commit(batch);
}

// Insert `root` into the view root and commit.
export function mount(root: number): void {
  insert(ROOT_NODE, root, NO_NODE);
  commit();
}

// The reactive hook of L1: `effect(run)` runs `run` now and again whenever
// what it read changes. The last installed runner wins.
export function setEffect(effect: Effect): void {
  state.effects.push(effect);
}
function track(run: () => void): void {
  if (state.effects.length === 0) {
    run();
    return;
  }
  const effect = state.effects[state.effects.length - 1];
  effect(run);
}
export function bindText(id: number, getter: () => string): void {
  track(() => setText(id, getter()));
}
export function bindAttribute(id: number, attribute: number, getter: () => string): void {
  track(() => setAttribute(id, attribute, getter()));
}
export function bindFlag(id: number, attribute: number, getter: () => boolean): void {
  track(() => {
    if (getter()) {
      setAttribute(id, attribute, '');
    } else {
      removeAttribute(id, attribute);
    }
  });
}
export function bindClass(id: number, getter: () => string): void {
  track(() => setClass(id, getter()));
}
export function bindStyle(id: number, property: number, getter: () => string): void {
  track(() => setStyle(id, property, getter()));
}
export function bindValue(id: number, getter: () => string): void {
  track(() => setValue(id, getter()));
}
export function bindChecked(id: number, getter: () => boolean): void {
  track(() => setChecked(id, getter()));
}
export function on(
  id: number,
  event: number,
  callback: Handler,
  options: number,
  key: number,
  required: number,
  forbidden: number,
): void {
  listen(id, event, registerHandler(callback), options, key, required, forbidden);
}

function u32At(record: string, at: number): number {
  return varta.text.unit(record, at) + varta.text.unit(record, at + 1) * UNIT;
}
function f64At(record: string, at: number): number {
  const bottom = u32At(record, at);
  const top = u32At(record, at + 2);
  const exponent = Math.floor(top / HIGH_MANTISSA) % (EXPONENT_MAX + 1);
  const sign = top >= UNIT * (UNIT / 2) ? -1 : 1;
  const mantissa = (top % HIGH_MANTISSA) * UNIT * UNIT + bottom;
  if (exponent === 0) {
    return sign * mantissa * Math.pow(2, 1 - EXPONENT_BIAS - MANTISSA_BITS);
  }
  return sign * (1 + mantissa / Math.pow(2, MANTISSA_BITS)) * Math.pow(2, exponent - EXPONENT_BIAS);
}
function stringAt(record: string, at: number): string {
  const length = u32At(record, at) / 2;
  return record.slice(at + 2, at + 2 + length);
}

// Decode one record (the event export's copy) and run its handler, then
// commit what the turn changed.
export function dispatchViewEvent(record: string): void {
  const event = new ViewEvent(
    varta.text.unit(record, RECORD_KIND),
    u32At(record, RECORD_HANDLER),
    u32At(record, RECORD_TARGET),
  );
  event.seq = u32At(record, RECORD_SEQ);
  event.flags = u32At(record, RECORD_FLAGS);
  event.status = u32At(record, RECORD_STATUS);
  event.scrollTop = f64At(record, RECORD_SCROLL_TOP);
  event.scrollHeight = f64At(record, RECORD_SCROLL_HEIGHT);
  event.clientHeight = f64At(record, RECORD_CLIENT_HEIGHT);
  const keyLength = u32At(record, RECORD_STRINGS) / 2;
  event.key = stringAt(record, RECORD_STRINGS);
  event.value = stringAt(record, RECORD_STRINGS + 2 + keyLength + (keyLength % 2));
  if (event.kind === EVENT_INPUT || event.kind === EVENT_CHANGE) {
    while (state.seqs.length <= event.target) {
      state.seqs.push(0);
    }
    state.seqs[event.target] = event.seq;
  }
  if (event.kind === EVENT_COMMAND_RESULT) {
    if (event.key === COPY_TEXT_RESULT) {
      // Each request is answered once; its slot is cleared, never reused.
      const at = state.requests.indexOf(event.seq);
      if (at >= 0) {
        const done = state.done[at];
        state.requests[at] = 0;
        done(event.status);
      }
    } else {
      for (const link of state.links) {
        link(event);
      }
    }
  } else if (event.handler >= 1 && event.handler <= state.handlers.length) {
    const callback = state.handlers[event.handler - 1];
    callback(event);
  }
  commit();
}
