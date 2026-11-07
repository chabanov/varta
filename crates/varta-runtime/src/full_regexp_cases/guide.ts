export function main(): void {
  const date = /(?<y>\d{4})-(?<m>\d{2})|(?<m>\d{2})\/(?<y>\d{4})/d;
  const found = date.exec("due 11/2026");
  console.log(found?.groups?.y + " " + found?.groups?.m); // 2026 11
  console.log(String(found?.indices?.groups?.y?.[0])); // 7
  console.log("aa bb cd".replace(/(\w)\1/g, "<$1>")); // <a> <b> cd
  console.log("$42.50 or €7".match(/(?<=\$)\d+(\.\d+)?/)?.[0]); // 42.50
  console.log("camelCaseText".split(/(?=[A-Z])/).join(" ")); // camel Case Text
  console.log("Ab ab AB".replace(/a(?i:b)/g, "x")); // Ab x AB
  console.log("ΑΒΓ abc".match(/\p{Script=Greek}+/u)?.[0]); // ΑΒΓ
  console.log("x1y2".replace(/[\w--\d]/gv, "_")); // _1_2
  console.log(String(/^\p{RGI_Emoji}$/v.test("👩‍💻"))); // true
}
