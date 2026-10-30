// Incremental parsing of a reply that grows while it streams. A blank line
// outside a fenced code block resets every block state of the parser, so the
// text before it parses the same alone as in the whole reply. Blocks up to the
// last such line are parsed once and kept; each update scans and reparses only
// the text after them. The result is always equal to parse(source).
import { parse, type Block, type Document } from "./parse.js";

const FENCE = "```";
const NEWLINE = "\n";

export class MarkdownStream {
  stable: Block[] = [];
  /** The source the stable blocks were parsed from; it ends at a block boundary, outside any fence. */
  stableSource = "";

  reset(): void {
    this.stable = [];
    this.stableSource = "";
  }

  /** `source` normally extends the previous one; text that does not keep the stable part starts over. */
  update(source: string): Document {
    if (!source.startsWith(this.stableSource)) this.reset();
    const start = this.stableSource.length;
    let boundary = start;
    let inFence = false;
    let lineStart = start;
    let lineEnd = source.indexOf(NEWLINE, lineStart);
    while (lineEnd >= 0) {
      const trimmed = source.slice(lineStart, lineEnd).trim();
      if (trimmed.startsWith(FENCE)) inFence = !inFence;
      else if (!inFence && trimmed === "") boundary = lineEnd + 1;
      lineStart = lineEnd + 1;
      lineEnd = source.indexOf(NEWLINE, lineStart);
    }
    if (boundary > start) {
      for (const block of parse(source.slice(start, boundary)).blocks) this.stable.push(block);
      this.stableSource = source.slice(0, boundary);
    }
    const blocks: Block[] = [];
    for (const block of this.stable) blocks.push(block);
    for (const block of parse(source.slice(boundary)).blocks) blocks.push(block);
    return { blocks: blocks };
  }
}
