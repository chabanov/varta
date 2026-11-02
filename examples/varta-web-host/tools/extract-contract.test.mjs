import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync, mkdtempSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";

test("generated admission separates legacy tools from v38 HTTP v2", () => {
  const tools = dirname(fileURLToPath(import.meta.url));
  const root = resolve(tools, "../../..");
  const scratch = mkdtempSync(join(tmpdir(), "varta-contract-"));
  try {
    const output = join(scratch, "contract.json");
    execFileSync(process.execPath, [join(tools, "extract-contract.mjs"), join(root, "docs/embedding.md"), join(root, "docs/architecture.md"), output]);
    const text = readFileSync(output, "utf8");
    assert.equal(text, readFileSync(resolve(tools, "../contract.json"), "utf8"), "regenerate the checked-in contract");
    const contract = JSON.parse(text);
    const profile = (name) => contract.profiles.find((entry) => entry.name === name);
    assert.equal(profile("checked-i32-utf8-v1").toolEntries, true);
    assert.equal(profile("checked-i32-utf8-v1").httpVersion, 1);
    assert.equal(profile("checked-f64-utf16-v37").toolEntries, false);
    assert.equal(profile("checked-f64-utf16-v37").httpVersion, null);
    assert.equal(profile("checked-f64-utf16-v38").toolEntries, false);
    assert.equal(profile("checked-f64-utf16-v38").httpVersion, 2);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
});
