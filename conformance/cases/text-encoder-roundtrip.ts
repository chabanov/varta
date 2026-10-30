export function main():void{const bytes=new TextEncoder().encode('hé');console.log(`${bytes.length} ${bytes[1]} ${new TextDecoder().decode(bytes)}`);}
