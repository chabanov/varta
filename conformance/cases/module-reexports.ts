import { value, increment, renamed } from './module-reexports/barrel.ts';
export function main():void{console.log(value,renamed());increment();console.log(value,renamed());}
