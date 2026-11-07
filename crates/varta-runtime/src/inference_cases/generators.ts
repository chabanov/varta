function* count(n:number){for(let i=0;i<n;i++){yield i;}return 'done';}
function* mixed(flag:boolean){if(flag){yield 'text';}else{yield 2;}}
function* delegated(){yield* count(1);yield 5;}
function* empty(){}
function* arrayValues(){for(const n of [1,2]){yield n+1;}}
function* returns(flag:boolean){if(flag){return 1;}return 'end';}
export function main():void{
 const c=count(2);console.log(c.next().value,c.next().value,c.next().value,c.next().done);
 console.log(mixed(true).next().value,mixed(false).next().value);
 const seed=5;const closure=function*(){yield seed;return 7;};
 const it=closure();console.log(it.next().value,it.next().value);
 console.log(empty().next().value,empty().next().done);
 console.log(arrayValues().next().value,returns(true).next().value,returns(false).next().value);
 const d=delegated();console.log(d.next().value,d.next().value,d.next().done);
}
