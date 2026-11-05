// `"key" in` a value held by `unknown` (v38 `byte_values`), for every
// string-keyed member of the prototype chains of Uint8Array, ArrayBuffer,
// Blob, File and Object in Node 24.20.0 and further own, index and numeric
// keys. Generated once from Node's `Object.getOwnPropertyNames` along each
// chain; each line prints one value's answers as 0/1 in the order of `KEYS`.
const KEYS = ["constructor","BYTES_PER_ELEMENT","buffer","byteLength","byteOffset","length","entries","keys","values","at","copyWithin","every","fill","filter","find","findIndex","findLast","findLastIndex","forEach","includes","indexOf","join","lastIndexOf","map","reverse","reduce","reduceRight","set","slice","some","sort","subarray","toReversed","toSorted","with","toLocaleString","toString","__defineGetter__","__defineSetter__","hasOwnProperty","__lookupGetter__","__lookupSetter__","isPrototypeOf","propertyIsEnumerable","valueOf","__proto__","maxByteLength","resizable","resize","transfer","transferToFixedLength","detached","type","size","arrayBuffer","text","bytes","stream","textStream","name","lastModified","data","filename","0","1","2","3","-0","-1","1.5","01","NaN","Infinity","4294967294","4294967295","a","b"];

function group0(v: unknown): string {
  let answers = '';
  answers += 'constructor' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'BYTES_PER_ELEMENT' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'buffer' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'byteLength' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'byteOffset' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'length' in (v as Record<string, unknown>) ? '1' : '0';
  return answers;
}

function group1(v: unknown): string {
  let answers = '';
  answers += 'entries' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'keys' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'values' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'at' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'copyWithin' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'every' in (v as Record<string, unknown>) ? '1' : '0';
  return answers;
}

function group2(v: unknown): string {
  let answers = '';
  answers += 'fill' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'filter' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'find' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'findIndex' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'findLast' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'findLastIndex' in (v as Record<string, unknown>) ? '1' : '0';
  return answers;
}

function group3(v: unknown): string {
  let answers = '';
  answers += 'forEach' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'includes' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'indexOf' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'join' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'lastIndexOf' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'map' in (v as Record<string, unknown>) ? '1' : '0';
  return answers;
}

function group4(v: unknown): string {
  let answers = '';
  answers += 'reverse' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'reduce' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'reduceRight' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'set' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'slice' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'some' in (v as Record<string, unknown>) ? '1' : '0';
  return answers;
}

function group5(v: unknown): string {
  let answers = '';
  answers += 'sort' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'subarray' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'toReversed' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'toSorted' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'with' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'toLocaleString' in (v as Record<string, unknown>) ? '1' : '0';
  return answers;
}

function group6(v: unknown): string {
  let answers = '';
  answers += 'toString' in (v as Record<string, unknown>) ? '1' : '0';
  answers += '__defineGetter__' in (v as Record<string, unknown>) ? '1' : '0';
  answers += '__defineSetter__' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'hasOwnProperty' in (v as Record<string, unknown>) ? '1' : '0';
  answers += '__lookupGetter__' in (v as Record<string, unknown>) ? '1' : '0';
  answers += '__lookupSetter__' in (v as Record<string, unknown>) ? '1' : '0';
  return answers;
}

function group7(v: unknown): string {
  let answers = '';
  answers += 'isPrototypeOf' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'propertyIsEnumerable' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'valueOf' in (v as Record<string, unknown>) ? '1' : '0';
  answers += '__proto__' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'maxByteLength' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'resizable' in (v as Record<string, unknown>) ? '1' : '0';
  return answers;
}

function group8(v: unknown): string {
  let answers = '';
  answers += 'resize' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'transfer' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'transferToFixedLength' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'detached' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'type' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'size' in (v as Record<string, unknown>) ? '1' : '0';
  return answers;
}

function group9(v: unknown): string {
  let answers = '';
  answers += 'arrayBuffer' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'text' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'bytes' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'stream' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'textStream' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'name' in (v as Record<string, unknown>) ? '1' : '0';
  return answers;
}

function group10(v: unknown): string {
  let answers = '';
  answers += 'lastModified' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'data' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'filename' in (v as Record<string, unknown>) ? '1' : '0';
  answers += '0' in (v as Record<string, unknown>) ? '1' : '0';
  answers += '1' in (v as Record<string, unknown>) ? '1' : '0';
  answers += '2' in (v as Record<string, unknown>) ? '1' : '0';
  return answers;
}

function group11(v: unknown): string {
  let answers = '';
  answers += '3' in (v as Record<string, unknown>) ? '1' : '0';
  answers += '-0' in (v as Record<string, unknown>) ? '1' : '0';
  answers += '-1' in (v as Record<string, unknown>) ? '1' : '0';
  answers += '1.5' in (v as Record<string, unknown>) ? '1' : '0';
  answers += '01' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'NaN' in (v as Record<string, unknown>) ? '1' : '0';
  return answers;
}

function group12(v: unknown): string {
  let answers = '';
  answers += 'Infinity' in (v as Record<string, unknown>) ? '1' : '0';
  answers += '4294967294' in (v as Record<string, unknown>) ? '1' : '0';
  answers += '4294967295' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'a' in (v as Record<string, unknown>) ? '1' : '0';
  answers += 'b' in (v as Record<string, unknown>) ? '1' : '0';
  return answers;
}

function asserted(v: unknown): string {
  return group0(v) + group1(v) + group2(v) + group3(v) + group4(v) + group5(v) + group6(v) + group7(v) + group8(v) + group9(v) + group10(v) + group11(v) + group12(v);
}

// The bare form of an `object` fact.
function narrowed(v: unknown): string {
  if (typeof v === 'object' && v !== null) {
    return `${String('data' in v)} ${String('0' in v)} ${String('length' in v)} ${String('size' in v)} ${String('toString' in v)}`;
  }
  return 'primitive';
}

interface Part {
  data: Uint8Array;
  filename?: string;
}

export function main(): void {
  console.log(String(KEYS.length));
  const part: Part = { data: new Uint8Array([1]) };
  const values: unknown[] = [
    new Uint8Array([7, 8, 9]),
    new Uint8Array(0),
    new ArrayBuffer(2),
    new Blob(['x']),
    new File(['x'], 'a.txt'),
    part,
    JSON.parse('{"data":1,"0":2}'),
  ];
  for (const v of values) {
    console.log(asserted(v));
    console.log(narrowed(v));
  }
}
