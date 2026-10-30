// v38: host calls renew per turn. Three timer turns of 600 log calls each exceed
// one execution's allowance but stay within every turn's.
const CALLS_PER_TURN = 600;
const TURNS = 3;

function burst(turn: number): void {
  for (let i = 0; i < CALLS_PER_TURN; i++) console.log("turn " + String(turn));
  if (turn < TURNS) setTimeout(() => burst(turn + 1), 1);
}

export function main(): void {
  setTimeout(() => burst(1), 1);
}
