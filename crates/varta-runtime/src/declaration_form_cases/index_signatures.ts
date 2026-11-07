// Class index signatures: undeclared members of an instance are its own
// extra properties, read and written by name or by computed string key.
class Bag {
  [key: string]: number;
  count = 0;
}
class Scores extends Bag {
  bonus = 5;
}
function add(bag: Bag, key: string, amount: number): void {
  bag[key] = (bag[key] ?? 0) + amount;
  bag.count++;
}
export function main(): void {
  const bag = new Bag();
  bag['x'] = 1;
  bag.y = 2;
  bag.y += 3;
  bag.y++;
  add(bag, 'x', 10);
  const key: string = 'z';
  bag[key] = 7;
  console.log(String(bag['x'] + bag.y) + ' ' + String(bag[key]) + ' ' + String(bag.count));
  console.log(String(bag.missing === undefined));
  const scores = new Scores();
  scores.top = 99;
  add(scores, 'top', 1);
  console.log(String(scores.top) + ' ' + String(scores.count) + ' ' + String(scores.bonus));
}
