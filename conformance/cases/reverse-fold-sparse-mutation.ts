export function main():void{console.log([1,2,3].reduceRight((s,v,i,a)=>{if(i===2)delete a[1];return s+v;},0));}
