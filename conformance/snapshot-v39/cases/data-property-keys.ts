interface A { n: number; child: { x: number } }
export function main(): void {
 const raw: unknown = JSON.parse('{"n":1,"__proto__":{"kept":true},"child":{"x":2,"__proto__":[3]},"constructor":"data"}');
 const a = raw as A;
 console.log(JSON.stringify(a));
 console.log(JSON.stringify(Object.keys(a)));
 console.log(JSON.stringify((a as unknown) as Record<string, unknown>));
 console.log(JSON.stringify(a as Record<string, unknown>));
 const copy = { ...a };
 console.log(JSON.stringify(copy));
 console.log(Object.hasOwn(copy, '__proto__'));
 try { Object.assign({}, a); console.log('wrong assign'); }
 catch (error) { if (error instanceof TypeError) console.log('assign setter refused'); }
 const d: Record<string, unknown> = {};
 const key = '__proto__';
 try { d[key] = 1; console.log('wrong set'); }
 catch (error) { if (error instanceof TypeError) console.log('ordinary setter refused'); }
}
