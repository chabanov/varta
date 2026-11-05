import { offset } from "./math.js";
export function twice(n: number): number {
  return n * 2 + offset();
}
