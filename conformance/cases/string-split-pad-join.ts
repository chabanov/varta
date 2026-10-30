export function main():void{const parts='a,b,,c'.split(',');console.log(parts.map((p)=>p.padStart(2,'.')).join('|')+' '+'Mix'.toUpperCase()+String.fromCharCode(33));}
