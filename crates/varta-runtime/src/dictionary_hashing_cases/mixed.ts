// V38 hashed dictionaries through mixed records, checked JSON, spreads,
// entries and for...in. Parsed dictionaries start unindexed and are indexed
// by their first keyed access. Expected lines are Node 24's.
interface Problem {
  type?: string;
  status?: number;
  [key: string]: unknown;
}
type Scores = Record<string, number>;
function parse(text: string): Scores {
  return JSON.parse(text) as Scores;
}
export function main(): void {
  let text = "{";
  for (let i = 0; i < 40; i++) {
    text += (i === 0 ? "" : ",") + JSON.stringify("p" + String(i)) + ":" + String(i * 2);
  }
  text += ',"12":1,"3":2}';
  const scores = parse(text);
  console.log(String(scores["p39"]) + " " + String(scores["3"]) + " " + Object.keys(scores).slice(0, 4).join(","));
  delete scores["p0"];
  delete scores["12"];
  scores["p0"] = -1;
  scores["5"] = 5;
  console.log(JSON.stringify(Object.entries(scores).slice(0, 4)) + " " + JSON.stringify(Object.entries(scores).slice(-2)));
  // A spread copies the entries in own-key order.
  const copy: Scores = { ...scores, p1: 100, extra: 7 };
  console.log(String(Object.keys(copy).length) + " " + String(copy["p1"]) + " " + Object.keys(copy).slice(-3).join(","));
  // for...in visits a snapshot and skips keys deleted by the body.
  let visited = 0;
  for (const key in copy) {
    if (key === "p2") {
      delete copy["p3"];
      delete copy["p4"];
    }
    visited++;
  }
  console.log(String(visited) + " " + String(Object.keys(copy).length));
  // Mixed records: declared fields share the hashed entry list.
  const problem: Problem = { type: "about:blank", status: 404 };
  for (let i = 0; i < 12; i++) {
    problem["detail" + String(i)] = "d" + String(i);
  }
  delete problem.type;
  problem.type = "retry";
  problem["status"] = 503;
  console.log(String(problem.status) + " " + String(problem.type) + " " + Object.keys(problem).slice(-3).join(","));
  console.log(JSON.stringify(problem).length);
  const parsed = JSON.parse(JSON.stringify(problem)) as Problem;
  delete parsed["detail5"];
  console.log(String(parsed.status) + " " + String(Object.keys(parsed).length) + " " + String("detail5" in parsed));
  // Values: compound updates over many keys.
  const counts: Scores = {};
  const words = "the quick brown fox jumps over the lazy dog the end fox".split(" ");
  for (let round = 0; round < 5; round++) {
    for (const word of words) {
      counts[word + String(round % 3)] = (counts[word + String(round % 3)] ?? 0) + 1;
    }
  }
  console.log(JSON.stringify(counts));
}
