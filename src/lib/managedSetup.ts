/**
 * Pure helpers for managed configuration setup (`grok setup` / `grok setup --json`).
 * Redacts secret-like fields so Settings never shows full keys.
 */

import { redact } from "./redact";

/** Known failure modes from the CLI / host wrapper. */
export type ManagedSetupErrorKind =
  | "missing_auth"
  | "rejected"
  | "cli_missing"
  | "timeout"
  | "parse"
  | "other";

/** Sanitized preview of managed config (safe to show in UI). */
export type ManagedSetupSummary = {
  /** Top-level keys present after redaction (stable sort). */
  topLevelKeys: string[];
  /** Safe scalar facts (string/number/boolean only, already redacted). */
  facts: Array<{ key: string; value: string }>;
  /** Nested section counts (e.g. models → 3). */
  sectionCounts: Array<{ key: string; count: number }>;
  /** Pretty JSON with secrets redacted. */
  redactedJson: string;
  /** Optional short note (e.g. deployment id fingerprint). */
  note?: string | null;
};

export type ManagedSetupResult = {
  ok: boolean;
  /** Install stdout message, or preview note. */
  message?: string | null;
  summary?: ManagedSetupSummary | null;
  error?: string | null;
  errorKind?: ManagedSetupErrorKind | null;
};

const SENSITIVE_KEY_RE =
  /^(api[_-]?key|token|secret|password|passwd|authorization|auth|access[_-]?token|refresh[_-]?token|client[_-]?secret|private[_-]?key|bearer|deployment[_-]?key|xai[_-]?api[_-]?key)$/i;

const SENSITIVE_CONTAINER_KEYS = new Set([
  "env",
  "environment",
  "headers",
  "authorization",
  "secrets",
  "credentials",
  "signatures",
  "managed_identity_signatures",
  "managedidentitysignatures",
]);

/** Max scalar facts / nested keys to list in the summary panel. */
const MAX_FACTS = 24;
const MAX_SECTION_COUNTS = 16;
/** Cap pretty-print size so huge payloads do not freeze the UI. */
const MAX_JSON_CHARS = 48_000;

export function isSensitiveKey(key: string): boolean {
  const k = (key ?? "").trim();
  if (!k) return false;
  if (SENSITIVE_KEY_RE.test(k)) return true;
  if (/api[_-]?key/i.test(k)) return true;
  if (/(^|[_-])(token|secret|password|passwd)($|[_-])/i.test(k)) return true;
  if (/deployment[_-]?key/i.test(k)) return true;
  // Signature blobs / fingerprints that are still secret-adjacent
  if (/(_sig|signature|fingerprint)$/i.test(k) && !/key_fingerprint/i.test(k)) {
    return /sig|signature/i.test(k);
  }
  return false;
}

function asRecord(v: unknown): Record<string, unknown> | null {
  if (v && typeof v === "object" && !Array.isArray(v)) {
    return v as Record<string, unknown>;
  }
  return null;
}

/**
 * Drop secrets from an arbitrary JSON-like value.
 * Sensitive keys become `"[REDACTED]"`; env/header/signature maps are fully redacted.
 */
export function redactSensitiveValue(value: unknown): unknown {
  if (value == null) return value;
  if (typeof value === "string") {
    return redact(value);
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return value;
  }
  if (Array.isArray(value)) {
    return value.map((item) => redactSensitiveValue(item));
  }
  const obj = asRecord(value);
  if (!obj) return value;

  const out: Record<string, unknown> = {};
  for (const [key, child] of Object.entries(obj)) {
    if (isSensitiveKey(key) || SENSITIVE_CONTAINER_KEYS.has(key.toLowerCase())) {
      out[key] = "[REDACTED]";
      continue;
    }
    out[key] = redactSensitiveValue(child);
  }
  return out;
}

