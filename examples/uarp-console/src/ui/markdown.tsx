// A parsed Markdown reply as L0 nodes, with the mockups' classes. Text always
// becomes text nodes, never markup. Links are buttons (opening them needs
// ui.open_url, added with the request lane); tables use grid roles, since the
// L0 v1 subset has no table elements.
import type { Document, Block, Inline, Table } from "../markdown/parse.js";
import { COPY } from "./text.js";

const NO_NODE = 0;

function unreachable(value: never): never {
  throw new Error("unreachable Markdown node");
}

function text(value: string): number {
  const node = View.node();
  View.createText(node, value);
  return node;
}

function append(parent: number, children: number[]): number {
  for (const child of children) View.insert(parent, child, NO_NODE);
  return parent;
}

function inlines(nodes: Inline[]): number[] {
  return nodes.map(inline);
}

function inline(n: Inline): number {
  switch (n.kind) {
    case "text":
      return text(n.text);
    case "code":
      return <code class="inline-code">{n.text}</code>;
    case "strong":
      return append(<strong />, inlines(n.children));
    case "emphasis":
      return append(<em />, inlines(n.children));
    case "link":
      return append(<button class="link" type="button" />, inlines(n.children));
    case "break":
      return <br />;
    default:
      return unreachable(n);
  }
}

function heading(level: number, children: Inline[]): number {
  if (level === 1) return append(<h1 class="h" />, inlines(children));
  if (level === 2) return append(<h2 class="h" />, inlines(children));
  return append(<h3 class="h" />, inlines(children));
}

function codeBlock(lang: string, body: string): number {
  return (
    <div class="code-block">
      <div class="code-head">
        <span>{lang}</span>
        <button class="button ghost small" type="button" onClick={() => View.copyText(body, (status: number): void => {})}>
          {COPY}
        </button>
      </div>
      <pre class="pre">{body}</pre>
    </div>
  );
}

function row(cells: Inline[][], header: boolean): number {
  const r = <div role="row" class="md-row" />;
  for (const cell of cells) View.insert(r, append(header ? <div role="columnheader" class="md-cell head" /> : <div role="cell" class="md-cell" />, inlines(cell)), NO_NODE);
  return r;
}

function table(t: Table): number {
  const grid = <div role="table" class="md-table" />;
  View.insert(grid, row(t.head, true), NO_NODE);
  for (const cells of t.body) View.insert(grid, row(cells, false), NO_NODE);
  return grid;
}

/** One block as an L0 node. */
export function blockNode(b: Block): number {
  switch (b.kind) {
    case "paragraph":
      return append(<p class="p" />, inlines(b.children));
    case "heading":
      return heading(b.level, b.children);
    case "list": {
      const list = b.ordered ? <ol class="list" /> : <ul class="list" />;
      for (const item of b.items) View.insert(list, append(<li />, inlines(item)), NO_NODE);
      return list;
    }
    case "code_block":
      return codeBlock(b.lang, b.text);
    case "table":
      return table(b);
    default:
      return unreachable(b);
  }
}

/** The blocks of a document, appended to `prose`. */
export function renderBlocks(prose: number, d: Document): void {
  for (const b of d.blocks) View.insert(prose, blockNode(b), NO_NODE);
}
