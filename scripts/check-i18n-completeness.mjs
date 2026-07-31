// Standalone i18n completeness checker.
// Loads src/i18n/messages.ts via typescript.transpileModule (no vitest dependency),
// asserts key parity + non-empty values across all locales, and exits 1 on any gap.
// Parallels check-brand-policy.mjs in shape and exit-code convention.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const PLACEHOLDER_RE = /\{(\w+)\}/g;

/**
 * Sorted unique `{var}` placeholder names in a message string. Same regex
 * shape as the frontend `interpolate()` (src/i18n/index.ts).
 */
export function extractPlaceholders(text) {
  const set = new Set();
  for (const m of text.matchAll(PLACEHOLDER_RE)) set.add(m[1]);
  return [...set].sort();
}

function transpileTs(filePath) {
  const source = fs.readFileSync(filePath, "utf8");
  return ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ESNext,
      target: ts.ScriptTarget.ES2022,
    },
  }).outputText;
}

/**
 * Load `src/i18n/messages.ts` as an ES module.
 *
 * `messages.ts` imports `./zh-tw`, which the `data:` URL loader cannot resolve
 * (relative specifiers need a filesystem base). We work around this by inlining
 * the transpiled `zh-tw.ts` export directly into the transpiled `messages.ts`
 * before importing — replacing the `import { zhTW } from "./zh-tw"` statement
 * with the actual `const zhTW = { ... }` declaration. After inlining, the
 * module has no relative imports and the `data:` URL loads cleanly.
 */
function loadMessagesModule() {
  const messagesJs = transpileTs(path.join(root, "src/i18n/messages.ts"));
  const zhTwJs = transpileTs(path.join(root, "src/i18n/zh-tw.ts"));

  // Match: export const zhTW = { ... };
  const zhTwExportMatch = zhTwJs.match(/export\s+const\s+zhTW\s*=\s*\{[\s\S]*?^};/m);
  if (!zhTwExportMatch) {
    throw new Error("check-i18n: could not find `export const zhTW` in transpiled src/i18n/zh-tw.ts");
  }
  // Drop the `export ` prefix so it becomes a local declaration.
  const zhTwInline = zhTwExportMatch[0].replace(/^export\s+/, "");

  // Replace the import statement with the inlined declaration.
  const importRe = /import\s+\{\s*zhTW\s*\}\s+from\s+"\.\/zh-tw"\s*;?/;
  if (!importRe.test(messagesJs)) {
    throw new Error("check-i18n: could not find `import { zhTW } from \"./zh-tw\"` in transpiled src/i18n/messages.ts");
  }
  const inlined = messagesJs.replace(importRe, zhTwInline);

  const dataUrl = `data:text/javascript;base64,${Buffer.from(inlined).toString("base64")}`;
  return import(dataUrl);
}

export function checkCatalog(messages) {
  const locales = Object.keys(messages);
  const violations = [];

  if (locales.length === 0) {
    violations.push("no locales found in messages export");
    return violations;
  }

  const enKeys = new Set(Object.keys(messages.en ?? {}));
  if (enKeys.size === 0) {
    violations.push("no keys found in messages.en (source locale is empty)");
    return violations;
  }

  const enTable = messages.en ?? {};
  const enPlaceholders = new Map();
  for (const k of enKeys) {
    const v = enTable[k];
    if (typeof v === "string") enPlaceholders.set(k, extractPlaceholders(v));
  }

  for (const loc of locales) {
    const table = messages[loc];
    if (!table || typeof table !== "object") {
      violations.push(`${loc}: locale table is missing or not an object`);
      continue;
    }
    const keys = Object.keys(table);

    // Missing keys
    for (const k of enKeys) {
      if (!(k in table)) {
        violations.push(`${loc}: missing key "${k}"`);
        continue;
      }
      const v = table[k];
      if (typeof v !== "string") {
        violations.push(`${loc}.${k}: value is not a string (got ${typeof v})`);
      } else if (v.trim().length === 0) {
        violations.push(`${loc}.${k}: value is empty or whitespace-only`);
      } else if (loc !== "en" && enPlaceholders.has(k)) {
        const want = enPlaceholders.get(k);
        const got = extractPlaceholders(v);
        const missing = want.filter((p) => !got.includes(p));
        const extra = got.filter((p) => !want.includes(p));
        if (missing.length || extra.length) {
          const parts = [];
          if (missing.length) parts.push(`missing: ${missing.join(", ")}`);
          if (extra.length) parts.push(`extra: ${extra.join(", ")}`);
          violations.push(`${loc}.${k}: placeholder mismatch (${parts.join("; ")})`);
        }
      }
    }

    // Extra keys
    for (const k of keys) {
      if (!enKeys.has(k)) {
        violations.push(`${loc}: extra key "${k}" not present in en`);
      }
    }
  }

  return violations;
}

async function main() {
  const messagesMod = await loadMessagesModule();
  const messages = messagesMod.messages;
  if (!messages) {
    console.error("check-i18n: failed to load messages export from src/i18n/messages.ts");
    process.exitCode = 1;
    return;
  }

  const violations = checkCatalog(messages);
  if (violations.length) {
    for (const v of violations) console.error(`check-i18n: ${v}`);
    console.error(`check-i18n: ${violations.length} violation(s)`);
    process.exitCode = 1;
    return;
  }

  const locales = Object.keys(messages);
  const enKeys = Object.keys(messages.en).length;
  console.log(`check-i18n: OK (${locales.length} locales, ${enKeys} keys each)`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  await main();
}
