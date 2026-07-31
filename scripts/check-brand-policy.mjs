import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
  directoryExclusions,
  repositoryExclusions,
  rules,
  structuredAllowlist,
  textExtensions,
  userVisiblePathPatterns,
  wholeFileAllowlist,
} from "./brand-policy.mjs";

const lowercaseBrand = /(?<![A-Za-z0-9_-])(?:omp|Omp|oMp|omP)(?![A-Za-z0-9_-])/g;

function normalizeRepositoryPath(file) {
  return path.posix.normalize(file.replaceAll("\\", "/")).replace(/^\.\//, "");
}

function valueAtPointer(value, pointer) {
  return pointer.reduce((current, segment) => current?.[segment], value);
}

function structuredRanges(file, text) {
  const entries = structuredAllowlist.get(file);
  if (!entries) return [];

  let document;
  try {
    document = JSON.parse(text);
  } catch {
    return [];
  }

  return entries.flatMap((entry) => {
    if (valueAtPointer(document, entry.pointer) !== entry.value) return [];
    const token = JSON.stringify(entry.value);
    const indexes = [];
    for (let index = text.indexOf(token); index !== -1; index = text.indexOf(token, index + token.length)) indexes.push(index);
    if (indexes.length !== 1) return [];
    return [{ start: indexes[0] + 1, end: indexes[0] + token.length - 1, rules: entry.rules }];
  });
}

function isStructuredAllowance(ranges, rule, index, length) {
  return ranges.some((range) => range.rules.has(rule) && index >= range.start && index + length <= range.end);
}

export function scanText(file, text) {
  const normalizedFile = normalizeRepositoryPath(file);
  if (wholeFileAllowlist.has(normalizedFile)) return [];

  const ranges = structuredRanges(normalizedFile, text);
  const violations = [];
  for (const [rule, configuredPattern] of rules) {
    const pattern = new RegExp(configuredPattern.source, configuredPattern.flags);
    for (const match of text.matchAll(pattern)) {
      if (!isStructuredAllowance(ranges, rule, match.index, match[0].length)) {
        violations.push({ file: normalizedFile, rule, match: match[0], index: match.index });
      }
    }
  }
  if (userVisiblePathPatterns.some((pattern) => pattern.test(normalizedFile))) {
    const pattern = new RegExp(lowercaseBrand.source, lowercaseBrand.flags);
    // Master design §2.4 item 26: command/executable/env-var/path/protocol/identifier
    // technical tokens may use lowercase. Inline-code spans (`` `omp` ``) in
    // user-visible markdown are technical identifiers (CLI binary names, etc.),
    // not product brand — exclude them from the lowercase-brand rule.
    const codeSpans = [];
    for (const m of text.matchAll(/`[^`\n]*`/g)) codeSpans.push([m.index, m.index + m[0].length]);
    const inCodeSpan = (index, length) => codeSpans.some(([s, e]) => index >= s && index + length <= e);
    for (const match of text.matchAll(pattern)) {
      if (inCodeSpan(match.index, match[0].length)) continue;
      violations.push({ file: normalizedFile, rule: "lowercase-user-visible-omp", match: match[0], index: match.index });
    }
  }
  return violations;
}

export function checkRepository(root) {
  const records = execFileSync("git", ["-C", root, "ls-files", "--stage", "-z"], { encoding: "utf8" }).split("\0").filter(Boolean);
  const violations = [];

  for (const record of records) {
    const tab = record.indexOf("\t");
    const metadata = record.slice(0, tab).split(" ");
    const file = normalizeRepositoryPath(record.slice(tab + 1));
    const mode = metadata[0];
    if ((mode !== "100644" && mode !== "100755") || repositoryExclusions.has(file) || directoryExclusions.some((pattern) => pattern.test(file)) || !textExtensions.has(path.posix.extname(file))) continue;

    const content = fs.readFileSync(path.join(root, file));
    if (content.includes(0)) continue;
    violations.push(...scanText(file, content.toString("utf8")));
  }

  return violations;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const violations = checkRepository(process.cwd());
  for (const item of violations) console.error(`${item.file}: ${item.rule}: ${item.match}`);
  if (violations.length) process.exitCode = 1;
}
