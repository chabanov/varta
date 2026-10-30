// Deterministic Markdown subset for chat replies, parsed into a closed tree that
// maps onto the L0 v1 view subset (embedding.md round 3 "Markdown"): paragraphs
// with hard breaks, h1–h3, bullet and ordered lists, fenced code, GFM tables, and
// inline code, strong, emphasis and links. Everything outside the subset (block
// quotes, rules, h4–h6, raw HTML) stays literal text. No input is ever
// interpreted as markup for the renderer.

export interface Text { kind: "text"; text: string }
export interface Code { kind: "code"; text: string }
export interface Strong { kind: "strong"; children: Inline[] }
export interface Emphasis { kind: "emphasis"; children: Inline[] }
export interface Link { kind: "link"; href: string; children: Inline[] }
export interface Break { kind: "break" }
export type Inline = Text | Code | Strong | Emphasis | Link | Break;

export interface Paragraph { kind: "paragraph"; children: Inline[] }
export interface Heading { kind: "heading"; level: number; children: Inline[] }
/** Each item is its inline content. */
export interface List { kind: "list"; ordered: boolean; items: Inline[][] }
export interface CodeBlock { kind: "code_block"; lang: string; text: string }
/** `head` and every row of `body` hold one inline sequence per cell. */
export interface Table { kind: "table"; head: Inline[][]; body: Inline[][][] }
export type Block = Paragraph | Heading | List | CodeBlock | Table;

export interface Document { blocks: Block[] }

const MAX_HEADING_LEVEL = 3;
const FENCE = "```";
const HARD_BREAK = "  ";
const CONTINUATION_INDENT = "  ";
const BACKSLASH = 92;
const DIGIT_0 = 48;
const DIGIT_9 = 57;

/** Splits on "\n" after removing "\r" before it. */
export function lines(source: string): string[] {
  return source.split("\n").map((line: string): string => (line.endsWith("\r") ? line.slice(0, line.length - 1) : line));
}

// ------------------------------------------------------------------ inline

function closingIndex(text: string, marker: string, from: number): number {
  let at = text.indexOf(marker, from);
  while (at > from && text.charCodeAt(at - 1) === BACKSLASH) at = text.indexOf(marker, at + 1);
  return at;
}

/**
 * Parses inline markup; unmatched markers stay literal. Literal text is taken as
 * slices between markers, so the cost is linear in the input.
 */
export function inline(text: string): Inline[] {
  const out: Inline[] = [];
  let buffer = "";
  let start = 0;
  let at = 0;
  const flush = (end: number): void => {
    buffer += text.slice(start, end);
    if (buffer !== "") out.push({ kind: "text", text: buffer });
    buffer = "";
  };
  while (at < text.length) {
    const ch = text.charAt(at);
    if (ch === "\\" && at + 1 < text.length) {
      buffer += text.slice(start, at) + text.charAt(at + 1);
      at += 2;
      start = at;
      continue;
    }
    if (ch === "`") {
      const close = text.indexOf("`", at + 1);
      if (close > at + 1) {
        flush(at);
        out.push({ kind: "code", text: text.slice(at + 1, close) });
        at = close + 1;
        start = at;
        continue;
      }
    }
    if (text.startsWith("**", at)) {
      const close = closingIndex(text, "**", at + 2);
      if (close > at + 2) {
        flush(at);
        out.push({ kind: "strong", children: inline(text.slice(at + 2, close)) });
        at = close + 2;
        start = at;
        continue;
      }
    }
    if (ch === "*" || ch === "_") {
      const close = closingIndex(text, ch, at + 1);
      if (close > at + 1) {
        flush(at);
        out.push({ kind: "emphasis", children: inline(text.slice(at + 1, close)) });
        at = close + 1;
        start = at;
        continue;
      }
    }
    if (ch === "[") {
      const labelEnd = text.indexOf("](", at + 1);
      const close = labelEnd < 0 ? -1 : text.indexOf(")", labelEnd + 2);
      if (labelEnd > at + 1 && close > labelEnd + 2) {
        flush(at);
        out.push({ kind: "link", href: text.slice(labelEnd + 2, close).trim(), children: inline(text.slice(at + 1, labelEnd)) });
        at = close + 1;
        start = at;
        continue;
      }
    }
    at += 1;
  }
  flush(text.length);
  return out;
}

// ------------------------------------------------------------------- blocks

function headingLevel(line: string): number {
  let level = 0;
  while (level < line.length && line.charAt(level) === "#") level += 1;
  if (level === 0 || level > MAX_HEADING_LEVEL || line.charAt(level) !== " ") return 0;
  return level;
}

/** Marker length of a bullet item ("- ", "* ", "+ "), or 0. */
function bulletLength(line: string): number {
  if (line.startsWith("- ") || line.startsWith("* ") || line.startsWith("+ ")) return 2;
  return 0;
}

