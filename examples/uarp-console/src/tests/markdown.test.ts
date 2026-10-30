import { parse } from "../markdown/parse.js";
import { show } from "../markdown/show.js";
import { MarkdownStream } from "../markdown/stream.js";
import { equal, type Report } from "./check.js";

function md(r: Report, name: string, source: string, expected: string): void {
  equal(r, "markdown: " + name, show(parse(source)), expected);
}

export function markdownInlineTests(r: Report): void {
  md(r, "plain paragraph", "Готово.", '(document (paragraph (text "Готово.")))');
  md(r, "soft line joins with a space", "a\nb", '(document (paragraph (text "a b")))');
  md(r, "hard break", "a  \nb", '(document (paragraph (text "a") (break) (text "b")))');
  md(r, "blank line separates paragraphs", "a\n\nb", '(document (paragraph (text "a")) (paragraph (text "b")))');
  md(r, "inline code", "у `research/pl.md`:", '(document (paragraph (text "у ") (code "research/pl.md") (text ":")))');
  md(r, "strong and emphasis", "**3 конкуренти** і *AgentFlow*", '(document (paragraph (strong (text "3 конкуренти")) (text " і ") (emphasis (text "AgentFlow"))))');
  md(r, "underscore emphasis", "_так_", '(document (paragraph (emphasis (text "так"))))');
  md(r, "link", "див. [ціни](https://agentflow.example/pricing)", '(document (paragraph (text "див. ") (link "https://agentflow.example/pricing" (text "ціни"))))');
}

export function markdownBlockTests(r: Report): void {
  md(r, "unmatched markers stay literal", "2 * 3 = 6 і `", '(document (paragraph (text "2 * 3 = 6 і `")))');
  md(r, "escapes", "\\*не курсив\\*", '(document (paragraph (text "*не курсив*")))');
  md(r, "headings 1 to 3", "# A\n## B\n### C", '(document (heading 1 (text "A")) (heading 2 (text "B")) (heading 3 (text "C")))');
  md(r, "h4 is not in the subset", "#### D", '(document (paragraph (text "#### D")))');
  md(r, "bullet list", "- one\n- two", '(document (list (item (text "one")) (item (text "two"))))');
  md(r, "ordered list", "1. one\n2. two", '(document (list ordered (item (text "one")) (item (text "two"))))');
  md(r, "list continuation", "- one\n  more", '(document (list (item (text "one more"))))');
  md(r, "list then paragraph", "- one\n\nafter", '(document (list (item (text "one"))) (paragraph (text "after")))');
}

export function markdownCodeAndTableTests(r: Report): void {
  md(r, "fenced code keeps text", "```ts\nconst a = 1;\n  b();\n```", '(document (code_block ts "const a = 1;\\n  b();"))');
  md(r, "unterminated fence runs to the end", "```\nx", '(document (code_block "x"))');
  md(r, "markup inside code is literal", "```\n**x** <b>\n```", '(document (code_block "**x** <b>"))');
  md(r, "table", "| A | B |\n|---|:-:|\n| 1 | `2` |", '(document (table (row (cell header (text "A")) (cell header (text "B"))) (row (cell (text "1")) (cell (code "2")))))');
  md(r, "block quote stays literal", "> quote", '(document (paragraph (text "> quote")))');
  md(r, "raw HTML stays text", "<script>x</script>", '(document (paragraph (text "<script>x</script>")))');
  md(r, "CRLF lines", "a\r\nb", '(document (paragraph (text "a b")))');
  md(r, "empty input", "", "(document)");
}

// A reply that streams in pieces must render, after every piece, exactly as the
// whole text parsed at once; blocks before the last boundary are parsed once.
// The cut points fall inside a heading, a list item, a fence (before and after its
// blank line) and a table, then the whole text; each half runs as its own group to
// stay within one turn's fuel (a whole parse of this text costs about 190k).
const STREAMED = "# План\n\nКроки:\n- один\n- два\n\n```ts\nconst a = 1;\n\nconst b = 2;\n```\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\nГотово  \nдалі";
const EARLY_CUTS: number[] = [3, 24, 47, 58];
const LATE_CUTS: number[] = [80];

function streamed(r: Report, name: string, stream: MarkdownStream, cuts: number[]): void {
  let same = 0;
  for (const cut of cuts) {
    const prefix = STREAMED.slice(0, cut);
    const actual = show(stream.update(prefix));
    const expected = show(parse(prefix));
    if (actual === expected) same += 1;
    else equal(r, "markdown stream: prefix " + String(cut), actual, expected);
  }
  equal(r, "markdown stream: " + name, String(same), String(cuts.length));
}

export function markdownStreamEarlyTests(r: Report): void {
  const stream = new MarkdownStream();
  streamed(r, "early prefixes equal a whole parse", stream, EARLY_CUTS);
  equal(r, "markdown stream: a blank line inside a fence is no boundary", stream.stableSource.includes("const a") ? "split inside the fence" : "boundary before the fence", "boundary before the fence");
  equal(r, "markdown stream: replaced text starts over", show(stream.update("ok")), show(parse("ok")));
}

export function markdownStreamLateTests(r: Report): void {
  const stream = new MarkdownStream();
  const cuts = LATE_CUTS.slice(0);
  cuts.push(STREAMED.length);
  streamed(r, "late prefixes equal a whole parse", stream, cuts);
  equal(r, "markdown stream: earlier blocks are kept", String(stream.stable.length), "5");
}
