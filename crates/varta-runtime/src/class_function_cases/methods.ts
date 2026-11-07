let reads:number=0;
class Base{n:number=2;shared(x:number):number{return this.n+x;} override(x:number):number{return this.n+x;}}
class Child extends Base{override(x:number):number{return this.n*x;}}
class Box<T>{value:T;constructor(v:T){this.value=v;}get():T{return this.value;}}
function receiver(v:Child):Child{reads++;return v;}
export function main():void{
 const a=new Base();const b=new Child();const c=new Child();
 const shared=a.shared;const inherited=b.shared;const own=receiver(b).override;
 const typed:(this:Base,x:number)=>number=shared;
 console.log(shared===inherited,own===c.override,reads);
 console.log(shared.call(a,3),inherited.call(b,4),own.call(c,5),typed.call(b,6));
 const poly:Base=b;console.log(poly.override(7));
 const box=new Box<number>(9);const get=box.get;console.log(get.call(box));
}
