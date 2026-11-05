// Artifacts this prototype refuses before validation. It reads only portable
// Wasm and loads no fonts, while embedding.md requires a host that renders
// views to load an artifact's resource section (its packaged fonts) or refuse
// the artifact, never to substitute its own fonts. So an artifact that ships
// one is refused: a bare form with its `*.resources` sidecar (the embedder
// passes the sidecar as `options.resources`) and a VARTA3 container of format
// 2. Other VARTA3 containers are refused too, with a message naming the
// format, since only the portable Wasm is read here.
import { ValidationError } from "./errors.js";
import { WASM_MAGIC } from "./wasm-binary.js";

// The fixed VARTA3 footer at EOF (architecture.md "Artifact format and
// validation"): Wasm length (u64), manifest length (u64), format version
// (u32), reserved flags (u32), then the magic. Read at its fixed position,
// never by scanning the payload.
const LENGTH_BYTES = 8;
const WORD_BYTES = 4;
const VERSION_AT = 2 * LENGTH_BYTES;
const MAGIC_AT = VERSION_AT + 2 * WORD_BYTES;
const CONTAINER_MAGIC = new TextEncoder().encode("VARTA3\0\0");
const FOOTER_BYTES = MAGIC_AT + CONTAINER_MAGIC.length;
/** The container format that adds the resource section. */
export const RESOURCE_CONTAINER_FORMAT = 2;

const startsWith = (bytes, prefix) => bytes.length >= prefix.length && prefix.every((byte, index) => bytes[index] === byte);

/** The container format version from a VARTA3 footer, or null for bytes that are not a container. */
function containerFormat(input) {
  const bytes = input instanceof Uint8Array ? input : new Uint8Array(input);
  if (startsWith(bytes, WASM_MAGIC) || bytes.length < FOOTER_BYTES) return null;
  const footer = bytes.subarray(bytes.length - FOOTER_BYTES);
  if (!CONTAINER_MAGIC.every((byte, index) => footer[MAGIC_AT + index] === byte)) return null;
  return new DataView(footer.buffer, footer.byteOffset, FOOTER_BYTES).getUint32(VERSION_AT, true);
}

/** Throw a ValidationError for an artifact this host cannot honour. */
export function refuseUnsupportedArtifact(bytes, { resources = null } = {}) {
  if (resources != null) {
    throw new ValidationError("artifact has a resource section (its *.resources sidecar: the application's packaged fonts); this prototype host does not load packaged fonts and refuses the artifact rather than substitute its own");
  }
  const format = containerFormat(bytes);
  if (format === RESOURCE_CONTAINER_FORMAT) {
    throw new ValidationError(`VARTA3 container format ${format} carries a resource section (the application's packaged fonts); this prototype host does not load packaged fonts and refuses the artifact rather than substitute its own`);
  }
  if (format !== null) throw new ValidationError(`VARTA3 container format ${format}: this prototype host reads only portable Wasm`);
}