/** Classify CLI stderr/stdout into a stable error kind for UI copy. */
export function classifySetupError(message: string | null | undefined): ManagedSetupErrorKind {
  const m = (message ?? "").toLowerCase();
  if (!m.trim()) return "other";
  if (
    m.includes("cli not found") ||
    m.includes("runtime not found") ||
    m.includes("no such file")
  ) {
    return "cli_missing";
  }
  if (m.includes("timed out") || m.includes("timeout")) {
    return "timeout";
  }
  if (
    m.includes("no deployment key") ||
    m.includes("team sign-in") ||
    m.includes("team login") ||
    m.includes("sign in with a team") ||
    m.includes("export grok_deployment_key")
  ) {
    return "missing_auth";
  }
  if (
    m.includes("deployment key was rejected") ||
    m.includes("key was rejected") ||
    m.includes("hasn't expired") ||
    m.includes("hasnt expired")
  ) {
    return "rejected";
  }
  if (m.includes("json") && (m.includes("parse") || m.includes("invalid"))) {
    return "parse";
  }
  return "other";
}

/** Pretty-print redacted JSON, capped for UI. */
export function formatRedactedJson(value: unknown): string {
  const safe = redactSensitiveValue(value);
  let text: string;
  try {
    text = JSON.stringify(safe, null, 2);
  } catch {
    text = String(safe);
  }
  text = redact(text);
  if (text.length > MAX_JSON_CHARS) {
    return `${text.slice(0, MAX_JSON_CHARS)}\n… [truncated]`;
  }
  return text;
}

function formatScalar(v: unknown): string | null {
  if (typeof v === "string") {
    const s = redact(v.trim());
    if (!s) return null;
    // Avoid dumping multi-line blobs into fact rows
    if (s.length > 120 || s.includes("\n")) {
      return `${s.slice(0, 80)}…`;
    }
    return s;
  }
  if (typeof v === "number" && Number.isFinite(v)) return String(v);
  if (typeof v === "boolean") return v ? "true" : "false";
  return null;
}

function countEntries(v: unknown): number | null {
  if (Array.isArray(v)) return v.length;
  const obj = asRecord(v);
  if (obj) return Object.keys(obj).length;
  return null;
}

/**
 * Build a secret-safe summary from raw `grok setup --json` output.
 * Accepts already-parsed JSON, or a JSON string (will parse).
 */
export function summarizeSetupJson(raw: unknown): ManagedSetupSummary {
  let root: unknown = raw;
  if (typeof raw === "string") {
    const trimmed = raw.trim();
    if (!trimmed) {
      return {
        topLevelKeys: [],
        facts: [],
        sectionCounts: [],
        redactedJson: "{}",
        note: null,
      };
    }
    try {
      root = JSON.parse(trimmed);
    } catch {
      // Non-JSON preview (plain text) — still scrub and show
      const scrubbed = redact(trimmed);
      return {
        topLevelKeys: [],
        facts: [],
        sectionCounts: [],
        redactedJson: scrubbed.slice(0, MAX_JSON_CHARS),
        note: "non-json",
      };
    }
  }

  const redacted = redactSensitiveValue(root);
  const obj = asRecord(redacted);
  const topLevelKeys = obj
    ? Object.keys(obj).sort((a, b) => a.localeCompare(b))
    : [];

  const facts: ManagedSetupSummary["facts"] = [];
  const sectionCounts: ManagedSetupSummary["sectionCounts"] = [];

  if (obj) {
    for (const key of topLevelKeys) {
      const child = obj[key];
      const scalar = formatScalar(child);
      if (scalar != null) {
        if (facts.length < MAX_FACTS) {
          facts.push({ key, value: scalar });
        }
        continue;
      }
      const n = countEntries(child);
      if (n != null && sectionCounts.length < MAX_SECTION_COUNTS) {
        sectionCounts.push({ key, count: n });
      }
    }
  } else if (Array.isArray(redacted)) {
    sectionCounts.push({ key: "items", count: redacted.length });
  }

  return {
    topLevelKeys,
    facts,
    sectionCounts,
    redactedJson: formatRedactedJson(root),
    note: null,
  };
}

/** Empty result helper for tests / UI defaults. */
export function emptySetupResult(
  partial?: Partial<ManagedSetupResult>,
): ManagedSetupResult {
  return {
    ok: false,
    message: null,
    summary: null,
    error: null,
    errorKind: null,
    ...partial,
  };
}
