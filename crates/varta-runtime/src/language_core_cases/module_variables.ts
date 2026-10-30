// Module variables, function-valued module constants and a self-clearing interval.
interface Session { user: string }
let count = 0;
let label = "start";
let session: Session | undefined;
var hits = 10;
const NOOP = (): void => {};
export const double = (x: number): number => x * 2;
const greet = function (name: string): string {
  return "hi " + name;
};
function bump(): number {
  count += 1;
  count++;
  ++count;
  hits--;
  return count;
}
function login(user: string): void {
  session = { user };
}
function current(): string {
  if (session !== undefined) return session.user;
  return "anonymous";
}
export async function main(): Promise<void> {
  NOOP();
  console.log(String(bump()) + " " + String(bump()) + " " + String(hits));
  console.log(current());
  login("ada");
  console.log(current() + " " + greet("bob") + " " + String(double(4)));
  label = label + "!";
  const read = (): string => label + String(count);
  count = 100;
  console.log(read());
  label ||= "unused";
  session ??= { user: "never" };
  console.log(label + " " + current());
  let ticks = 0;
  await new Promise<void>((resolve) => {
    const id = setInterval(() => {
      ticks++;
      if (ticks === 3) {
        clearInterval(id);
        resolve();
      }
    }, 10);
  });
  console.log("ticks " + String(ticks));
  const once = setTimeout(() => clearTimeout(once), 5);
}
