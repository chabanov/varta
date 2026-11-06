// Type predicates, assertion functions, `satisfies` and `never`
// exhaustiveness.
interface Fish {
  swim: number;
}
interface Bird {
  fly: number;
}
type Shape = "circle" | "square" | "triangle";
type Config = { name: string; retries: number };

function isString(value: string | number): value is string {
  return typeof value === "string";
}
function isFish(pet: Fish | Bird): pet is Fish {
  return "swim" in pet;
}
function isDefined<T>(value: T | undefined): value is T {
  return value !== undefined;
}
function assertString(value: unknown): asserts value is string {
  if (typeof value !== "string") {
    throw new TypeError("not a string");
  }
}
function assertTruthy(value: number | null): asserts value {
  if (value === null) {
    throw new Error("null");
  }
}
function sides(shape: Shape): number {
  switch (shape) {
    case "circle":
      return 0;
    case "square":
      return 4;
    case "triangle":
      return 3;
    default: {
      const unreachable: never = shape;
      return unreachable;
    }
  }
}
function describe(value: string | number | boolean): string {
  if (typeof value === "string") {
    return "s";
  } else if (typeof value === "number") {
    return "n";
  } else if (typeof value === "boolean") {
    return "b";
  } else {
    const rest: never = value;
    return rest;
  }
}

export function main(): void {
  const values: (string | number)[] = ["a", 1, "b"];
  let text = "";
  for (const value of values) {
    if (isString(value)) {
      text += value.toUpperCase();
    } else {
      text += String(value * 10);
    }
  }
  console.log(text);
  const pets: (Fish | Bird)[] = [{ swim: 1 }, { fly: 2 }];
  for (const pet of pets) {
    if (isFish(pet)) {
      console.log(`fish ${pet.swim}`);
    } else {
      console.log(`bird ${pet.fly}`);
    }
  }
  const maybe: string | undefined = values.length > 2 ? "here" : undefined;
  if (isDefined(maybe)) {
    console.log(maybe.length);
  }
  const input: unknown = "checked";
  assertString(input);
  console.log(input.toUpperCase());
  const count: number | null = values.length;
  assertTruthy(count);
  console.log(count + 1);
  try {
    assertString(42);
  } catch (error) {
    console.log(error instanceof TypeError ? "TypeError" : "other");
  }
  const config = { name: "svc", retries: 3 } satisfies Config;
  const shapes = ["circle", "square"] satisfies Shape[];
  console.log(`${config.name} ${config.retries} ${shapes.join(",")}`);
  console.log(`${sides("circle")} ${sides("square")} ${sides("triangle")}`);
  console.log(`${describe("x")}${describe(1)}${describe(true)}`);
}
