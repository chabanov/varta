interface Settings {
  theme: string;
  zoom?: number;
}

class Point {
  readonly x: number;
  constructor(x: number) {
    this.x = x;
    Object.freeze(this);
  }
}

export function main(): void {
  const settings: Settings = Object.freeze({ theme: 'dark' });
  try {
    settings.theme = 'light';
  } catch (error) {
    console.log(String(error)); // TypeError: Cannot assign to read only property 'theme' of object '#<Object>'
  }
  const list = Object.seal([1, 2, 3]);
  list[0] = 10;
  try {
    list.pop();
  } catch (error) {
    console.log(String(error)); // TypeError: Cannot delete property '2' of [object Array]
  }
  console.log(`${Object.isFrozen(settings)} ${Object.isSealed(list)} ${list.join()}`); // true true 10,2,3
  console.log(String(Object.isFrozen(new Point(1)))); // true
}
