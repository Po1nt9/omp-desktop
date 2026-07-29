import { describe, expect, it } from "vitest";
import {
  createT,
  messages,
  resolveLocale,
  t,
  type MessageKey,
} from "./index";

describe("i18n catalog", () => {
  it("en and zh share the same keys", () => {
    const enKeys = Object.keys(messages.en).sort();
    const zhKeys = Object.keys(messages.zh).sort();
    expect(zhKeys).toEqual(enKeys);
  });

  it("zh-TW shares the same keys as en", () => {
    const enKeys = Object.keys(messages.en).sort();
    const twKeys = Object.keys(messages["zh-TW"]).sort();
    expect(twKeys).toEqual(enKeys);
  });

  it("interpolates variables", () => {
    expect(t("en", "project.trustFirst", { name: "Demo" })).toContain("Demo");
    expect(t("zh", "project.trustFirst", { name: "演示" })).toContain("演示");
  });

  it("createT binds locale (English is the product default)", () => {
    const tr = createT("en");
    expect(tr("sidebar.settings")).toBe("Settings");
    const zh = createT("zh");
    expect(zh("sidebar.settings")).toBe("设置");
  });

  it("every value is a non-empty string", () => {
    for (const loc of ["en", "zh", "zh-TW"] as const) {
      for (const [k, v] of Object.entries(messages[loc])) {
        expect(v.trim().length, `${loc}.${k}`).toBeGreaterThan(0);
      }
    }
  });

  it("type surface accepts known keys only", () => {
    const key: MessageKey = "composer.send";
    expect(t("en", key)).toBeTruthy();
  });
});

describe("resolveLocale", () => {
  it("keeps canonical ids unchanged", () => {
    expect(resolveLocale("zh-TW")).toBe("zh-TW");
    expect(resolveLocale("zh")).toBe("zh");
    expect(resolveLocale("en")).toBe("en");
  });

  it("accepts case/alias variants of Traditional Chinese", () => {
    expect(resolveLocale("zh-tw")).toBe("zh-TW");
    expect(resolveLocale("zh_TW")).toBe("zh-TW");
    expect(resolveLocale("zh-Hant")).toBe("zh-TW");
    expect(resolveLocale(" ZH-HANT ")).toBe("zh-TW");
  });

  it("accepts case/alias variants of Simplified Chinese and English", () => {
    expect(resolveLocale("ZH")).toBe("zh");
    expect(resolveLocale("zh-CN")).toBe("zh");
    expect(resolveLocale("EN-US")).toBe("en");
  });

  it("falls back to the product default for unknown or empty ids", () => {
    expect(resolveLocale("fr")).toBe("en");
    expect(resolveLocale("")).toBe("en");
    expect(resolveLocale(undefined)).toBe("en");
    expect(resolveLocale(null)).toBe("en");
  });
});

describe("v1 protocol error keys", () => {
  const v1Keys = [
    "v1Error.runtimeUnavailable",
    "v1Error.invalidParams",
    "v1Error.notFound",
    "v1Error.authFailed",
    "v1Error.capabilityMissing",
    "v1Error.tooLate",
    "v1Error.schemaDigestMismatch",
    "v1Error.unknownMethod",
    "v1Error.journalGap",
  ] as const;

  it("all 9 v1 error keys resolve in every locale", () => {
    for (const loc of ["en", "zh", "zh-TW"] as const) {
      for (const key of v1Keys) {
        const v = t(loc, key);
        expect(v, `${loc}.${key}`).not.toBe(key); // not falling back to the key itself
        expect(v.trim().length).toBeGreaterThan(0);
      }
    }
  });

  it("v1Error.runtimeUnavailable matches the runtime messageKey", () => {
    // Cross-check: the runtime's error registry messageKey is "runtime.unavailable".
    // The frontend key is "v1Error.runtimeUnavailable" (prefixed to avoid dot-namespace collisions).
    // A mapping table in src/lib/ompDesktopV1/errors.ts connects them.
    expect(t("en", "v1Error.runtimeUnavailable")).toBe("Runtime is not connected.");
  });
});
