export function main():void {
 let read=false;let write=false;let clear=false;
 try{varta.http.session.get();}catch(error){read=error instanceof DOMException&&error.name==='NotAllowedError';}
 try{varta.http.session.set('synthetic');}catch(error){write=error instanceof DOMException&&error.name==='NotAllowedError';}
 try{varta.http.session.clear();}catch(error){clear=error instanceof DOMException&&error.name==='NotAllowedError';}
 console.log(read,write,clear);
}
