import { value } from './module-initialization/dependency.ts';
import { value as again } from './module-initialization/dependency.ts';
function initialize():number{console.log('entry');return value+again;}
const total:number=initialize();
export function main():void{console.log(total);}
