type Score = number;
type Tags = Tag[];
interface Tag { name: string; score: Score; note?: string }
type Same = { score: Score; name: string; note?: string };

function label(tag: Tag): string {
  return tag.name + ":" + String(tag.score);
}

export function main(): void {
  const literal: Same = { name: "same", score: 3 };
  const tags: Tags = [{ name: "a", score: 1.5 }, literal];
  for (const tag of tags) {
    if (tag !== undefined) {
      console.log(label(tag));
    }
  }
  const parsed = varta.json.parse<Tag>('{"name":"json","note":"n","score":2}');
  console.log(JSON.stringify(parsed));
  console.log(Object.hasOwn(literal, "note"));
}
