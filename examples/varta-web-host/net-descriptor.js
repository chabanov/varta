// Request descriptors of `net_request_start` / `net_request_start_v2`
// (embedding.md "Request descriptor"), parsed from the generated tables and
// checked against the contract's own accepted and rejected examples.

const SEPARATOR = "\n";
const CODE_CELL = /^`(.*)`$/;
const DECIMAL = /^\+?[0-9]+$/;
const BYTE_UNIT_MAX = 0xff;

export class DescriptorError extends Error {}

/** The descriptor facts of one contract: first line per import and the body flags each admits. */
export function descriptorRules(net) {
  const rows = Object.entries(net.descriptor);
  const cell = (text) => CODE_CELL.exec(text)[1];
  const versions = {};
  const flags = {};
  let maxHeaders = null;
  for (const [item, value] of rows) {
    const first = /^First line of `([a-z0-9_]+)`$/.exec(item);
    const flag = /^Body flag: (.*) \(([^)]*)\)$/.exec(item);
    if (first) versions[first[1]] = cell(value);
    else if (flag) {
      const kind = /^No body/.test(flag[1]) ? "none" : /^Text body/.test(flag[1]) ? "text" : /^Byte body/.test(flag[1]) ? "bytes" : null;
      if (!kind) throw new Error(`unknown body flag ${flag[1]}`);
      for (const name of [...flag[2].matchAll(/`([a-z0-9_]+)`/g)].map((m) => m[1])) (flags[name] ??= {})[cell(value)] = kind;
    } else if (item === "Largest header count") maxHeaders = Number(value);
  }
  return { versions, flags, maxHeaders };
}

/**
 * Splits a descriptor (already decoded, lone surrogates as U+FFFD) into its fields.
 * Throws DescriptorError for anything outside the grammar of `importName`.
 */
export function parseDescriptor(rules, importName, text) {
  const version = rules.versions[importName];
  const lines = [];
  let at = 0;
  const line = () => {
    const end = text.indexOf(SEPARATOR, at);
    if (end < 0) throw new DescriptorError("missing separator");
    const value = text.slice(at, end);
    at = end + SEPARATOR.length;
    lines.push(value);
    return value;
  };
  if (line() !== version) throw new DescriptorError("wrong version line");
  const method = line();
  const url = line();
  const countText = line();
  if (!DECIMAL.test(countText)) throw new DescriptorError("header count not a decimal integer");
  const count = Number(countText);
  if (count > rules.maxHeaders) throw new DescriptorError("header count above the limit");
  const headers = [];
  for (let i = 0; i < count; i += 1) headers.push([line(), line()]);
  const flag = rules.flags[importName][line()];
  if (!flag) throw new DescriptorError("unknown body flag");
  const rest = text.slice(at);
  if (flag === "none") {
    if (rest !== "") throw new DescriptorError("text after the no-body flag");
    return { method, url, headers, body: null };
  }
  if (flag === "text") return { method, url, headers, body: new TextEncoder().encode(rest) };
  const bytes = new Uint8Array(rest.length);
  for (let i = 0; i < rest.length; i += 1) {
    const unit = rest.charCodeAt(i);
    if (unit > BYTE_UNIT_MAX) throw new DescriptorError("byte body unit above 0xFF");
    bytes[i] = unit;
  }
  return { method, url, headers, body: bytes };
}

/** Runs the contract's examples through this parser once; a disagreement stops the host. */
export function checkDescriptorExamples(rules, net) {
  for (const example of net.examples) {
    try {
      parseDescriptor(rules, example.import, example.text);
    } catch (error) {
      throw new Error(`descriptor parser rejects the contract's accepted example ${JSON.stringify(example.text)}: ${error.message}`);
    }
  }
  for (const rejection of net.rejections) {
    let accepted = false;
    try {
      parseDescriptor(rules, rejection.import, rejection.text);
      accepted = true;
    } catch (error) {
      if (!(error instanceof DescriptorError)) throw error;
    }
    if (accepted) throw new Error(`descriptor parser accepts the contract's rejected example (${rejection.why})`);
  }
}
