type Event = { kind: "chunk"; text: string } | { kind: "done" };

let text = "";            // module state: every function sees the current value
let finished = 0;

export const record = (e: Event): void => {
  switch (e.kind) {       // narrows `e` by its discriminant
    case "chunk":
      text += e.text;
      break;
    case "done":
      finished++;
      break;
    default: {
      const unreachable: never = e;   // exhaustiveness check
    }
  }
};

export function main(): void {
  record({ kind: "chunk", text: "ab" });
  record({ kind: "done" });
  console.log(text + " " + String(finished));          // ab 1
  const parsed = JSON.parse('{"kind":"chunk","text":"c"}') as Event;
  record(parsed);
  console.log(text);                                    // abc
}
