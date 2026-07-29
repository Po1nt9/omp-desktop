import { describe, expect, it } from "vitest";
import { checkCatalog } from "./check-i18n-completeness.mjs";

describe("check-i18n-completeness", () => {
  it("passes for a clean catalog", () => {
    const messages = {
      en: { "a.ok": "OK", "b.title": "Title" },
      zh: { "a.ok": "好的", "b.title": "标题" },
    };
    expect(checkCatalog(messages)).toEqual([]);
  });

  it("flags a missing key", () => {
    const messages = {
      en: { "a.ok": "OK", "b.title": "Title" },
      zh: { "a.ok": "好的" },
    };
    const v = checkCatalog(messages);
    expect(v).toContain('zh: missing key "b.title"');
  });

  it("flags an empty value", () => {
    const messages = {
      en: { "a.ok": "OK" },
      zh: { "a.ok": "   " },
    };
    const v = checkCatalog(messages);
    expect(v.some((s) => s.startsWith("zh.a.ok: value is empty"))).toBe(true);
  });

  it("flags an extra key not in en", () => {
    const messages = {
      en: { "a.ok": "OK" },
      zh: { "a.ok": "好的", "b.extra": "额外" },
    };
    const v = checkCatalog(messages);
    expect(v).toContain('zh: extra key "b.extra" not present in en');
  });

  it("fails when en is empty", () => {
    const messages = { en: {}, zh: {} };
    const v = checkCatalog(messages);
    expect(v.some((s) => s.includes("source locale is empty"))).toBe(true);
  });
});
