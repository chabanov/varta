// JSON.stringify of class instances with an index signature: the extra
// properties are own properties, listed in JavaScript's own-key order
// (array indices ascending, then the fields and the other keys in
// insertion order).
class Bag {
  [key: string]: number;
  count = 0;
  size = 2;
}
class Scores extends Bag {
  bonus = 5;
}
class Notes {
  [key: string]: string | undefined;
  title = 'notes';
}
export function main(): void {
  const bag = new Bag();
  bag.z = 1;
  bag['10'] = 2;
  bag['2'] = 3;
  bag.a = 4;
  bag.count += 1;
  console.log(JSON.stringify(bag));
  const scores = new Scores();
  scores.top = 9;
  console.log(JSON.stringify([scores, { inner: bag }]));
  console.log(JSON.stringify(scores, ['top', 'count', 'missing']));
  const notes = new Notes();
  notes.body = 'text';
  notes.draft = undefined;
  notes['0'] = 'first';
  console.log(JSON.stringify(notes));
  console.log(JSON.stringify(new Bag()));
}
