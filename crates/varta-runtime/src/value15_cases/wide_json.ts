// Checked JSON parsing of a 40-field record with optional fields beyond slot 32.
interface Wide {
  w00: string;
  w01: number;
  w02: string;
  w03: number;
  w04: string;
  w05: number;
  w06: string;
  w07: number;
  w08: string;
  w09: number;
  w10: string;
  w11: number;
  w12: string;
  w13: number;
  w14: string;
  w15: number;
  w16: string;
  w17: number;
  w18: string;
  w19: number;
  w20: string;
  w21: number;
  w22: string;
  w23: number;
  w24: string;
  w25: number;
  w26: string;
  w27: number;
  w28: string;
  w29: number;
  w30: string;
  w31: number;
  w32: string;
  w33?: number;
  w34: string;
  w35: number;
  w36: string;
  w37: number;
  w38?: string;
  w39: number;
}
export function main(): void {
  const parsed = varta.json.parse<Wide>("{\"w00\":\"v0\",\"w01\":1,\"w02\":\"v2\",\"w03\":3,\"w04\":\"v4\",\"w05\":5,\"w06\":\"v6\",\"w07\":7,\"w08\":\"v8\",\"w09\":9,\"w10\":\"v10\",\"w11\":11,\"w12\":\"v12\",\"w13\":13,\"w14\":\"v14\",\"w15\":15,\"w16\":\"v16\",\"w17\":17,\"w18\":\"v18\",\"w19\":19,\"w20\":\"v20\",\"w21\":21,\"w22\":\"v22\",\"w23\":23,\"w24\":\"v24\",\"w25\":25,\"w26\":\"v26\",\"w27\":27,\"w28\":\"v28\",\"w29\":29,\"w30\":\"v30\",\"w31\":31,\"w32\":\"v32\",\"w33\":33,\"w34\":\"v34\",\"w35\":35,\"w36\":\"v36\",\"w37\":37,\"w39\":39}");
  console.log(String(parsed.w39) + " " + String(parsed.w33) + " " + String(parsed.w38 === undefined));
  console.log(String(Object.keys(parsed).length) + " " + String(Object.hasOwn(parsed, "w38")));
  parsed.w38 = "set";
  const text = JSON.stringify(parsed);
  console.log(text.substring(text.length - 24));
  const again = varta.json.parse<Wide>(text);
  console.log(String(again.w38 === "set") + " " + String(Object.keys(again).length));
}
