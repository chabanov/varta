// A generic Markdown-like block parser of the shape a chat client renders a
// reply with: a heading, a list, a fenced block, a table and a paragraph,
// mostly ASCII with a few non-ASCII characters. It scans with `charAt` and
// index reads; `codegen_v2_tests` replaces the constant below.
const REPEAT = 1;

interface Block { kind: string; text: string; cells: string[] }

function isSpace(c: string): boolean { return c === " " || c === "\t"; }

function trimmed(line: string): string {
  let start = 0;
  let end = line.length;
  while (start < end && isSpace(line.charAt(start))) { start++; }
  while (end > start && isSpace(line.charAt(end - 1))) { end--; }
  return line.slice(start, end);
}

function inline(text: string): string {
  let out = "";
  let strong = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === "*" && text.charAt(i + 1) === "*") {
      out += strong ? "</b>" : "<b>";
      strong = !strong;
      i++;
    } else if (c === "`") {
      out += "<code>";
    } else {
      out += c;
    }
  }
  return out;
}

function cells(line: string): string[] {
  const out: string[] = [];
  let cell = "";
  for (let i = 1; i < line.length; i++) {
    const c = line.charAt(i);
    if (c === "|") { out.push(trimmed(cell)); cell = ""; } else { cell += c; }
  }
  return out;
}

function parse(text: string): Block[] {
  const blocks: Block[] = [];
  let line = "";
  let fence = false;
  let code = "";
  const lines: string[] = [];
  for (let i = 0; i < text.length; i++) {
    const c = text.charAt(i);
    if (c === "\n") { lines.push(line); line = ""; } else { line += c; }
  }
  lines.push(line);
  for (const raw of lines) {
    const current = trimmed(raw);
    if (current.startsWith("```")) {
      if (fence) { blocks.push({ kind: "pre", text: code, cells: [] }); code = ""; }
      fence = !fence;
    } else if (fence) {
      code += raw + "\n";
    } else if (current.charAt(0) === "#") {
      let level = 0;
      while (current.charAt(level) === "#") { level++; }
      blocks.push({ kind: "h" + String(level), text: inline(trimmed(current.slice(level))), cells: [] });
    } else if (current.startsWith("- ")) {
      blocks.push({ kind: "li", text: inline(current.slice(2)), cells: [] });
    } else if (current.charAt(0) === "|") {
      blocks.push({ kind: "tr", text: "", cells: cells(current) });
    } else if (current.length > 0) {
      blocks.push({ kind: "p", text: inline(current), cells: [] });
    }
  }
  return blocks;
}

export function main(): void {
  let reply = "";
  for (let i = 0; i < REPEAT; i++) {
    reply += "# Résumé\n- first **item**\n- second — `x`\n```\nlet a = 1;\n```\n| a | b |\n| 1 | 😀 |\nPlain text, done.\n";
  }
  const blocks = parse(reply);
  let size = 0;
  for (const block of blocks) { size += block.text.length + block.cells.length; }
  console.log(String(reply.length) + " " + String(blocks.length) + " " + String(size));
}
