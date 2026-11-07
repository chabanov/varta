interface A {kind:'a'; n:number; extra:string;}
interface B {kind:'b'; other:boolean; n:number;}
class C {n:number; constructor(n:number){this.n=n;}}
function read(v:{n:number}):number{return v.n;}
function bump(v:{n:number}):{n:number}{v.n+=10;return v;}
function pass(v:A|B|C):{n:number}{return v;}
function optional(v:{extra?:string}):string|undefined{return v.extra;}
export function main():void{
 const a:A={kind:'a',n:1,extra:'x'};
 const b:B={kind:'b',other:true,n:2};
 const c:C=new C(3);
 const items:(A|B|C)[]=[a,b,c];
 console.log(read(items[0]),read(items[1]),read(items[2]));
 const same=bump(items[0]); bump(items[1]);bump(items[2]);
 console.log(same===a,a.n,b.n,c.n);
 const f:(v:A|B|C)=>number=read;
 console.log(read(pass(items[0]))+3,f(items[1])+3,f(items[2])+3);
 const either:{extra:string}|{extra?:string;n:number}=a;
 const missing:{extra?:string;n:number}={n:2};
 console.log(optional(missing),optional(either));
 console.log(JSON.stringify(pass(items[0])));
}
