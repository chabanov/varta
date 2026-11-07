function pair(...args:[number,string]):string{return args.length+':'+args[0]+args[1];}
function optional(...args:[number,string?]):string{return args.length+':'+args[0]+':'+String(args[1]);}
function many(...args:[number,...string[]]):string{return args.length+':'+args[0]+':'+args[1];}
function middle(...args:[number,...string[],boolean]):string{return args.length+':'+args[0];}
function length(...args:(number|string)[]):number{return args.length;}
function forward(...args:[number,...string[]]):number{return length(...args);}
function overloaded(...args:[number,...string[],boolean]):string;
function overloaded(...args:(number|string|boolean)[]):string{return String(args.length);}
function collect<A extends unknown[]>(...args:A):A{return args;}
function copy<Args extends unknown[],R>(fn:(...args:Args)=>R):(...args:Args)=>R{return fn;}
function add(n:number,s:string):string{return n+s;}
function invoke<This, Args extends unknown[], R>(fn:(this:This,...args:Args)=>R,receiver:This,...args:Args):R{return fn.call(receiver,...args);}
class Receiver{n:number=10;add(x:number):number{return this.n+x;}}
class Methods{pair(...args:[number,string]):string{return args[0]+args[1];}}
export function main():void{
 const p=pair;console.log(p(2,'x'),optional(3),optional(4,undefined),optional(5,'y'));
 console.log(many(1,'a','b'),middle(2,'x','y',true));
 const inferred=collect(7,'z');console.log(inferred[0],inferred[1]);
 const f=copy(add);const args:[number,string]=[8,'w'];console.log(f(...args));
 const adapted:(n:number,s:string)=>string=pair;console.log(adapted(9,'v'));
 console.log(new Methods().pair(4,'u'));
 const receiver=new Receiver();console.log(invoke(receiver.add,receiver,6));
 console.log(forward(1,'a','b'));
 const arrow:(n:number,s:string)=>string=(...args:[number,string])=>args[0]+args[1];
 const fixed:(...args:[number,string])=>string=(n,s)=>n+s;console.log(arrow(1,'a'),fixed(2,'b'));
 console.log(overloaded(1,'x',true));
}
