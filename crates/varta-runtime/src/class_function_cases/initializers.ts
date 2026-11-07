let order:string='';
function mark(n:number):number{order+=String(n);return n;}
class Base{
 self:Base=this;
 first:number=mark(1);
 next:number=this.first+mark(2);
 same:boolean=this===this;
 read:()=>number=()=>this.next;
}
class Child extends Base{
 extra:number=this.next+mark(3);
 later:()=>number=()=>this.extra;
}
export function main():void{
 const a=new Base();const b=new Child();
 console.log(a.self===a,b.self===b,a.self===b,order);
 console.log(a.same,b.same,a.read(),b.read(),b.later());
}
