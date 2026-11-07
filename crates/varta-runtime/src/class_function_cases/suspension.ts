class Counter{n:number=5;self:Counter=this; add(x:number):number{this.n+=x;return this.n;}}
function invoke<This,Args extends unknown[],R>(fn:(this:This,...args:Args)=>R,receiver:This,...args:Args):R{return fn.call(receiver,...args);}
async function later(fn:(this:Counter,n:number)=>number,c:Counter):Promise<number>{await Promise.resolve(0);return fn.call(c,2);}
export async function main():Promise<void>{
 const c=new Counter();const method=c.add;
 const pending=later(method,c);await Promise.resolve(1);
 console.log(await pending,invoke(method,c,3),c.self===c,method===c.add);
}
