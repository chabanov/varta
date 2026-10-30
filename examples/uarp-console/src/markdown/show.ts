// Compact, deterministic text form of a Markdown tree, used by tests:
// (kind attrs "text" children…).
import type { Document, Block, Inline } from "./parse.js";

function quote(text: string): string {
  return JSON.stringify(text);
}

// Exhaustiveness: a new member of Inline or Block fails to compile here.
function unreachable(value: never): never {
  throw new Error("unreachable Markdown node");
}

function sequence(nodes: Inline[]): string {
  return nodes.map((n: Inline): string => " " + showInline(n)).join("");
}

function showInline(n: Inline): string {
  switch (n.kind) {
    case "text":
    case "code":
      return "(" + n.kind + " " + quote(n.text) + ")";
    case "strong":
    case "emphasis":
      return "(" + n.kind + sequence(n.children) + ")";
    case "link":
      return "(link " + quote(n.href) + sequence(n.children) + ")";
    case "break":
      return "(break)";
    default:
      return unreachable(n);
  }
}

function row(cells: Inline[][], header: boolean): string {
  return " (row" + cells.map((cell: Inline[]): string => " (cell" + (header ? " header" : "") + sequence(cell) + ")").join("") + ")";
}

function showBlock(b: Block): string {
  switch (b.kind) {
    case "paragraph":
      return "(paragraph" + sequence(b.children) + ")";
    case "heading":
      return "(heading " + String(b.level) + sequence(b.children) + ")";
    case "list":
      return "(list" + (b.ordered ? " ordered" : "") + b.items.map((item: Inline[]): string => " (item" + sequence(item) + ")").join("") + ")";
    case "code_block":
      return "(code_block" + (b.lang !== "" ? " " + b.lang : "") + " " + quote(b.text) + ")";
    case "table":
      return "(table" + row(b.head, true) + b.body.map((cells: Inline[][]): string => row(cells, false)).join("") + ")";
    default:
      return unreachable(b);
  }
}

export function show(d: Document): string {
  return "(document" + d.blocks.map((b: Block): string => " " + showBlock(b)).join("") + ")";
}
