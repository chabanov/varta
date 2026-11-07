interface Temperature {
  celsius: number;
  readonly fahrenheit: number;
}
let reads = 0;
function thermometer(start: number): Temperature {
  return {
    celsius: start,
    get fahrenheit(): number {
      reads++;
      return this.celsius * 9 / 5 + 32;
    },
  };
}
function fahrenheitOf(t: Temperature): number {
  return t.fahrenheit;
}
function setFahrenheit(target: { fahrenheit: number }): void {
  target.fahrenheit = 1;
}
function caught(run: () => void): string {
  try {
    run();
    return "ok";
  } catch (error) {
    return error instanceof TypeError ? "TypeError" : "other";
  }
}
export function main(): void {
  const t = thermometer(100);
  console.log(String(t.fahrenheit) + " " + String(fahrenheitOf(t)));
  t.celsius = 0;
  console.log(String(t.fahrenheit) + " reads " + String(reads));
  const box = {
    stored: 1,
    get value(): number {
      return this.stored;
    },
    set value(v: number) {
      this.stored = v * 2;
    },
  };
  box.value = 5;
  console.log(String(box.value) + " " + String(box.stored));
  box.value += 1;
  console.log(String(box.value));
  console.log(Object.keys(box).join(","));
  console.log(JSON.stringify(box));
  console.log(JSON.stringify(t));
  const copy = { ...box };
  copy.value = 3;
  console.log(String(copy.value) + " " + String(box.value) + " " + JSON.stringify(copy));
  console.log(Object.entries(t).map(([key, value]) => key + "=" + String(value)).join(" "));
  console.log(caught(() => {
    setFahrenheit(t);
  }));
  let counter = 0;
  const ticker = {
    get next(): number {
      counter++;
      return counter;
    },
  };
  console.log(String(ticker.next + ticker.next) + " " + String(counter));
  const list: Temperature[] = [thermometer(10), thermometer(20)];
  console.log(JSON.stringify({ count: list.length, list }));
  console.log(String(reads));
}
