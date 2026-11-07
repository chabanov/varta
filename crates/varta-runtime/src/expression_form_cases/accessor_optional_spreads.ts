interface Options {
  label?: string;
  size: number;
}
interface Shown {
  label?: string;
  size: number;
  readonly text: string;
}
function show(options: Options, extra: Options): Shown {
  return {
    ...options,
    get text(): string {
      return (this.label ?? "none") + ":" + String(this.size);
    },
    ...extra,
  };
}
export function main(): void {
  const a = show({ size: 1 }, { size: 2 });
  console.log(Object.keys(a).join(",") + " " + a.text + " " + JSON.stringify(a));
  const b = show({ label: "x", size: 1 }, { size: 3 });
  console.log(Object.keys(b).join(",") + " " + b.text + " " + JSON.stringify(b));
  const c = show({ size: 1 }, { label: "late", size: 4 });
  console.log(Object.keys(c).join(",") + " " + c.text + " " + JSON.stringify(c));
}
