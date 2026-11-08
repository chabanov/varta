interface Declared {value:number;}
export function main():void {
 const raw:unknown=JSON.parse('{"value":1,"retained":2}');
 const record=raw as Declared;
 console.log(JSON.stringify(record));
 let strict=false;try{JSON.parse('{"value":1,"retained":2}') as Declared;}catch(error){strict=error instanceof TypeError;}
 console.log('strict parse',strict);
}
