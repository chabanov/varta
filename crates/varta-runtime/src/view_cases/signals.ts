// A minimal reactive core (L1) for the view tests: test fixture code, not
// engine code. The real L1 is a typed package outside the engine; this one
// only shows the compiler-side contract: TSX holes are getters `() => T`
// that the engine's view prelude runs through the effect runner installed
// with `View.setEffect`.
interface Tracking {
  running: (() => void)[];
}
const tracking: Tracking = { running: [] };

export function effect(run: () => void): void {
  tracking.running.push(run);
  run();
  tracking.running.pop();
}

function subscribe(subscribers: (() => void)[]): void {
  if (tracking.running.length > 0) {
    subscribers.push(tracking.running[tracking.running.length - 1]);
  }
}

export class NumberSignal {
  value: number;
  subscribers: (() => void)[];
  constructor(value: number) {
    this.value = value;
    this.subscribers = [];
  }
  get(): number {
    subscribe(this.subscribers);
    return this.value;
  }
  set(value: number): void {
    this.value = value;
    for (const run of this.subscribers) {
      run();
    }
  }
}

export class TextSignal {
  value: string;
  subscribers: (() => void)[];
  constructor(value: string) {
    this.value = value;
    this.subscribers = [];
  }
  get(): string {
    subscribe(this.subscribers);
    return this.value;
  }
  set(value: string): void {
    this.value = value;
    for (const run of this.subscribers) {
      run();
    }
  }
}
