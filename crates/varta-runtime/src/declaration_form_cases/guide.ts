enum Level { Low = 1, Mid, High }
const enum Unit { Px = 'px', Em = 'em' }

namespace Theme {
  export const base = 4;
  export function space(step: Level): string { return String(base * step) + Unit.Px; }
}

abstract class Widget {
  static created = 0;
  constructor(protected label: string) { Widget.created++; }
  abstract render(): string;
  get title(): string { return this.label; }
  set title(value: string) { this.label = value.trim(); }
}

class Button extends Widget {
  accessor pressed = false;
  render(): string { return '[' + this.title + (this.pressed ? '*' : '') + ']'; }
}

function size(level: Level): string;
function size(name: string): string;
function size(value: Level | string): string {
  return typeof value === 'string' ? value : Theme.space(value);
}

export function main(): void {
  const ok = new Button('ok');
  ok.title = '  Save ';
  ok.pressed = true;
  console.log(ok.render() + ' ' + size(Level.High) + ' ' + size('auto')); // [Save*] 12px auto
  console.log(Level[2] + ' ' + String(Widget.created)); // Mid 1
}
