import assert from "node:assert/strict";
import test from "node:test";
import { checkCatalog, extractPlaceholders } from "./check-i18n-completeness.mjs";

test("passes for a clean catalog", () => {
  const messages = {
    en: { "a.ok": "OK", "b.title": "Title" },
    zh: { "a.ok": "好的", "b.title": "标题" },
  };
  assert.deepEqual(checkCatalog(messages), []);
});

test("flags a missing key", () => {
  const messages = {
    en: { "a.ok": "OK", "b.title": "Title" },
    zh: { "a.ok": "好的" },
  };
  const v = checkCatalog(messages);
  assert.ok(v.includes('zh: missing key "b.title"'));
});

test("flags an empty value", () => {
  const messages = {
    en: { "a.ok": "OK" },
    zh: { "a.ok": "   " },
  };
  const v = checkCatalog(messages);
  assert.ok(v.some((s) => s.startsWith("zh.a.ok: value is empty")));
});

test("flags an extra key not in en", () => {
  const messages = {
    en: { "a.ok": "OK" },
    zh: { "a.ok": "好的", "b.extra": "额外" },
  };
  const v = checkCatalog(messages);
  assert.ok(v.includes('zh: extra key "b.extra" not present in en'));
});

test("fails when en is empty", () => {
  const messages = { en: {}, zh: {} };
  const v = checkCatalog(messages);
  assert.ok(v.some((s) => s.includes("source locale is empty")));
});

test("extractPlaceholders returns sorted unique names", () => {
  assert.deepEqual(extractPlaceholders("{b} and {a} and {b}"), ["a", "b"]);
  assert.deepEqual(extractPlaceholders("no placeholders"), []);
});

test("passes when placeholders match across locales", () => {
  const messages = {
    en: { "a.greet": "Hello {name}, you have {count} item(s)" },
    zh: { "a.greet": "你好 {name}，你有 {count} 个项目" },
  };
  assert.deepEqual(checkCatalog(messages), []);
});

test("flags a placeholder missing from a translation", () => {
  const messages = {
    en: { "a.greet": "Hello {name}, you have {count} item(s)" },
    zh: { "a.greet": "你好 {name}" },
  };
  const v = checkCatalog(messages);
  assert.ok(v.includes("zh.a.greet: placeholder mismatch (missing: count)"));
});

test("flags an extra placeholder in a translation", () => {
  const messages = {
    en: { "a.greet": "Hello {name}" },
    zh: { "a.greet": "你好 {name}，共 {count} 项" },
  };
  const v = checkCatalog(messages);
  assert.ok(v.includes("zh.a.greet: placeholder mismatch (extra: count)"));
});

test("flags missing and extra placeholders together", () => {
  const messages = {
    en: { "a.greet": "Hello {name}" },
    zh: { "a.greet": "你好 {user}" },
  };
  const v = checkCatalog(messages);
  assert.ok(v.includes("zh.a.greet: placeholder mismatch (missing: name; extra: user)"));
});

test("ignores placeholder-free keys", () => {
  const messages = {
    en: { "a.ok": "OK" },
    zh: { "a.ok": "好的" },
  };
  assert.deepEqual(checkCatalog(messages), []);
});
