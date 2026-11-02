import { test } from "node:test";
import assert from "node:assert/strict";
import { createViewPolicy } from "./view-policy.mjs";
import { registry } from "../../../varta-common/browser/registry.mjs";
const C = registry.view.capabilities;
const make = (patterns, ceiling, required) => createViewPolicy({ capabilities: [{ id: C.openUrl, patterns, required }, { id: C.clipboard, enabled: true, required }] }, ceiling);
test("view links require exact origin/resource intersection and snapshots", () => {
  const patterns = ["https://api.test/public/**"], host = { [C.openUrl]: { patterns: ["https://api.test/public/item"] } };
  const policy = make(patterns, host); patterns[0] = "https://evil.test/**"; host[C.openUrl].patterns[0] = "https://evil.test/**";
  assert.equal(policy.linkRequested("https://api.test/public/item"), true); assert.equal(policy.openAllowed("https://api.test/public/item"), true);
  for (const url of ["https://api.test/public/other", "https://api.test/public/item?", "https://api.test/public/item#", "http://api.test/public/item", "https://api.test:444/public/item", "https://user:pw@api.test/public/item", "https://evil.test/public/item"]) assert.equal(policy.openAllowed(url), false, url);
});
test("wide links need a prompt; absent/optional/empty permissions never create authority", () => {
  const wide = make([C.anyHttps]); assert.equal(wide.linkRequested("https://api.test/x"), true); assert.equal(wide.openAllowed("https://api.test/x"), false);
  assert.equal(make(["https://api.test/**"], {}).openAllowed("https://api.test/x"), false);
  assert.equal(make(["https://api.test/**"], undefined, false).openAllowed("https://api.test/x"), false);
  assert.equal(make(["https://api.test/**"], { [C.openUrl]: { patterns: [] } }).openAllowed("https://api.test/x"), false);
  const absent = createViewPolicy({ capabilities: [] }, { [C.openUrl]: { patterns: ["https://api.test/**"] }, [C.clipboard]: true });
  assert.equal(absent.linkRequested("https://api.test/x"), false); assert.equal(absent.clipboardAllowed(), false);
});
test("clipboard uses its own exact boolean request and ceiling", () => {
  assert.equal(make([], undefined).clipboardAllowed(), true);
  assert.equal(make([], { [C.clipboard]: true }).clipboardAllowed(), true);
  for (const ceiling of [{}, { "time.timer": true }, { [C.clipboard]: false }]) assert.equal(make([], ceiling).clipboardAllowed(), false);
  assert.equal(make([], undefined, false).clipboardAllowed(), false);
});
test('per-activation prompts arise from either side of the requested/host intersection',()=>{
  const url='https://api.test/path?#',explicit='https://api.test/**';
  for(const[request,host,expected]of [[[C.anyHttps],undefined,'prompt'],[[explicit],{[C.openUrl]:{patterns:[C.anyHttps]}},'prompt'],[[C.anyHttps],{[C.openUrl]:{patterns:[explicit]}},'prompt'],[[C.anyHttps,explicit],{[C.openUrl]:{patterns:[explicit]}},'granted'],[[C.anyHttps],{},'denied']])assert.equal(make(request,host).openDecision(url),expected);
  assert.equal(make([C.anyHttps]).openDecision('http://api.test/x'),'denied');assert.equal(make([C.anyHttps]).openDecision('https://user:pw@api.test/x'),'denied');
  assert.equal(make([C.anyHttps],undefined,false).openDecision(url),'denied');
});
