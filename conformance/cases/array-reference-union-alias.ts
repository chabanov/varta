export function main():void{const record={n:1};const values=[record,undefined];const first=values[0];if(first){first.n=2;}console.log(record.n);console.log(JSON.stringify(values));}
