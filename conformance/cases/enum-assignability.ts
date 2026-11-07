// Enum assignability as tsc decides it: any number is assignable to a
// numeric enum type; string enum values are nominal to their enum.
enum Level { Low = 1, Mid, High }
enum Direction { Up = 'UP', Down = 'DOWN' }
enum Mixed { No = 0, Yes = 'YES' }
function levelName(level: Level): string {
  switch (level) {
    case Level.Low: return 'low';
    case Level.Mid: return 'mid';
    case Level.High: return 'high';
    default: return 'level ' + String(level);
  }
}
function turn(direction: Direction): Direction {
  if (direction === Direction.Up) return Direction.Down;
  return Direction.Up;
}
function describe(value: Mixed): string { return typeof value === 'string' ? 'text ' + value : 'number ' + String(value); }
export function main(): void {
  const raw: number = 2;
  const level: Level = raw;
  let other: Level = Level.Low;
  const seven: number = 7;
  other = seven;
  console.log(levelName(level) + ' ' + levelName(other) + ' ' + levelName(3));
  const direction = turn(Direction.Up);
  const text: string = direction;
  console.log(text + ' ' + turn(direction) + ' ' + String(direction === Direction.Down));
  console.log(describe(Mixed.Yes) + ' ' + describe(Mixed.No) + ' ' + describe(0));
  const parsed = 'UP' as Direction;
  console.log(parsed + ' ' + String(parsed === Direction.Up));
}
