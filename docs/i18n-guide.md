# Internationalization (i18n)

Supported locales, runtime switching, message catalogs, and the localization
envelope for Runtime-visible content.

> **Current status (2026-07-31):** three locales — **English (`en`),
> 简体中文 (`zh-CN`), 繁體中文 (`zh-TW`)** — 1889 message keys each, enforced
> by `pnpm check:i18n` (key parity + value types + non-empty). English is the
> source catalog and the final fallback.

## 1. Switching language

Settings → General → language picker. The choice persists
(`AppSettings.locale`) and applies **at runtime, without restart**. Default
locale is `zh-CN`. The system tray uses a minimal Rust-side locale of its
own.

**Honest boundary:** there is no OS-locale auto-detect — fresh installs start
in `zh-CN` until you switch.

## 2. Catalog layout (for developers)

| Locale | File |
|---|---|
| `en` + `zh` | `src/i18n/messages.ts` |
| `zh-TW` | `src/i18n/zh-tw.ts` (`zhTW` export) |

- Adding a key = adding it to **all three** locales in the same commit; the
  gate fails otherwise.
- `Locale = "zh" | "zh-TW" | "en"` (`messages.ts`); `normalizeLocale` maps
  aliases (`zh-cn`/`zh-hans` → `zh`, `zh-tw` → `zh-TW`, others → `en`).
- Lookup: `t(locale, key, vars)` / the `createT(locale)` helper; fallback
  chain requested locale → `en` → the key string itself.

## 3. Interpolation

Simple `{var}` substitution only:

```ts
t(locale, "sessions.deleteConfirm", { name })
```

**Honest boundary:** no ICU MessageFormat — no plural/select rules today.
Design §12's ICU parameter/type validation is roadmap; the current gate
checks key parity, value types, and emptiness, not ICU correctness. Write
copy that works without plurals (e.g. `"{count} item(s)"`).

## 4. Runtime-visible content (envelope)

Content crossing from the Runtime (tool results, errors, approvals) follows
the §12 envelope: a **stable `messageKey` + typed args**, rendered by the
shell. If no stable key exists, the shell shows a **localized summary + a
viewable redacted raw payload** — never embed uncontrolled raw text inside a
localized sentence.

## 5. What is not translated

- Model output, user input, project files, raw tool output (exempt
  categories).
- Product and model names, commands, file paths, code identifiers.
- Redacted raw Provider error payloads remain viewable as technical detail.

## 6. The gate

```sh
pnpm check:i18n
```

Validates, across all three locales: key parity (missing/extra), value types,
non-empty values. It is part of 1.0 acceptance (AC-2.5 / AC-3.1–3.3).

## 7. File index

| Area | File |
|---|---|
| en + zh catalogs | `src/i18n/messages.ts` |
| zh-TW catalog | `src/i18n/zh-tw.ts` |
| `createT` / `normalizeLocale` | `src/i18n/index.ts` |
| Completeness gate | `scripts/check-i18n-completeness.mjs` |
| Tray strings | `src-tauri/src/tray_i18n.rs` |
