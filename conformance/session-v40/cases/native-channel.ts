export async function main():Promise<void> {
 let refused=false;
 try{await varta.session.fetch('https://127.0.0.1:1/');}catch(error){refused=error instanceof TypeError;}
 console.log('native channel refused',refused);
}
