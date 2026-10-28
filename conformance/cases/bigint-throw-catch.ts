function fail():void{throw 7n;}export function main():void{try{fail();}catch(e){if(typeof e==='bigint'){console.log(e+1n);}}finally{console.log('finally');}}
