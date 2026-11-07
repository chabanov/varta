// Nested and dotted namespaces, exported and hidden members, types inside
// namespaces and merging with namespaces, functions, classes and enums.
namespace Geo {
  export const scale = 2;
  export let calls = 0;
  export function double(x: number): number { calls++; return x * scale; }
  export interface Point { x: number; y: number }
  export type Pair = [number, number];
  export class Vec {
    constructor(public x: number, public y: number) {}
    length(): number { return Math.sqrt(this.x * this.x + this.y * this.y); }
    scaled(): Vec { return new Vec(double(this.x), double(this.y)); }
  }
  export namespace Inner {
    export function origin(): Point { return { x: 0, y: double(1) }; }
    export enum Axis { X = 'x', Y = 'y' }
  }
  const hidden = 7;
  export function peek(): number { return hidden + Inner.origin().y; }
}
namespace Geo {
  export function quadruple(x: number): number { return double(double(x)); }
}
namespace App.Config.Defaults { export const port = 8080; export const host = 'localhost'; }
function greet(name: string): string { return greet.prefix + name + greet.suffix; }
namespace greet { export const prefix = 'hi '; export const suffix = '!'; }
enum Mode { Fast, Safe }
namespace Mode {
  export function parse(text: string): Mode { return text === 'safe' ? Mode.Safe : Mode.Fast; }
}
class Money {
  constructor(public cents: number) {}
  format(): string { return Money.symbol + String(this.cents / 100); }
}
namespace Money { export const symbol = '$'; export function zero(): Money { return new Money(0); } }

export function main(): void {
  console.log(String(Geo.scale) + ' ' + String(Geo.double(5)) + ' ' + String(Geo.quadruple(3)));
  const p: Geo.Point = Geo.Inner.origin();
  const pair: Geo.Pair = [p.x, p.y];
  console.log(String(pair[0]) + ',' + String(pair[1]) + ' ' + String(Geo.peek()));
  const v = new Geo.Vec(3, 4);
  console.log(String(v.length()) + ' ' + String(v.scaled().length()));
  console.log(Geo.Inner.Axis.Y + ' ' + String(Geo.calls));
  Geo.calls = 100;
  console.log(String(Geo.calls));
  console.log(App.Config.Defaults.host + ':' + String(App.Config.Defaults.port));
  console.log(greet('ann'));
  console.log(String(Mode.parse('safe')) + ' ' + Mode[Mode.parse('x')]);
  console.log(new Money(250).format() + ' ' + Money.zero().format());
}
