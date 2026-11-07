// Static fields per class (ECMA-262 OrdinarySet on class objects): a
// subclass inherits a static field until code assigns it through the
// subclass, which creates the subclass's own property; the parent keeps
// its value. Reads walk the prototype chain of class objects.
interface Box {
  n: number;
}
class A {
  static x: number = 1;
  static label: string = 'a';
  static box: Box = { n: 1 };
  static maybe: string | undefined = undefined;
  protected static hidden: number = 7;
  static count: number = 0;
  static accessor shared: number = 3;
  static bump(): void {
    this.x = this.x + 10;
  }
  static make(): number {
    return ++this.count;
  }
}
class B extends A {
  static peek(): number {
    return this.hidden;
  }
  static {
    this.label = this.label + 'b';
  }
}
class C extends B {}
class M extends B {
  static label: string = 'm';
}
class N extends M {}
const K = class extends A {
  static twice(): number {
    return this.x * 2;
  }
};
namespace NS {
  export class P extends A {}
}
class G<T> {
  static n: number = 1;
  value: T;
  constructor(value: T) {
    this.value = value;
  }
}
class H extends G<string> {}
class D {
  static '1': number = 1;
  static b: number = 2;
  static '0': number = 3;
}
function local(): number {
  class L extends A {}
  L.x = 9;
  return L.x + A.x;
}
function keys(): string {
  return Object.keys(A).join(',') + '|' + Object.keys(B).join(',') + '|' + Object.keys(C).join(',');
}
export function main(): void {
  console.log(String(B.x) + ' ' + String(C.x) + ' ' + keys());
  B.x = 2;
  console.log(String(A.x) + ' ' + String(B.x) + ' ' + String(C.x));
  A.x = 5;
  C.x += 3;
  console.log(String(A.x) + ' ' + String(B.x) + ' ' + String(C.x) + ' ' + keys());
  C.bump();
  B.bump();
  console.log(String(A.x) + ' ' + String(B.x) + ' ' + String(C.x));
  const v: typeof A = Math.random() < 2 ? C : A;
  v.x = 100;
  v.x++;
  console.log(String(A.x) + ' ' + String(B.x) + ' ' + String(C.x) + ' ' + String(v.x));
  console.log(A.label + ' ' + B.label + ' ' + M.label + ' ' + N.label);
  N.label += '!';
  C.label = 'c';
  console.log(A.label + ' ' + B.label + ' ' + M.label + ' ' + N.label + ' ' + keys());
  B.box = { n: 2 };
  console.log(String(A.box.n) + ' ' + String(B.box.n) + ' ' + String(N.box.n));
  B.maybe ??= 'set';
  A.maybe ||= 'alpha';
  N.maybe &&= 'n';
  console.log(String(A.maybe) + ' ' + String(B.maybe) + ' ' + String(N.maybe));
  console.log(String(B.make()) + ' ' + String(B.make()) + ' ' + String(A.make()) + ' ' + String(N.make()));
  console.log(String(A.count) + ' ' + String(B.count) + ' ' + String(N.count) + ' ' + String(B.peek()));
  B.shared = 30;
  console.log(String(A.shared) + ' ' + String(C.shared));
  const inherited: string[] = [];
  for (const key in C) inherited.push(key);
  console.log(inherited.join(',') + ' | ' + Object.keys(v).join(',') + ' | ' + Object.keys(D).join(','));
  K.x = 3;
  NS.P.x = 4;
  H.n = 6;
  console.log(String(K.x) + ' ' + String(K.twice()) + ' ' + String(NS.P.x) + ' ' + String(G.n) + ' ' + String(H.n) + ' ' + String(local()));
  let k: typeof A = N;
  let sum = '';
  for (let i = 0; i < 2000; i++) {
    k = i % 2 === 0 ? B : N;
    k.box = { n: i };
    sum = String(B.box.n + N.box.n);
  }
  console.log(sum + ' ' + String(A.box.n));
}
