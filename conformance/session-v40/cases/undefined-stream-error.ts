function broken():ReadableStream<Uint8Array> {
 return new ReadableStream<Uint8Array>({start:(controller:ReadableStreamDefaultController<Uint8Array>):void=>{controller.error(undefined);}});
}
export async function main():Promise<void>{
 let reader=false;let body=false;
 try{await broken().getReader().read();}catch(error){reader=error===undefined;}
 try{await new Response(broken()).text();}catch(error){body=error===undefined;}
 console.log('undefined errors preserved',reader,body);
}
