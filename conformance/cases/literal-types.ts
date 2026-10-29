type Method = "GET" | "POST" | "DELETE";
type Status = 200 | 204 | 404;
interface Route { method: Method; path: string; expect: Status; cached: false }

function parseMethod(text: string): Method | null {
  if (text === "GET" || text === "POST" || text === "DELETE") {
    return text;
  }
  return null;
}

function describe(route: Route): string {
  if (route.expect === 404) {
    return route.method + " " + route.path + " missing";
  }
  return route.method + " " + route.path + " " + String(route.expect);
}

export function main(): void {
  const routes: Route[] = [
    { method: "GET", path: "/agents", expect: 200, cached: false },
    { method: "DELETE", path: "/agents/1", expect: 204, cached: false },
  ];
  for (const route of routes) {
    if (route !== undefined) {
      console.log(describe(route));
    }
  }
  const parsed = parseMethod("POST");
  if (parsed !== null) {
    const created: Route = { method: parsed, path: "/runs", expect: 404, cached: false };
    console.log(describe(created));
    console.log(parsed + " has " + String(parsed.length) + " letters");
  }
  console.log(parseMethod("PATCH") === null);
  console.log(JSON.stringify(routes[0]));
}
