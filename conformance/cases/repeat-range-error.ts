export function main():void{try{'x'.repeat(-1);}catch(error){console.log(error instanceof RangeError);}}
