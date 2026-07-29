import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { checkRepository, scanText } from "./check-brand-policy.mjs";

const read = (name) => fs.readFileSync(new URL(`../testdata/brand-policy/${name}`, import.meta.url), "utf8");

test("allows structured runtime Provider and model identities", () => {
  assert.deepEqual(scanText("testdata/brand-policy/allowed/provider-xai.json", read("allowed/provider-xai.json")), []);
  assert.deepEqual(scanText("testdata/brand-policy/allowed/model-grok.json", read("allowed/model-grok.json")), []);
});

test("rejects product branding, direct xAI auth, private methods, and lowercase user-facing OMP", () => {
  assert.deepEqual(scanText("testdata/brand-policy/denied/app-title-grok.json", read("denied/app-title-grok.json")).map(({ rule }) => rule), ["grok-product-brand"]);
  assert.deepEqual(scanText("testdata/brand-policy/denied/direct-auth-xai.json", read("denied/direct-auth-xai.json")).map(({ rule }) => rule), ["desktop-direct-xai"]);
  assert.deepEqual(scanText("testdata/brand-policy/denied/private-method-xai.json", read("denied/private-method-xai.json")).map(({ rule }) => rule), ["private-xai-method"]);
  assert.deepEqual(scanText("src/i18n/lowercase-brand.json", read("denied/lowercase-brand.json")).map(({ rule }) => rule), ["lowercase-user-visible-omp"]);
});

test("rejects every SuperGrok brand form", () => {
  assert.equal(scanText("testdata/brand-policy/denied/supergrok-brand.json", read("denied/supergrok-brand.json")).length, 3);
});

test("rejects every legacy identifier, runtime environment variable, and runtime path fixture", () => {
  assert.equal(scanText("testdata/brand-policy/denied/legacy-identifiers.json", read("denied/legacy-identifiers.json")).length, 4);
  assert.equal(scanText("testdata/brand-policy/denied/legacy-runtime-env.json", read("denied/legacy-runtime-env.json")).length, 7);
  assert.equal(scanText("testdata/brand-policy/denied/legacy-runtime-path.json", read("denied/legacy-runtime-path.json")).length, 3);
});

test("applies structured allowances only at their exact path and JSON field", () => {
  const provider = read("allowed/provider-xai.json");
  assert.ok(scanText("testdata/brand-policy/allowed/provider-copy.json", provider).some(({ rule }) => rule === "desktop-direct-xai"));
  assert.ok(
    scanText(
      "testdata/brand-policy/allowed/provider-xai.json",
      '{"provider":{"id":"xai","name":"xAI","desktopAuthEndpoint":"https://api.x.ai","authMethods":["xAI OAuth"]}}',
    ).some(({ rule }) => rule === "desktop-direct-xai"),
  );

  const violations = scanText(
    "testdata/brand-policy/allowed/provider-xai.json",
    '{"provider":{"id":"xai","name":"xAI","endpoint":"https://api.x.ai","authMethods":["xAI OAuth"]},"productName":"Grok Desktop"}',
  );
  assert.deepEqual(violations.map(({ rule }) => rule), ["grok-product-brand"]);
});

test("normalizes repository paths and does not leak regular-expression state", () => {
  const provider = read("allowed/provider-xai.json");
  assert.deepEqual(scanText(".\\testdata\\brand-policy\\allowed\\provider-xai.json", provider), []);

  const productText = read("denied/app-title-grok.json");
  assert.deepEqual(
    scanText("src/product.json", productText),
    scanText("src/product.json", productText),
  );

  const text = read("denied/lowercase-brand.json");
  const first = scanText("src\\i18n\\lowercase-brand.json", text);
  const second = scanText("./src/i18n/lowercase-brand.json", text);
  assert.deepEqual(second, first);
  assert.deepEqual(first.map(({ rule }) => rule), ["lowercase-user-visible-omp"]);
});

test("does not exempt historical documentation or similarly named scanner files", () => {
  assert.ok(scanText("docs/upstream-history/grok-app/README.md", "Grok Desktop").length > 0);

  const policy = fs.readFileSync(new URL("./brand-policy.mjs", import.meta.url), "utf8");
  assert.deepEqual(scanText("scripts/brand-policy.mjs", policy), []);
  assert.ok(scanText("scripts/brand-policy-copy.mjs", policy).length > 0);
});

test("current README files describe OMP Desktop and fail-closed behavior", () => {
  const readme = fs.readFileSync(new URL("../README.md", import.meta.url), "utf8");
  assert.ok(readme.includes("OMP Desktop"), "README must mention OMP Desktop");
  assert.ok(
    readme.includes("runtime_unavailable") || readme.includes("unavailable until the versioned OMP integration lands"),
    "README must describe fail-closed behavior",
  );
  assert.ok(
    !/run Agent prompts|configure Providers|install a Grok CLI/.test(readme),
    "README must not advertise removed capabilities",
  );
});

test("repository scan skips exact denied fixtures, binary files, and submodules", (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "brand-policy-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  execFileSync("git", ["init", "-q", root]);

  fs.mkdirSync(path.join(root, "src"), { recursive: true });
  fs.writeFileSync(path.join(root, "src", "violation.json"), '{"productName":"Grok Desktop"}');
  fs.writeFileSync(path.join(root, "src", "binary.json"), Buffer.from([0, 71, 114, 111, 107, 32, 68, 101, 115, 107, 116, 111, 112]));
  fs.mkdirSync(path.join(root, "testdata", "brand-policy", "denied"), { recursive: true });
  fs.writeFileSync(path.join(root, "testdata", "brand-policy", "denied", "app-title-grok.json"), '{"productName":"Grok Desktop"}');
  fs.writeFileSync(path.join(root, "testdata", "brand-policy", "denied", "app-title-grok-copy.json"), '{"productName":"Grok Desktop"}');
  execFileSync("git", ["-C", root, "add", "src", "testdata"]);

  const object = execFileSync("git", ["-C", root, "hash-object", "-w", "--stdin"], { input: "submodule", encoding: "utf8" }).trim();
  execFileSync("git", ["-C", root, "update-index", "--add", "--cacheinfo", `160000,${object},vendor/plugin.json`]);

  assert.deepEqual(checkRepository(root).map(({ file, rule }) => ({ file, rule })), [
    { file: "src/violation.json", rule: "grok-product-brand" },
    { file: "testdata/brand-policy/denied/app-title-grok-copy.json", rule: "grok-product-brand" },
  ]);
});
