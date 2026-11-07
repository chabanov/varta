// Bounded exception/rejection reader. Layout numbers are generated from Rust.
// No guest reference is retained after render() returns.
function reasonReader(memory, module, profile, descriptor) {
  const L = registry.values, T = L.tags, S = registry.scheduler;
  const sections = WebAssembly.Module.customSections(module, L.staticSection);
  const pages = sections.length ? new DataView(sections[0]).getUint32(0, true) : 0;
  const staticEnd = L.ownedBase + pages * L.pageBytes;
  const view = () => new DataView(memory().buffer);
  const invalid = () => { throw new Error("noncanonical rejection value"); };
  const range = (at, len) => {
    if (!Number.isSafeInteger(at) || !Number.isSafeInteger(len) || at < 0 || len < 0 || at + len > view().byteLength) invalid();
  };
  const word = (at) => { range(at, 4); return view().getUint32(at, true); };
  const wide = (at) => { range(at, 8); return view().getBigUint64(at, true); };
  const split = (handle) => [Number(BigInt.asUintN(32, handle)), Number(BigInt.asUintN(64, handle) >> 32n)];
  const block = (handle, flag) => {
    const [ptr, len] = split(handle), header = ptr - L.headerBytes;
    if (ptr < L.ownedBase + L.headerBytes || ptr % L.alignment) invalid();
    const size = word(header);
    if (size < L.headerBytes || size % L.alignment || len > size - L.headerBytes) invalid();
    range(header, size);
    // Value18 object blocks may carry integrity-level bits (Object.freeze).
    const ignored = profile.valueAbi === L.integrity.version ? L.integrity.flags : 0;
    if ((word(header + L.flagsOffset) & ~ignored) >>> 0 !== (L.allocated | flag)) invalid();
    return { ptr, len, header, size };
  };
  const cell = (handle) => {
    const b = block(handle, L.unionFlag), kind = word(b.ptr), payload = wide(b.ptr + L.cellPayload);
    if (b.len !== L.cellBytes || word(b.ptr + 4) !== 1 || !profile.supportedKinds.includes(kind) || word(b.header + L.linkOffset) !== Number(L.references.includes(kind))) invalid();
    if ((kind === T.Null || kind === T.Undefined) && payload !== 0n || kind === T.Boolean && payload > 1n) invalid();
    return { kind, payload };
  };
  let truncated, rejection = true, lastCut = false;
  const text = (handle, limit = descriptor.maxStringBytes) => {
    const [ptr, len] = split(handle);
    if (len % 2 || len > limit) invalid();
    range(ptr, len);
    const count = Math.min(len / 2, S.reasonUnits);
    lastCut = len / 2 > count;
    truncated ||= lastCut;
    const units = [];
    for (let i = 0; i < count; i++) units.push(view().getUint16(ptr + 2 * i, true));
    return String.fromCharCode(...units);
  };
  const quote = (value) => JSON.stringify(value);
  const bare = (value) => quote(value).slice(1, -1);
  const error = (handle) => {
    const b = block(handle, L.errorFlag), flags = word(b.ptr), cls = flags >>> L.errorShift;
    const message = wide(b.ptr + L.recordMeta);
    if (b.len !== L.errorBytes || word(b.ptr + 4) !== 1 || word(b.header + L.linkOffset) !== 1 || cls >= L.errorClasses.length || !(flags & L.errorOwn) && message !== 0n) invalid();
    const value = text(message), cut = lastCut;
    return L.errorClasses[cls] + (value ? ": " + quote(value) + (!rejection && cut ? "…" : "") : "");
  };
  const bigint = (handle) => {
    const B = L.bigint, b = block(handle, B.flag);
    if (b.len < B.meta || b.len > B.maxBytes || word(b.header + L.linkOffset) !== 0 || word(b.header + L.headerBytes - 4) !== 0) invalid();
    const sign = word(b.ptr), digits = word(b.ptr + B.digitBytes);
    if (![B.zero, B.positive, B.negative].includes(sign) || B.meta + digits * B.digitBytes !== b.len || (sign === B.zero) !== (digits === 0) || digits && word(b.ptr + b.len - B.digitBytes) === 0) invalid();
    if (!digits) return "0n";
    let out = sign === B.negative ? "-0x" : "0x", count = 0;
    for (let at = b.ptr + b.len - 1; at >= b.ptr + B.meta; at--) {
      const byte = view().getUint8(at);
      if (!count && !byte) continue;
      if (count === L.diagnosticUnits / 2) { out += "…"; break; }
      out += byte.toString(16).padStart(count ? 2 : 1, "0"); count++;
    }
    return out + "n";
  };
  const staticRange = (at, len) => {
    range(at, len);
    if (!(at + len <= L.firstPageEnd || at >= L.ownedBase && at + len <= staticEnd)) invalid();
  };
  const bits = (at, words) => {
    let result = 0n;
    for (let i = 0; i < words; i++) result |= BigInt(word(at + i * 4)) << BigInt(i * 32);
    return result;
  };
  const instance = (handle) => {
    const b = block(handle, L.recordFlag), desc = word(b.ptr), count = word(b.ptr + 4), shape = L.recordShapes[count];
    if (!shape || b.len !== shape.bytes) invalid();
    staticRange(desc, shape.descriptorBytes);
    if (word(desc) !== profile.valueAbi || word(desc + 4) !== count) invalid();
    const mask = (1n << BigInt(count)) - 1n, present = bits(b.ptr + shape.presence, shape.words);
    if (present & ~mask) invalid();
    const names = new Set(), fields = {}, hidden = {};
    let slots = 0n, references = 0n;
    for (let i = 0; i < count; i++) {
      const entry = desc + L.descriptorHeader + i * L.descriptorEntry;
      const key = word(entry), len = word(entry + 4), raw = word(entry + 8), kind = word(entry + 12);
      const slot = raw & ~L.optionalFlag, optional = Boolean(raw & L.optionalFlag);
      if (slot < 0 || slot >= count) invalid();
      const bit = 1n << BigInt(slot);
      if (slots & bit || len > L.maxKeyBytes || !profile.supportedKinds.includes(kind) || optional && ![T.Undefined, T.PrimitiveUnion, T.ReferenceUnion].includes(kind)) invalid();
      staticRange(key, len);
      const name = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(new Uint8Array(memory().buffer, key, len));
      if (names.has(name)) invalid();
      names.add(name); slots |= bit;
      if (L.references.includes(kind)) references |= bit;
      const value = wide(b.ptr + L.recordMeta + slot * L.slotBytes);
      if (!(present & bit) && (!optional || (kind === T.Undefined ? value !== 0n : cell(value).kind !== T.Undefined))) invalid();
      const privateKey = name.startsWith(L.privatePrefix), field = privateKey ? name.slice(L.privatePrefix.length) : name;
      if (field === "name" || field === "message") {
        if (optional || kind !== T.String) invalid();
        const content = text(value);
        (privateKey ? hidden : fields)[field] = { value: content, cut: lastCut };
      }
    }
    if (slots !== mask || word(b.header + L.linkOffset) !== (shape.wide ? 0 : Number(references))) invalid();
    if (shape.wide && bits(desc + L.descriptorHeader + count * L.descriptorEntry, shape.words) !== references) invalid();
    let seen = 0n, active = 0;
    for (let m = present; m; m &= m - 1n) active++;
    for (let i = 0; i < count; i++) {
      const slot = word(b.ptr + shape.order + i * 4);
      if (slot >= Math.max(count, 1)) invalid();
      const bit = 1n << BigInt(slot);
      if (i < active) { if (seen & bit || !(present & bit)) invalid(); seen |= bit; }
      else if (slot !== 0) invalid();
    }
    if (seen !== present) invalid();
    const name = fields.name ?? hidden.name, message = fields.message ?? hidden.message;
    if (name === undefined || message === undefined) invalid();
    const q = quote(message.value) + (!rejection && message.cut ? "…" : "");
    return name.value ? bare(name.value) + (!rejection && name.cut ? "…" : "") + (message.value ? ": " + q : "") : q;
  };
  const number = (payload) => {
    const buffer = new ArrayBuffer(8), data = new DataView(buffer);
    data.setBigUint64(0, payload, true);
    const n = data.getFloat64(0, true);
    if (Object.is(n, -0)) return "-0";
    const text = String(n), match = /^(-?)(\d+)(?:\.(\d+))?e([+-]?\d+)$/.exec(text);
    if (!match) return text;
    const digits = match[2] + (match[3] ?? ""), point = match[2].length + Number(match[4]);
    return match[1] + (point <= 0 ? "0." + "0".repeat(-point) + digits : point >= digits.length ? digits + "0".repeat(point - digits.length) : digits.slice(0, point) + "." + digits.slice(point));
  };
  return (handle, isRejection = true) => {
    rejection = isRejection; truncated = false; lastCut = false;
    const { kind, payload } = cell(BigInt.asUintN(64, handle));
    let result;
    if (kind === T.Null) result = "null";
    else if (kind === T.Undefined) result = "undefined";
    else if (kind === T.Boolean) result = String(payload !== 0n);
    else if (kind === T.Float64) result = number(payload);
    else if (kind === T.String) { result = quote(text(payload)); if (!rejection && lastCut) result += "…"; }
    else if (kind === T.Error) result = error(payload);
    else if (kind === T.BigInt) result = bigint(payload);
    else if (kind === T.Record) result = instance(payload);
    else invalid();
    const bytes = new TextEncoder().encode(result);
    if (rejection && bytes.length > S.reasonBytes) {
      let end = S.reasonBytes;
      while ((bytes[end] & 0xc0) === 0x80) end--;
      result = new TextDecoder().decode(bytes.subarray(0, end)); truncated = true;
    }
    return result + (rejection && truncated ? S.truncated : "");
  };
}
