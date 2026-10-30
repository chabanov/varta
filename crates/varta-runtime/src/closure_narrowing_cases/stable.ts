// V38 closure narrowing: parameters and `let` bindings that are never
// written after a closure is created keep their narrowing inside it.
type Inline =
  | { kind: "text"; text: string }
  | { kind: "code"; text: string; lang: string }
  | { kind: "link"; href: string; label: string };

function render(n: Inline): () => string {
  switch (n.kind) {
    case "code":
      return () => `<code ${n.lang}>${n.text}</code>`;
    case "link":
      return function (): string {
        return `<a ${n.href}>${n.label}</a>`;
      };
    default:
      return () => n.text;
  }
}

function late(value: string | number): () => string {
  let current = value;
  current = typeof current === "number" ? current * 2 : current;
  if (typeof current === "string") {
    // The last assignment precedes the closure.
    return () => current.toUpperCase();
  }
  return () => `n${current.toFixed(1)}`;
}

function nested(n: Inline): string {
  if (n.kind !== "link") {
    return "-";
  }
  const outer = (): (() => string) => {
    console.log("outer");
    return () => n.href.length + ":" + n.label;
  };
  return outer()();
}

function callbacks(items: Inline[]): string {
  let out = "";
  for (const item of items) {
    if (item.kind === "code") {
      out += [1, 2].map((i) => item.lang + i).join(",") + ";";
    }
  }
  return out;
}

function main(): void {
  console.log(render({ kind: "code", text: "x", lang: "ts" })());
  console.log(render({ kind: "link", href: "/a", label: "A" })());
  console.log(render({ kind: "text", text: "plain" })());
  console.log(late("abc")());
  console.log(late(2)());
  console.log(nested({ kind: "link", href: "/docs", label: "Docs" }));
  console.log(
    callbacks([
      { kind: "code", text: "a", lang: "rs" },
      { kind: "text", text: "b" },
      { kind: "code", text: "c", lang: "go" },
    ]),
  );
}
