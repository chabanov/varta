// A generic JSON round trip with logged records, shaped like a tool-call
// screen: each call's arguments are serialized and logged, the call is
// parsed back from its JSON text, and records, arrays, dictionaries and
// unions are logged as values.
interface ToolCall {
  name: string;
  arguments: Record<string, unknown>;
  id?: number;
  note?: string | null;
}
interface Screen {
  title: string;
  calls: ToolCall[];
  totals: Record<string, number>;
}

function describe(call: ToolCall): string {
  const text = JSON.stringify(call.arguments);
  return call.name + "(" + text + ")";
}

export function main(): void {
  const calls: ToolCall[] = [
    { name: "list", arguments: { path: "/notes", limit: 20, recursive: false } },
    { name: "read", arguments: { path: "/notes/today.md", range: [1, 40] }, id: 7 },
    { name: "write", arguments: { path: "/notes/é😀.md", body: "line\n\"quoted\"\ttab", tags: ["a", "b"] }, note: null },
    { name: "search", arguments: { query: "x\ud800y", options: { fuzzy: true, weight: 0.25 } }, note: "lone" },
  ];
  const totals: Record<string, number> = {};
  for (const call of calls) {
    console.log(describe(call));
    totals[call.name] = JSON.stringify(call).length;
  }
  const screen: Screen = { title: "Tool calls", calls: calls, totals: totals };
  const text = JSON.stringify(screen);
  const back = JSON.parse(text) as Screen;
  console.log(String(JSON.stringify(back) === text) + " " + String(text.length));
  const shown: unknown = back.calls[2];
  console.log(shown);
  const dictionary: unknown = back.totals;
  console.log(dictionary);
  const mixed: unknown = [1.5, -0, "two", null, true, { nested: [back.calls[1].id, back.calls[0].id] }];
  console.log(mixed);
  const cell: number | string = back.calls.length;
  console.log(cell);
}
