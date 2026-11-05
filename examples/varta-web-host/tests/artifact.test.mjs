// Artifacts with packaged fonts are refused before validation (embedding.md:
// a view-rendering host loads the resource section's faces or refuses).
import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile, readFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { refuseUnsupportedArtifact, RESOURCE_CONTAINER_FORMAT } from "../artifact.js";
import { createHost, ValidationError } from "../host.js";

/** The smallest portable module: the magic and version 1. */
const MODULE = Uint8Array.from([0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);
const MANIFEST = new TextEncoder().encode('{"capabilities":[]}');
const NATIVE_PREFIX = new TextEncoder().encode("native launcher bytes");
const RESOURCE_SECTION = new TextEncoder().encode("VARTARS\0 section");
const LEGACY_CONTAINER_FORMAT = 1;

// The VARTA3 footer fields (architecture.md "Artifact format and
// validation") and the u64 resource length that format 2 puts before it.
const FOOTER = { bytes: 32, wasmLength: 0, manifestLength: 8, format: 16, magic: 24 };
const RESOURCE_LENGTH_BYTES = 8;

/** A VARTA3 container with the fixed EOF footer of `format`. */
function container(format) {
  const length = new DataView(new ArrayBuffer(RESOURCE_LENGTH_BYTES));
  length.setBigUint64(0, BigInt(RESOURCE_SECTION.length), true);
  const resources = format === RESOURCE_CONTAINER_FORMAT ? [...RESOURCE_SECTION, ...new Uint8Array(length.buffer)] : [];
  const footer = new DataView(new ArrayBuffer(FOOTER.bytes));
  footer.setBigUint64(FOOTER.wasmLength, BigInt(MODULE.length), true);
  footer.setBigUint64(FOOTER.manifestLength, BigInt(MANIFEST.length), true);
  footer.setUint32(FOOTER.format, format, true);
  new Uint8Array(footer.buffer).set(new TextEncoder().encode("VARTA3\0\0"), FOOTER.magic);
  return Uint8Array.from([...NATIVE_PREFIX, ...MODULE, ...MANIFEST, ...resources, ...new Uint8Array(footer.buffer)]);
}

test("portable Wasm without resources passes; resources and containers are refused by name", () => {
  assert.doesNotThrow(() => refuseUnsupportedArtifact(MODULE));
  assert.doesNotThrow(() => refuseUnsupportedArtifact(MODULE.buffer));
  assert.throws(() => refuseUnsupportedArtifact(MODULE, { resources: RESOURCE_SECTION }), (error) => error instanceof ValidationError && /resource section .*does not load packaged fonts/.test(error.message));
  assert.throws(() => refuseUnsupportedArtifact(container(RESOURCE_CONTAINER_FORMAT)), (error) => error instanceof ValidationError && /container format 2 carries a resource section/.test(error.message));
  assert.throws(() => refuseUnsupportedArtifact(container(LEGACY_CONTAINER_FORMAT)), (error) => error instanceof ValidationError && /container format 1: .*reads only portable Wasm/.test(error.message));
  // Bytes that are neither are left to module validation.
  assert.doesNotThrow(() => refuseUnsupportedArtifact(NATIVE_PREFIX));
});

test("every host entry refuses an artifact with resources before validating it", async () => {
  const contract = JSON.parse(await readFile(new URL("../contract.json", import.meta.url), "utf8"));
  const host = createHost(contract);
  const resources = { resources: RESOURCE_SECTION };
  assert.throws(() => host.validate(MODULE, resources), /does not load packaged fonts/);
  await assert.rejects(host.run(MODULE, resources), /does not load packaged fonts/);
  assert.throws(() => host.start(MODULE, resources), /does not load packaged fonts/);
  await assert.rejects(host.run(container(RESOURCE_CONTAINER_FORMAT)), /container format 2 carries a resource section/);
  const invoked = await host.invoke(MODULE, "tool", "{}", resources);
  assert.match(JSON.stringify(invoked), /invalid_artifact.*does not load packaged fonts/);
});

test("the CLI refuses a bare artifact with a resources sidecar", async () => {
  const directory = await mkdtemp(join(tmpdir(), "varta-web-host-resources-"));
  try {
    const wasm = join(directory, "app.wasm");
    await writeFile(wasm, MODULE);
    await writeFile(join(directory, "app.resources"), RESOURCE_SECTION);
    const cli = fileURLToPath(new URL("../cli.mjs", import.meta.url));
    const result = spawnSync(process.execPath, [cli, "validate", wasm], { encoding: "utf8" });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /resource section .*does not load packaged fonts/);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
