function* values(){yield 1;}export function main():void{const iterator=values();console.log(iterator.next().value);}