/** Marker length of an ordered item ("12. "), or 0. */
function orderedLength(line: string): number {
  let at = 0;
  while (at < line.length && line.charCodeAt(at) >= DIGIT_0 && line.charCodeAt(at) <= DIGIT_9) at += 1;
  if (at === 0 || !line.startsWith(". ", at)) return 0;
  return at + 2;
}

function isTableRow(line: string): boolean {
  return line.startsWith("|");
}

function isDelimiterRow(line: string): boolean {
  if (!isTableRow(line)) return false;
  for (let i = 0; i < line.length; i++) {
    const ch = line.charAt(i);
    if (ch !== "|" && ch !== "-" && ch !== ":" && ch !== " ") return false;
  }
  return line.includes("-");
}

function tableCells(line: string): Inline[][] {
  let body = line.trim();
  if (body.startsWith("|")) body = body.slice(1);
  if (body.endsWith("|")) body = body.slice(0, body.length - 1);
  return body.split("|").map((cell: string): Inline[] => inline(cell.trim()));
}

/** Joins paragraph lines with spaces; a line ending in two spaces is a hard break. */
function paragraph(parts: string[]): Paragraph {
  const children: Inline[] = [];
  let segment = "";
  for (let i = 0; i < parts.length; i++) {
    const raw = parts[i]!;
    const line = raw.trim();
    segment = segment === "" ? line : segment + " " + line;
    const hard = raw.endsWith(HARD_BREAK) && i + 1 < parts.length;
    if (hard || i + 1 === parts.length) {
      for (const child of inline(segment)) children.push(child);
      segment = "";
      if (hard) children.push({ kind: "break" });
    }
  }
  return { kind: "paragraph", children: children };
}

/** Parses a reply into its block tree; deterministic for every input. */
export function parse(source: string): Document {
  const blocks: Block[] = [];
  const all = lines(source);
  const paragraphLines: string[] = [];
  // Item text is collected raw while parsing and parsed inline once at the end.
  let list: List | undefined = undefined;
  let itemText: string | undefined = undefined;
  const endItem = (): void => {
    if (list !== undefined && itemText !== undefined) list.items.push(inline(itemText));
    itemText = undefined;
  };
  const endParagraph = (): void => {
    if (paragraphLines.length > 0) {
      blocks.push(paragraph(paragraphLines));
      paragraphLines.length = 0;
    }
  };
  const endBlock = (): void => {
    endParagraph();
    endItem();
    list = undefined;
  };
  let i = 0;
  while (i < all.length) {
    const line = all[i]!;
    const trimmed = line.trim();

    if (trimmed.startsWith(FENCE)) {
      endBlock();
      const lang = trimmed.slice(FENCE.length).trim();
      const body: string[] = [];
      i += 1;
      while (i < all.length && !all[i]!.trim().startsWith(FENCE)) {
        body.push(all[i]!);
        i += 1;
      }
      blocks.push({ kind: "code_block", lang: lang, text: body.join("\n") });
      i += 1;
      continue;
    }

    if (trimmed === "") {
      endBlock();
      i += 1;
      continue;
    }

    if (itemText !== undefined && line.startsWith(CONTINUATION_INDENT) && bulletLength(trimmed) === 0 && orderedLength(trimmed) === 0) {
      itemText += " " + trimmed;
      i += 1;
      continue;
    }

    const level = headingLevel(trimmed);
    if (level > 0) {
      endBlock();
      blocks.push({ kind: "heading", level: level, children: inline(trimmed.slice(level + 1).trim()) });
      i += 1;
      continue;
    }

    const bullet = bulletLength(trimmed);
    const ordered = orderedLength(trimmed);
    if (bullet > 0 || ordered > 0) {
      endParagraph();
      endItem();
      const isOrdered = ordered > 0;
      if (list === undefined || list.ordered !== isOrdered) {
        const created: List = { kind: "list", ordered: isOrdered, items: [] };
        blocks.push(created);
        list = created;
      }
      itemText = trimmed.slice(isOrdered ? ordered : bullet).trim();
      i += 1;
      continue;
    }

    if (isTableRow(trimmed) && i + 1 < all.length && isDelimiterRow(all[i + 1]!.trim())) {
      endBlock();
      const head = tableCells(trimmed);
      const body: Inline[][][] = [];
      i += 2;
      while (i < all.length && isTableRow(all[i]!.trim())) {
        body.push(tableCells(all[i]!.trim()));
        i += 1;
      }
      blocks.push({ kind: "table", head: head, body: body });
      continue;
    }

    endItem();
    list = undefined;
    paragraphLines.push(line);
    i += 1;
  }
  endBlock();
  return { blocks: blocks };
}
