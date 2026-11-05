// Toasts and the confirm dialog of the accepted console design: short notices
// that leave by themselves, and one question before a destructive action. The
// state is plain signals; the view (src/ui/console.tsx) renders it.
import { Runtime, signal, untracked, type Signal } from "../l1/reactive.js";

/** How long a toast stays. */
export const TOAST_MS = 4000;

export interface Toast {
  id: number;
  text: string;
}

export class Toasts {
  rt: Runtime;
  list: Signal<Toast[]>;
  nextId: number = 1;
  /** How long each toast stays: TOAST_MS in the product. */
  durationMs: number;
  /** Called after every change, so a timer turn commits the view (as ChatStore.afterChange). */
  afterChange: () => void;

  constructor(rt: Runtime, durationMs: number, afterChange: () => void) {
    this.rt = rt;
    this.list = signal<Toast[]>(rt, []);
    this.durationMs = durationMs;
    this.afterChange = afterChange;
  }

  show(text: string): void {
    const id = this.nextId;
    this.nextId += 1;
    const next = untracked<Toast[]>(this.rt, this.list.get).slice(0);
    next.push({ id: id, text: text });
    this.list.set(next);
    this.afterChange();
    setTimeout((): void => this.dismiss(id), this.durationMs);
  }

  dismiss(id: number): void {
    const kept = untracked<Toast[]>(this.rt, this.list.get).filter((toast: Toast): boolean => toast.id !== id);
    this.list.set(kept);
    this.afterChange();
  }
}

/** One question before an action; `run` performs it on confirmation. */
export interface ConfirmRequest {
  title: string;
  text: string;
  action: string;
  run: () => void;
}

export class Confirmation {
  rt: Runtime;
  request: Signal<ConfirmRequest | null>;

  constructor(rt: Runtime) {
    this.rt = rt;
    this.request = signal<ConfirmRequest | null>(rt, null);
  }

  ask(request: ConfirmRequest): void {
    this.request.set(request);
  }

  /** The person answered (or dismissed the dialog); the action runs only on yes. */
  answer(yes: boolean): void {
    const request = untracked<ConfirmRequest | null>(this.rt, this.request.get);
    this.request.set(null);
    if (yes && request !== null) request.run();
  }
}
