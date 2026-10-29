// Checked JSON parsing of dictionaries and recursive types (Varta-specific API).
type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };
interface Tree {
  name: string;
  children: Tree[];
}
interface Config {
  name: string;
  limits: Record<string, number>;
  labels?: { [key: string]: string };
}
export function main(): void {
  const counts = varta.json.parse<Record<string, number>>('{"b":2,"10":10,"a":1,"2":3}');
  console.log(JSON.stringify(counts) + " " + String(Object.keys(counts).length));
  const config = varta.json.parse<Config>('{"limits":{"cpu":2,"mem":512},"name":"svc"}');
  config.limits["disk"] = 9;
  console.log(JSON.stringify(config) + " " + String(config.labels === undefined));
  const tree = varta.json.parse<Tree>('{"name":"r","children":[{"name":"a","children":[]},{"name":"b","children":[{"name":"c","children":[]}]}]}');
  const first = tree.children[1];
  console.log(String(tree.children.length) + " " + (first === undefined ? "" : first.name) + " " + JSON.stringify(tree));
  const doc = varta.json.parse<JsonValue>('{"k":[1,"two",{"three":[null,true]}],"e":{},"s":"\\u00e9"}');
  console.log(JSON.stringify(doc));
  if (typeof doc === "object" && doc !== null && !Array.isArray(doc)) {
    doc["added"] = [false];
    console.log(JSON.stringify(Object.keys(doc)) + " " + JSON.stringify(doc["added"]));
  }
  for (const bad of ['{"a":1,"a":2}', '{"a":"x"}', '[1]']) {
    if (bad === undefined) {
      continue;
    }
    try {
      varta.json.parse<Record<string, number>>(bad);
      console.log("accepted");
    } catch (e) {
      if (e instanceof TypeError) {
        console.log("TypeError " + e.message);
      } else if (e instanceof SyntaxError) {
        console.log("SyntaxError");
      }
    }
  }
}
