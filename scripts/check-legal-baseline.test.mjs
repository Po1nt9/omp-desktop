import assert from "node:assert/strict";
import test from "node:test";
import { requiredLegalInputs, validateInventory } from "./check-legal-baseline.mjs";

test("tracks non-MIT OMP resources and upstream notices", () => {
  const paths = new Set(requiredLegalInputs().map((item) => item.path));
  assert.ok(paths.has("runtime/oh-my-pi/crates/pi-natives/src/fonts/Silver.LICENSE"));
  assert.ok(paths.has("runtime/oh-my-pi/packages/coding-agent/src/export/html/vendor/highlight.min.js"));
  assert.equal(validateInventory().length, 0);
});
