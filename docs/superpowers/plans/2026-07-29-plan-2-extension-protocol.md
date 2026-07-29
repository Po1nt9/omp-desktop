# OMP Desktop Extension Protocol Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Define and implement the versioned OMP Desktop Extension Protocol (`_omp/desktop/v1/*`) on the Tauri Host (Rust) and React frontend (TypeScript), with JSON Schema sources, generated types, stable IDs, error codes, compatibility rules, cursor pagination, event journal replay, queue/steer semantics, Provider/Model/Credential API, MCP config/discovery, message localization envelope, thinking visibility classification, and contract tests — without spawning the OMP runtime process.

**Architecture:** Protocol schemas live in a versioned `protocol/v1/` directory as JSON Schema files. TypeScript types are generated from those schemas via `json-schema-to-typescript`; Rust types are hand-defined in `src-tauri/src/protocol/` and validated against the same schemas by contract tests. The Host exposes a protocol capability module that preserves the Plan 1 fail-closed `runtime_unavailable` behavior when no runtime is connected. Plan 2 defines and tests the protocol layer only; actual runtime process spawning belongs to Plan 3.

**Tech Stack:** Rust (serde, serde_json, uuid, sha2, chrono, thiserror), TypeScript (vitest, json-schema-to-typescript, ajv), JSON Schema draft 2020-12, pnpm, cargo.

## Global Constraints

- The OMP runtime submodule at `runtime/oh-my-pi/` is READ-ONLY. Plan 2 implements protocol code only on the Desktop Host side (Rust + TypeScript). Never modify files under `runtime/oh-my-pi/`.
- The protocol namespace is `_omp/desktop/v1/*`. This is a compatibility evolution of the current `_omp/*` namespace; do not claim the new namespace already exists in the runtime.
- Plan 1's fail-closed `runtime_unavailable` behavior must be preserved. When no runtime is connected, every Agent execution path still returns `runtime_unavailable`. Plan 2 adds protocol type definitions and contract tests but does not spawn the runtime process.
- Protocol schemas must be defined as versioned JSON Schema files in `protocol/v1/`.
- TypeScript types must be generated from JSON Schema using `json-schema-to-typescript`.
- Rust types must be hand-defined but validated against the schemas via contract tests.
- All protocol code must have contract tests.
- Do NOT spawn the OMP runtime process — that is Plan 3. Plan 2 only defines and tests the protocol layer.
- Every user-visible brand abbreviation is `OMP`; lowercase `omp` is allowed only in technical identifiers per Plan 1's brand policy.
- The application version remains `0.1.9` during Plan 2.
- Existing `_omp/*` methods in the OMP runtime (`_omp/sessions/listAll`, `_omp/projects/list`, `_omp/usage`, etc.) must be audited before defining the v1 namespace, but the runtime submodule must not be modified.
- Stable IDs must be opaque, deterministic, and collision-resistant; they must not leak internal pointers or process-local state.
- Error codes must be stable, machine-readable, and paired with `messageKey + args` for localization; technical details are opt-in and redacted by default.
- The schema digest in the capability descriptor must be a SHA-256 hash of the canonicalized schema set, enabling offline integrity checks.

---

## File and Module Map

**Create**

- `protocol/v1/capability.schema.json` — initialize capability descriptor schema.
- `protocol/v1/error.schema.json` — stable error envelope schema.
- `protocol/v1/ids.schema.json` — stable ID format and validation schema.
- `protocol/v1/pagination.schema.json` — cursor pagination and snapshot boundary schema.
- `protocol/v1/event-journal.schema.json` — event journal, stable event ID, replay cursor schema.
- `protocol/v1/queue-steer.schema.json` — queue receipt and steer extension schema.
- `protocol/v1/provider-model-credential.schema.json` — Provider/Model/Credential lifecycle schema.
- `protocol/v1/mcp-discovery.schema.json` — MCP config/discovery extension schema.
- `protocol/v1/localization.schema.json` — message localization envelope schema.
- `protocol/v1/thinking-visibility.schema.json` — thinking visibility classification schema.
- `protocol/v1/compatibility.schema.json` — major/minor compatibility and deprecation schema.
- `protocol/v1/manifest.json` — ordered list of all v1 schema files with SHA-256 digests.
- `scripts/generate-protocol-types.mjs` — generates TypeScript types from JSON Schemas.
- `scripts/validate-schemas.mjs` — validates that Rust and TypeScript types match schemas.
- `scripts/audit-omp-methods.mjs` — audits existing `_omp/*` methods in the runtime submodule.
- `scripts/audit-omp-methods.test.mjs` — tests for the audit script.
- `src-tauri/src/protocol/mod.rs` — protocol module root.
- `src-tauri/src/protocol/capability.rs` — capability descriptor and initialize handshake types.
- `src-tauri/src/protocol/ids.rs` — stable ID generation and validation.
- `src-tauri/src/protocol/error.rs` — stable error code registry and envelope.
- `src-tauri/src/protocol/compatibility.rs` — version negotiation, unknown-field handling, deprecation.
- `src-tauri/src/protocol/pagination.rs` — cursor pagination and snapshot boundaries.
- `src-tauri/src/protocol/event_journal.rs` — event journal, stable event ID, replay cursor.
- `src-tauri/src/protocol/queue_steer.rs` — queue receipt and steer extension types.
- `src-tauri/src/protocol/provider_model_credential.rs` — Provider/Model/Credential lifecycle types.
- `src-tauri/src/protocol/mcp_discovery.rs` — MCP config/discovery extension types.
- `src-tauri/src/protocol/localization.rs` — message localization envelope types.
- `src-tauri/src/protocol/thinking_visibility.rs` — thinking visibility classification types.
- `src-tauri/src/protocol/contract_tests.rs` — contract tests validating Rust types against schemas.
- `src-tauri/src/protocol/audit.rs` — audit of existing `_omp/*` methods (read-only).
- `src/lib/protocol/index.ts` — generated TypeScript types barrel export.
- `src/lib/protocol/capability.ts` — capability descriptor and handshake helpers.
- `src/lib/protocol/ids.ts` — stable ID validation helpers.
- `src/lib/protocol/error.ts` — error code registry and messageKey helpers.
- `src/lib/protocol/compatibility.ts` — version negotiation helpers.
- `src/lib/protocol/pagination.ts` — cursor pagination helpers.
- `src/lib/protocol/eventJournal.ts` — event journal and replay helpers.
- `src/lib/protocol/queueSteer.ts` — queue and steer extension helpers.
- `src/lib/protocol/providerModelCredential.ts` — Provider/Model/Credential helpers.
- `src/lib/protocol/mcpDiscovery.ts` — MCP config/discovery helpers.
- `src/lib/protocol/localization.ts` — localization envelope helpers.
- `src/lib/protocol/thinkingVisibility.ts` — thinking visibility helpers.
- `src/lib/protocol/capability.test.ts` — capability descriptor contract tests.
- `src/lib/protocol/ids.test.ts` — stable ID contract tests.
- `src/lib/protocol/error.test.ts` — error registry contract tests.
- `src/lib/protocol/compatibility.test.ts` — compatibility rule contract tests.
- `src/lib/protocol/pagination.test.ts` — pagination contract tests.
- `src/lib/protocol/eventJournal.test.ts` — event journal contract tests.
- `src/lib/protocol/queueSteer.test.ts` — queue/steer contract tests.
- `src/lib/protocol/providerModelCredential.test.ts` — Provider/Model/Credential contract tests.
- `src/lib/protocol/mcpDiscovery.test.ts` — MCP discovery contract tests.
- `src/lib/protocol/localization.test.ts` — localization envelope contract tests.
- `src/lib/protocol/thinkingVisibility.test.ts` — thinking visibility contract tests.
- `docs/superpowers/verification/2026-07-29-plan-2-extension-protocol.md` — verification record.

**Modify**

- `src-tauri/src/lib.rs` — register the `protocol` module.
- `src-tauri/Cargo.toml` — add `jsonschema` dev-dependency for contract tests.
- `package.json` — add `json-schema-to-typescript`, `ajv` devDependencies and protocol scripts.
- `src/lib/runtimeAvailability.ts` — extend with protocol capability awareness (still fail-closed).

**Delete**

- None.

---

### Task 1: Audit Existing `_omp/*` Methods in the OMP Runtime Submodule

**Files:**
- Create: `scripts/audit-omp-methods.mjs`
- Test: `scripts/audit-omp-methods.test.mjs`
- Modify: `package.json`

**Interfaces:**
- Consumes: read-only `runtime/oh-my-pi/` submodule source tree.
- Produces: `pnpm audit:omp-methods` command; a JSON audit report of all existing `_omp/*` methods, notifications, request/response shapes, and error strings found in the runtime source.

- [ ] **Step 1: Write the failing audit test**

Create `scripts/audit-omp-methods.test.mjs`:

```js
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import test from "node:test";
import { auditOmpMethods, formatAuditReport } from "./audit-omp-methods.mjs";

test("audit discovers existing _omp/* method references in the runtime submodule", () => {
  const report = auditOmpMethods("runtime/oh-my-pi");
  assert.ok(report.methods.length > 0, "expected at least one _omp/* method");
  const hasSessionsListAll = report.methods.some((m) => m.name === "_omp/sessions/listAll");
  assert.ok(hasSessionsListAll, "expected _omp/sessions/listAll in audit results");
});

test("audit report is serializable and includes source file paths", () => {
  const report = auditOmpMethods("runtime/oh-my-pi");
  const json = JSON.stringify(report);
  const parsed = JSON.parse(json);
  assert.ok(parsed.methods.every((m) => typeof m.sourceFile === "string"));
});

test("formatAuditReport produces human-readable markdown", () => {
  const report = auditOmpMethods("runtime/oh-my-pi");
  const md = formatAuditReport(report);
  assert.ok(md.includes("# OMP Runtime `_omp/*` Method Audit"));
  assert.ok(md.includes("_omp/sessions/listAll"));
});
```

- [ ] **Step 2: Run the test and verify the missing module failure**

Run:

```bash
node --test scripts/audit-omp-methods.test.mjs
```

Expected: FAIL with `ERR_MODULE_NOT_FOUND` for `audit-omp-methods.mjs`.

- [ ] **Step 3: Implement the audit script**

Create `scripts/audit-omp-methods.mjs`:

```js
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const OMP_METHOD_PATTERN = /["'](_omp\/[a-zA-Z0-9_\/]+)["']/g;
const TEXT_EXTENSIONS = new Set([".ts", ".js", ".mjs", ".rs"]);

/**
 * @param {string} root - repository root containing runtime/oh-my-pi
 * @returns {{methods: Array<{name: string, sourceFile: string, line: number, context: string}>, notifications: Array<{name: string, sourceFile: string, line: number}>, errors: string[]}}
 */
export function auditOmpMethods(root) {
  const submodulePath = path.join(root, "runtime/oh-my-pi");
  if (!existsSync(submodulePath)) {
    return { methods: [], notifications: [], errors: ["runtime/oh-my-pi submodule not found"] };
  }
  const files = execFileSync("git", ["-C", submodulePath, "ls-files"], { encoding: "utf8" })
    .trim()
    .split("\n")
    .filter((f) => TEXT_EXTENSIONS.has(path.extname(f)));

  const methods = [];
  const seenMethods = new Set();
  const errors = [];

  for (const file of files) {
    const fullPath = path.join(submodulePath, file);
    const text = fs.readFileSync(fullPath, "utf8");
    const lines = text.split("\n");
    for (let i = 0; i < lines.length; i++) {
      OMP_METHOD_PATTERN.lastIndex = 0;
      const matches = [...lines[i].matchAll(OMP_METHOD_PATTERN)];
      for (const match of matches) {
        const name = match[1];
        const key = `${name}@${file}:${i + 1}`;
        if (!seenMethods.has(name)) {
          seenMethods.add(name);
          methods.push({
            name,
            sourceFile: `runtime/oh-my-pi/${file}`,
            line: i + 1,
            context: lines[i].trim().slice(0, 200),
          });
        }
      }
    }
  }

  methods.sort((a, b) => a.name.localeCompare(b.name));
  return { methods, notifications: [], errors };
}

export function formatAuditReport(report) {
  const lines = ["# OMP Runtime `_omp/*` Method Audit", ""];
  lines.push(`Total methods found: ${report.methods.length}`, "");
  lines.push("| Method | Source File | Line |");
  lines.push("| --- | --- | --- |");
  for (const m of report.methods) {
    lines.push(`| \`${m.name}\` | ${m.sourceFile} | ${m.line} |`);
  }
  return lines.join("\n");
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const report = auditOmpMethods(process.cwd());
  console.log(formatAuditReport(report));
  if (report.errors.length) {
    console.error("\nErrors:", report.errors.join("; "));
    process.exitCode = 1;
  }
}
```

- [ ] **Step 4: Register and run the audit command**

Add to `package.json` scripts:

```json
"audit:omp-methods": "node scripts/audit-omp-methods.mjs"
```

Run:

```bash
node --test scripts/audit-omp-methods.test.mjs
pnpm audit:omp-methods > /tmp/omp-plan2-audit.txt
```

Expected: tests PASS; audit output lists all `_omp/*` methods found in the runtime submodule including `_omp/sessions/listAll`.

- [ ] **Step 5: Commit the audit baseline**

Run:

```bash
git add scripts/audit-omp-methods.mjs scripts/audit-omp-methods.test.mjs package.json
git commit -m "feat: audit existing _omp/* methods in OMP runtime submodule"
```

---

### Task 2: Define the `_omp/desktop/v1/*` Namespace JSON Schemas

**Files:**
- Create: `protocol/v1/capability.schema.json`
- Create: `protocol/v1/error.schema.json`
- Create: `protocol/v1/ids.schema.json`
- Create: `protocol/v1/pagination.schema.json`
- Create: `protocol/v1/event-journal.schema.json`
- Create: `protocol/v1/queue-steer.schema.json`
- Create: `protocol/v1/provider-model-credential.schema.json`
- Create: `protocol/v1/mcp-discovery.schema.json`
- Create: `protocol/v1/localization.schema.json`
- Create: `protocol/v1/thinking-visibility.schema.json`
- Create: `protocol/v1/compatibility.schema.json`
- Create: `protocol/v1/manifest.json`
- Create: `scripts/generate-protocol-types.mjs`
- Modify: `package.json`

**Interfaces:**
- Consumes: master design spec sections 5.1–5.4, 10, 11, 13; audit results from Task 1.
- Produces: `pnpm generate:protocol` command; `src/lib/protocol/generated/` TypeScript types from all schemas; `protocol/v1/manifest.json` with SHA-256 digests.

- [ ] **Step 1: Create the capability descriptor schema**

Create `protocol/v1/capability.schema.json`:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://omp-desktop.dev/protocol/v1/capability.schema.json",
  "title": "CapabilityDescriptor",
  "description": "Initialize capability descriptor for the OMP Desktop Extension Protocol v1",
  "type": "object",
  "required": ["protocolVersion", "extensionVersion", "methods", "notifications", "features", "schemaDigest", "limits"],
  "additionalProperties": false,
  "properties": {
    "protocolVersion": {
      "type": "string",
      "const": "_omp/desktop/v1",
      "description": "The versioned protocol namespace identifier"
    },
    "extensionVersion": {
      "type": "object",
      "required": ["major", "minor", "patch"],
      "additionalProperties": false,
      "properties": {
        "major": { "type": "integer", "minimum": 1 },
        "minor": { "type": "integer", "minimum": 0 },
        "patch": { "type": "integer", "minimum": 0 }
      }
    },
    "methods": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["name", "required", "deprecated"],
        "additionalProperties": false,
        "properties": {
          "name": { "type": "string", "pattern": "^_omp/desktop/v1/[a-zA-Z0-9_]+$" },
          "required": { "type": "boolean" },
          "deprecated": { "type": "boolean" },
          "replacedBy": { "type": ["string", "null"] },
          "minVersion": { "type": "string", "pattern": "^[0-9]+\\.[0-9]+\\.[0-9]+$" }
        }
      }
    },
    "notifications": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["name", "required", "deprecated"],
        "additionalProperties": false,
        "properties": {
          "name": { "type": "string", "pattern": "^_omp/desktop/v1/[a-zA-Z0-9_]+$" },
          "required": { "type": "boolean" },
          "deprecated": { "type": "boolean" },
          "replacedBy": { "type": ["string", "null"] }
        }
      }
    },
    "features": {
      "type": "object",
      "required": ["core", "extensions"],
      "additionalProperties": false,
      "properties": {
        "core": {
          "type": "array",
          "items": { "type": "string" },
          "description": "Core ACP features: session, prompt, cancel, tool, permission, elicitation"
        },
        "extensions": {
          "type": "array",
          "items": {
            "type": "object",
            "required": ["name", "required"],
            "additionalProperties": false,
            "properties": {
              "name": { "type": "string" },
              "required": { "type": "boolean" },
              "available": { "type": "boolean" },
              "reasonCode": { "type": ["string", "null"] }
            }
          }
        }
      }
    },
    "schemaDigest": {
      "type": "string",
      "pattern": "^sha256:[0-9a-f]{64}$",
      "description": "SHA-256 digest of the canonicalized schema set"
    },
    "limits": {
      "type": "object",
      "required": ["maxFrameBytes", "maxReassembledFrameBytes", "maxPageSize", "maxQueueDepth"],
      "additionalProperties": false,
      "properties": {
        "maxFrameBytes": { "type": "integer", "minimum": 1048576 },
        "maxReassembledFrameBytes": { "type": "integer", "minimum": 16777216 },
        "maxPageSize": { "type": "integer", "minimum": 1, "maximum": 256 },
        "maxQueueDepth": { "type": "integer", "minimum": 1 }
      }
    }
  }
}
```

- [ ] **Step 2: Create the error envelope schema**

Create `protocol/v1/error.schema.json`:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://omp-desktop.dev/protocol/v1/error.schema.json",
  "title": "ProtocolError",
  "description": "Stable error envelope with messageKey + args for localization",
  "type": "object",
  "required": ["code", "category", "severity", "retryable", "recoverable", "messageKey", "args"],
  "additionalProperties": false,
  "properties": {
    "code": {
      "type": "string",
      "pattern": "^[A-Z][A-Z0-9_]*$",
      "description": "Stable machine-readable error code (SCREAMING_SNAKE_CASE)"
    },
    "category": {
      "type": "string",
      "enum": ["protocol", "runtime", "provider", "credential", "permission", "session", "queue", "steer", "mcp", "config", "transport", "internal"]
    },
    "severity": {
      "type": "string",
      "enum": ["fatal", "error", "warning", "info"]
    },
    "retryable": {
      "type": "boolean",
      "description": "Whether the caller may retry the same request"
    },
    "recoverable": {
      "type": "boolean",
      "description": "Whether the session/process can continue after this error"
    },
    "messageKey": {
      "type": "string",
      "pattern": "^[a-z][a-z0-9]*(\\.[a-z][a-z0-9]*)+$",
      "description": "Stable localization message key (e.g. 'protocol.error.runtime_unavailable')"
    },
    "args": {
      "type": "object",
      "description": "Typed arguments for the message key (ICU MessageFormat)",
      "additionalProperties": { "type": ["string", "number", "boolean", "null"] }
    },
    "technicalDetail": {
      "type": ["string", "null"],
      "description": "Opt-in redacted technical detail; never contains secrets or raw user content"
    },
    "recoveryActions": {
      "type": "array",
      "items": { "type": "string", "enum": ["retry", "reconnect", "reconfigure", "contact_support", "restart_session", "restart_runtime", "none"] }
    }
  }
}
```

- [ ] **Step 3: Create the stable ID schema**

Create `protocol/v1/ids.schema.json`:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://omp-desktop.dev/protocol/v1/ids.schema.json",
  "title": "StableIds",
  "description": "Stable ID format definitions for all protocol entities",
  "type": "object",
  "definitions": {
    "SessionId": {
      "type": "string",
      "pattern": "^ses_[0-9a-f]{16}$",
      "description": "Opaque session identifier; 16 hex chars prefixed with ses_"
    },
    "TurnId": {
      "type": "string",
      "pattern": "^turn_[0-9a-f]{16}$",
      "description": "Opaque turn identifier; 16 hex chars prefixed with turn_"
    },
    "EventId": {
      "type": "string",
      "pattern": "^evt_[0-9a-f]{24}$",
      "description": "Globally unique event identifier; 24 hex chars prefixed with evt_"
    },
    "PermissionRequestId": {
      "type": "string",
      "pattern": ^perm_[0-9a-f]{16}$",
      "description": "Opaque permission request identifier"
    },
    "QueueReceiptId": {
      "type": "string",
      "pattern": "^q_[0-9a-f]{16}$",
      "description": "Opaque queue receipt identifier"
    },
    "CredentialRefId": {
      "type": "string",
      "pattern": "^cred_[0-9a-f]{16}$",
      "description": "Opaque credential reference identifier; never contains the secret itself"
    },
    "ProjectId": {
      "type": "string",
      "pattern": "^proj_[0-9a-f]{16}$",
      "description": "Opaque project identifier"
    },
    "ModelId": {
      "type": "string",
      "pattern": "^mdl_[A-Za-z0-9_-]+$",
      "description": "Stable model identifier scoped to a provider"
    },
    "McpSourceId": {
      "type": "string",
      "pattern": "^mcp_[0-9a-f]{16}$",
      "description": "Opaque MCP source identifier"
    }
  }
}
```

- [ ] **Step 4: Create the pagination schema**

Create `protocol/v1/pagination.schema.json`:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://omp-desktop.dev/protocol/v1/pagination.schema.json",
  "title": "CursorPagination",
  "type": "object",
  "required": ["items", "hasMore"],
  "additionalProperties": false,
  "properties": {
    "items": {
      "type": "array",
      "description": "Page items in strict local ordering"
    },
    "hasMore": { "type": "boolean" },
    "nextCursor": {
      "type": ["string", "null"],
      "description": "Opaque cursor bound to the snapshot boundary; null when hasMore is false"
    },
    "snapshotBoundary": {
      "type": "object",
      "required": ["sessionId", "commitSeq"],
      "additionalProperties": false,
      "properties": {
        "sessionId": { "type": "string" },
        "commitSeq": { "type": "integer", "minimum": 0, "description": "Journal commit sequence at snapshot time" }
      }
    },
    "totalItems": {
      "type": ["integer", "null"],
      "minimum": 0
    }
  }
}
```

- [ ] **Step 5: Create the event journal schema**

Create `protocol/v1/event-journal.schema.json`:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://omp-desktop.dev/protocol/v1/event-journal.schema.json",
  "title": "EventJournal",
  "type": "object",
  "required": ["eventId", "sessionId", "turnId", "eventType", "timestamp", "seq"],
  "additionalProperties": true,
  "properties": {
    "eventId": {
      "type": "string",
      "pattern": "^evt_[0-9a-f]{24}$",
      "description": "Stable event ID; survives connection restart"
    },
    "sessionId": { "type": "string", "pattern": "^ses_[0-9a-f]{16}$" },
    "turnId": { "type": ["string", "null"], "pattern": "^turn_[0-9a-f]{16}$" },
    "eventType": { "type": "string" },
    "timestamp": { "type": "string", "format": "date-time" },
    "seq": { "type": "integer", "minimum": 0, "description": "Strict local sequence within session" },
    "replayCursor": {
      "type": ["string", "null"],
      "description": "Opaque cursor for resuming replay after this event"
    },
    "commitPoint": {
      "type": ["boolean", "null"],
      "description": "True if this event is a durable journal commit point"
    }
  }
}
```

- [ ] **Step 6: Create the queue and steer schema**

Create `protocol/v1/queue-steer.schema.json`:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://omp-desktop.dev/protocol/v1/queue-steer.schema.json",
  "title": "QueueAndSteer",
  "definitions": {
    "QueueReceipt": {
      "type": "object",
      "required": ["receiptId", "sessionId", "turnId", "status", "position", "submittedAt"],
      "additionalProperties": false,
      "properties": {
        "receiptId": { "type": "string", "pattern": "^q_[0-9a-f]{16}$" },
        "sessionId": { "type": "string", "pattern": "^ses_[0-9a-f]{16}$" },
        "turnId": { "type": "string", "pattern": "^turn_[0-9a-f]{16}$" },
        "status": { "type": "string", "enum": ["accepted", "rejected", "dequeued", "cancelled"] },
        "position": { "type": "integer", "minimum": 0 },
        "priority": { "type": ["integer", "null"], "minimum": 0 },
        "submittedAt": { "type": "string", "format": "date-time" }
      }
    },
    "SteerRequest": {
      "type": "object",
      "required": ["targetTurnId", "message", "ackDeadline"],
      "additionalProperties": false,
      "properties": {
        "targetTurnId": { "type": "string", "pattern": "^turn_[0-9a-f]{16}$" },
        "message": { "type": "string", "minLength": 1 },
        "ackDeadline": { "type": "integer", "minimum": 1000 },
        "images": {
          "type": "array",
          "items": { "type": "object" }
        }
      }
    },
    "SteerAck": {
      "type": "object",
      "required": ["targetTurnId", "appliedOrder", "acknowledgedAt"],
      "additionalProperties": false,
      "properties": {
        "targetTurnId": { "type": "string", "pattern": "^turn_[0-9a-f]{16}$" },
        "appliedOrder": { "type": "integer", "minimum": 0, "description": "Order in which the steer was applied relative to other steers" },
        "acknowledgedAt": { "type": "string", "format": "date-time" }
      }
    },
    "SteerError": {
      "type": "string",
      "enum": ["too_late", "conflict", "turn_not_active", "ack_timeout"]
    }
  }
}
```

- [ ] **Step 7: Create the Provider/Model/Credential schema**

Create `protocol/v1/provider-model-credential.schema.json`:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://omp-desktop.dev/protocol/v1/provider-model-credential.schema.json",
  "title": "ProviderModelCredential",
  "definitions": {
    "Provider": {
      "type": "object",
      "required": ["providerId", "displayNameKey", "authMethods", "status", "capabilities", "regions"],
      "additionalProperties": false,
      "properties": {
        "providerId": { "type": "string", "pattern": "^[a-z][a-z0-9-]*$" },
        "displayNameKey": { "type": "string", "pattern": "^[a-z][a-z0-9]*(\\.[a-z][a-z0-9]*)+$" },
        "authMethods": {
          "type": "array",
          "items": { "type": "string", "enum": ["api_key", "oauth", "bearer", "none"] }
        },
        "configSchema": { "type": ["object", "null"] },
        "status": { "type": "string", "enum": ["available", "unavailable", "deprecated", "error"] },
        "capabilities": {
          "type": "array",
          "items": { "type": "string" }
        },
        "regions": {
          "type": "array",
          "items": { "type": "string" }
        }
      }
    },
    "Model": {
      "type": "object",
      "required": ["modelId", "providerId", "displayName", "contextWindow", "inputModalities", "outputModalities", "toolSupport", "thinkingSupport"],
      "additionalProperties": false,
      "properties": {
        "modelId": { "type": "string", "pattern": "^mdl_[A-Za-z0-9_-]+$" },
        "providerId": { "type": "string", "pattern": "^[a-z][a-z0-9-]*$" },
        "displayName": { "type": "string" },
        "contextWindow": { "type": "integer", "minimum": 1 },
        "inputModalities": { "type": "array", "items": { "type": "string", "enum": ["text", "image", "audio", "video"] } },
        "outputModalities": { "type": "array", "items": { "type": "string", "enum": ["text", "image", "audio", "video"] } },
        "toolSupport": { "type": "boolean" },
        "thinkingSupport": { "type": "boolean" },
        "reasoningLevels": {
          "type": "array",
          "items": { "type": "string", "enum": ["off", "minimal", "low", "medium", "high", "xhigh", "max"] }
        },
        "availability": { "type": "string", "enum": ["available", "deprecated", "preview", "unavailable"] },
        "costPerInputToken": { "type": ["number", "null"], "minimum": 0 },
        "costPerOutputToken": { "type": ["number", "null"], "minimum": 0 }
      }
    },
    "CredentialMetadata": {
      "type": "object",
      "required": ["credentialRefId", "providerId", "status"],
      "additionalProperties": false,
      "properties": {
        "credentialRefId": { "type": "string", "pattern": "^cred_[0-9a-f]{16}$" },
        "providerId": { "type": "string" },
        "status": { "type": "string", "enum": ["active", "expired", "revoked", "pending", "error"] },
        "authMethod": { "type": "string" },
        "label": { "type": ["string", "null"] },
        "lastUsedAt": { "type": ["string", "null"], "format": "date-time" },
        "healthCheckedAt": { "type": ["string", "null"], "format": "date-time" }
      }
    },
    "SessionConfig": {
      "type": "object",
      "required": ["modelId", "providerId"],
      "additionalProperties": false,
      "properties": {
        "modelId": { "type": "string", "pattern": "^mdl_[A-Za-z0-9_-]+$" },
        "providerId": { "type": "string" },
        "reasoningLevel": { "type": ["string", "null"], "enum": ["off", "minimal", "low", "medium", "high", "xhigh", "max", null] },
        "configSchema": { "type": ["object", "null"] },
        "scope": { "type": "string", "enum": ["session", "project", "global"] },
        "changeableAt": { "type": "string", "enum": ["anytime", "turn_boundary", "session_start"] }
      }
    }
  }
}
```

- [ ] **Step 8: Create the MCP discovery schema**

Create `protocol/v1/mcp-discovery.schema.json`:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://omp-desktop.dev/protocol/v1/mcp-discovery.schema.json",
  "title": "McpDiscovery",
  "definitions": {
    "McpSource": {
      "type": "object",
      "required": ["sourceId", "name", "transport", "status"],
      "additionalProperties": false,
      "properties": {
        "sourceId": { "type": "string", "pattern": "^mcp_[0-9a-f]{16}$" },
        "name": { "type": "string" },
        "transport": { "type": "string", "enum": ["stdio", "sse", "websocket", "http"] },
        "command": { "type": ["string", "null"] },
        "args": { "type": "array", "items": { "type": "string" } },
        "url": { "type": ["string", "null"] },
        "status": { "type": "string", "enum": ["connected", "disconnected", "error", "disabled"] },
        "scope": { "type": "string", "enum": ["global", "project"] },
        "configSchema": { "type": ["object", "null"] }
      }
    },
    "McpTool": {
      "type": "object",
      "required": ["sourceId", "toolName", "description"],
      "additionalProperties": false,
      "properties": {
        "sourceId": { "type": "string", "pattern": "^mcp_[0-9a-f]{16}$" },
        "toolName": { "type": "string" },
        "description": { "type": "string" },
        "parameters": { "type": "object" }
      }
    }
  }
}
```

- [ ] **Step 9: Create the localization envelope schema**

Create `protocol/v1/localization.schema.json`:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://omp-desktop.dev/protocol/v1/localization.schema.json",
  "title": "LocalizationEnvelope",
  "type": "object",
  "required": ["messageKey", "args"],
  "additionalProperties": false,
  "properties": {
    "messageKey": {
      "type": "string",
      "pattern": "^[a-z][a-z0-9]*(\\.[a-z][a-z0-9]*)+$",
      "description": "Stable semantic message key for ICU MessageFormat"
    },
    "args": {
      "type": "object",
      "description": "Typed arguments for the message key",
      "additionalProperties": { "type": ["string", "number", "boolean", "null"] }
    },
    "fallback": {
      "type": ["string", "null"],
      "description": "English fallback string; only used when no locale catalog has the key"
    },
    "redactedRaw": {
      "type": ["string", "null"],
      "description": "Optional redacted raw source string for diagnostics; never contains secrets"
    }
  }
}
```

- [ ] **Step 10: Create the thinking visibility schema**

Create `protocol/v1/thinking-visibility.schema.json`:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://omp-desktop.dev/protocol/v1/thinking-visibility.schema.json",
  "title": "ThinkingVisibility",
  "type": "object",
  "required": ["classification", "source"],
  "additionalProperties": false,
  "properties": {
    "classification": {
      "type": "string",
      "enum": ["user-visible", "desktop-only", "remote-allowed", "internal"],
      "description": "Visibility classification for a thinking event"
    },
    "source": {
      "type": "string",
      "enum": ["runtime", "desktop", "model"],
      "description": "Who classified the visibility"
    },
    "reasonCode": {
      "type": ["string", "null"],
      "description": "Optional stable reason code when classification differs from default"
    }
  }
}
```

- [ ] **Step 11: Create the compatibility schema**

Create `protocol/v1/compatibility.schema.json`:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://omp-desktop.dev/protocol/v1/compatibility.schema.json",
  "title": "CompatibilityRules",
  "type": "object",
  "required": ["hostVersion", "runtimeVersion", "negotiatedVersion", "unknownFieldPolicy", "deprecations"],
  "additionalProperties": false,
  "properties": {
    "hostVersion": {
      "type": "object",
      "required": ["major", "minor", "patch"],
      "additionalProperties": false,
      "properties": {
        "major": { "type": "integer" },
        "minor": { "type": "integer" },
        "patch": { "type": "integer" }
      }
    },
    "runtimeVersion": {
      "type": ["object", "null"],
      "required": ["major", "minor", "patch"],
      "additionalProperties": false,
      "properties": {
        "major": { "type": "integer" },
        "minor": { "type": "integer" },
        "patch": { "type": "integer" }
      }
    },
    "negotiatedVersion": {
      "type": "string",
      "description": "The highest compatible major.minor both sides support"
    },
    "unknownFieldPolicy": {
      "type": "string",
      "enum": ["ignore", "reject", "warn"],
      "description": "How unknown fields in messages are handled"
    },
    "deprecations": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["name", "deprecatedIn", "removedIn", "replacement"],
        "additionalProperties": false,
        "properties": {
          "name": { "type": "string" },
          "deprecatedIn": { "type": "string", "pattern": "^[0-9]+\\.[0-9]+\\.[0-9]+$" },
          "removedIn": { "type": ["string", "null"], "pattern": "^[0-9]+\\.[0-9]+\\.[0-9]+$" },
          "replacement": { "type": ["string", "null"] }
        }
      }
    }
  }
}
```

- [ ] **Step 12: Create the manifest with schema digests**

Create `protocol/v1/manifest.json`:

```json
{
  "schemaVersion": 1,
  "protocolNamespace": "_omp/desktop/v1",
  "schemas": [
    "capability.schema.json",
    "error.schema.json",
    "ids.schema.json",
    "pagination.schema.json",
    "event-journal.schema.json",
    "queue-steer.schema.json",
    "provider-model-credential.schema.json",
    "mcp-discovery.schema.json",
    "localization.schema.json",
    "thinking-visibility.schema.json",
    "compatibility.schema.json"
  ],
  "digests": {}
}
```

- [ ] **Step 13: Install type generation dependencies and create the generator script**

Run:

```bash
pnpm add -D json-schema-to-typescript ajv
```

Create `scripts/generate-protocol-types.mjs`:

```js
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { compile, compileFromFile } from "json-schema-to-typescript";
import { fileURLToPath } from "node:url";

const SCHEMA_DIR = path.join(process.cwd(), "protocol", "v1");
const OUTPUT_DIR = path.join(process.cwd(), "src", "lib", "protocol", "generated");
const MANIFEST_PATH = path.join(SCHEMA_DIR, "manifest.json");
import crypto from "node:crypto";

function sha256File(filePath) {
  const content = fs.readFileSync(filePath);
  return "sha256:" + crypto.createHash("sha256").update(content).digest("hex");
}

async function generateTypes() {
  if (!fs.existsSync(OUTPUT_DIR)) fs.mkdirSync(OUTPUT_DIR, { recursive: true });
  const manifest = JSON.parse(fs.readFileSync(MANIFEST_PATH, "utf8"));
  const digests = {};
  for (const schemaFile of manifest.schemas) {
    const schemaPath = path.join(SCHEMA_DIR, schemaFile);
    digests[schemaFile] = sha256File(schemaPath);
    const tsName = schemaFile.replace(".schema.json", ".ts");
    const outputPath = path.join(OUTPUT_DIR, tsName);
    const ts = await compileFromFile(schemaPath, {
      bannerComment: "// AUTO-GENERATED from protocol/v1/*.schema.json — do not edit manually",
      additionalProperties: false,
    });
    fs.writeFileSync(outputPath, ts);
    console.log(`Generated ${outputPath}`);
  }
  manifest.digests = digests;
  const allSchemasContent = manifest.schemas
    .map((f) => fs.readFileSync(path.join(SCHEMA_DIR, f), "utf8"))
    .join("\n");
  manifest.schemaDigest = "sha256:" + crypto.createHash("sha256").update(allSchemasContent).digest("hex");
  fs.writeFileSync(MANIFEST_PATH, JSON.stringify(manifest, null, 2) + "\n");
  console.log("Updated manifest with digests");
}

generateTypes().catch((err) => {
  console.error(err);
  process.exit(1);
});
```

- [ ] **Step 14: Register and run the type generation command**

Add to `package.json` scripts:

```json
"generate:protocol": "node scripts/generate-protocol-types.mjs"
```

Run:

```bash
pnpm generate:protocol
```

Expected: TypeScript files generated under `src/lib/protocol/generated/`; `protocol/v1/manifest.json` updated with SHA-256 digests for each schema and a combined `schemaDigest`.

- [ ] **Step 15: Commit the protocol schema baseline**

Run:

```bash
git add protocol/v1 scripts/generate-protocol-types.mjs src/lib/protocol/generated package.json
git commit -m "feat: define _omp/desktop/v1/* JSON Schema namespace"
```

---

### Task 3: Implement Capability Descriptor and Initialize Handshake

**Files:**
- Create: `src-tauri/src/protocol/mod.rs`
- Create: `src-tauri/src/protocol/capability.rs`
- Create: `src/lib/protocol/capability.ts`
- Test: `src/lib/protocol/capability.test.ts`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `protocol/v1/capability.schema.json` from Task 2.
- Produces: Rust `CapabilityDescriptor` struct; TypeScript `CapabilityDescriptor` type; host-side capability descriptor constant for Plan 2 (fail-closed).

- [ ] **Step 1: Write the failing capability descriptor test**

Create `src/lib/protocol/capability.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { HOST_CAPABILITY_DESCRIPTOR, negotiateCapabilities, type CapabilityDescriptor } from "./capability";

describe("HOST_CAPABILITY_DESCRIPTOR", () => {
  it("advertises the _omp/desktop/v1 protocol namespace", () => {
    expect(HOST_CAPABILITY_DESCRIPTOR.protocolVersion).toBe("_omp/desktop/v1");
  });

  it("declares extension version 1.0.0", () => {
    expect(HOST_CAPABILITY_DESCRIPTOR.extensionVersion).toEqual({ major: 1, minor: 0, patch: 0 });
  });

  it("includes all mandatory 1.0 baseline extensions", () => {
    const required = [
      "queue_steer",
      "provider_model_credential",
      "mcp_discovery",
      "todo",
      "subagent",
      "branch_checkpoint_rewind",
      "usage_compaction",
      "attachment",
      "diagnostics",
      "event_replay_recovery",
      "message_localization",
      "thinking_visibility",
      "trace_correlation",
    ];
    const names = HOST_CAPABILITY_DESCRIPTOR.features.extensions.map((e) => e.name);
    for (const ext of required) {
      expect(names).toContain(ext);
    }
  });

  it("has a SHA-256 schema digest", () => {
    expect(HOST_CAPABILITY_DESCRIPTOR.schemaDigest).toMatch(/^sha256:[0-9a-f]{64}$/);
  });
});

describe("negotiateCapabilities", () => {
  it("rejects when runtime protocol version does not match", () => {
    const runtimeDescriptor: CapabilityDescriptor = {
      ...HOST_CAPABILITY_DESCRIPTOR,
      protocolVersion: "_omp/desktop/v2",
    };
    const result = negotiateCapabilities(HOST_CAPABILITY_DESCRIPTOR, runtimeDescriptor);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.errorCode).toBe("PROTOCOL_VERSION_MISMATCH");
    }
  });

  it("succeeds when both sides advertise v1 with matching schema digest", () => {
    const result = negotiateCapabilities(HOST_CAPABILITY_DESCRIPTOR, HOST_CAPABILITY_DESCRIPTOR);
    expect(result.ok).toBe(true);
  });
});
```

- [ ] **Step 2: Run the test and verify it fails**

Run:

```bash
pnpm test -- src/lib/protocol/capability.test.ts
```

Expected: FAIL because `./capability` module does not exist.

- [ ] **Step 3: Implement the TypeScript capability descriptor and negotiation**

Create `src/lib/protocol/capability.ts`:

```ts
export interface ExtensionVersion {
  major: number;
  minor: number;
  patch: number;
}

export interface MethodDescriptor {
  name: string;
  required: boolean;
  deprecated: boolean;
  replacedBy?: string | null;
  minVersion?: string;
}

export interface NotificationDescriptor {
  name: string;
  required: boolean;
  deprecated: boolean;
  replacedBy?: string | null;
}

export interface ExtensionFeature {
  name: string;
  required: boolean;
  available: boolean;
  reasonCode?: string | null;
}

export interface ProtocolLimits {
  maxFrameBytes: number;
  maxReassembledFrameBytes: number;
  maxPageSize: number;
  maxQueueDepth: number;
}

export interface CapabilityDescriptor {
  protocolVersion: string;
  extensionVersion: ExtensionVersion;
  methods: MethodDescriptor[];
  notifications: NotificationDescriptor[];
  features: {
    core: string[];
    extensions: ExtensionFeature[];
  };
  schemaDigest: string;
  limits: ProtocolLimits;
}

const V1_METHODS: MethodDescriptor[] = [
  { name: "_omp/desktop/v1/initialize", required: true, deprecated: false, replacedBy: null },
  { name: "_omp/desktop/v1/queue/enqueue", required: true, deprecated: false, replacedBy: null },
  { name: "_omp/desktop/v1/queue/dequeue", required: true, deprecated: false, replacedBy: null },
  { name: "_omp/desktop/v1/queue/query", required: true, deprecated: false, replacedBy: null },
  { name: "_omp/desktop/v1/steer", required: true, deprecated: false, replacedBy: null },
  { name: "_omp/desktop/v1/provider/list", required: true, deprecated: false, replacedBy: null },
  { name: "_omp/desktop/v1/model/list", required: true, deprecated: false, replacedBy: null },
  { name: "_omp/desktop/v1/credential/list", required: true, deprecated: false, replacedBy: null },
  { name: "_omp/desktop/v1/credential/beginAuth", required: true, deprecated: false, replacedBy: null },
  { name: "_omp/desktop/v1/credential/completeAuth", required: true, deprecated: false, replacedBy: null },
  { name: "_omp/desktop/v1/credential/revoke", required: true, deprecated: false, replacedBy: null },
  { name: "_omp/desktop/v1/credential/health", required: true, deprecated: false, replacedBy: null },
  { name: "_omp/desktop/v1/mcp/listSources", required: true, deprecated: false, replacedBy: null },
  { name: "_omp/desktop/v1/mcp/discoverTools", required: true, deprecated: false, replacedBy: null },
  { name: "_omp/desktop/v1/event/replay", required: true, deprecated: false, replacedBy: null },
  { name: "_omp/desktop/v1/session/config", required: true, deprecated: false, replacedBy: null },
];

const V1_NOTIFICATIONS: NotificationDescriptor[] = [
  { name: "_omp/desktop/v1/queue/updated", required: true, deprecated: false, replacedBy: null },
  { name: "_omp/desktop/v1/queue/dequeued", required: true, deprecated: false, replacedBy: null },
  { name: "_omp/desktop/v1/steer/ack", required: true, deprecated: false, replacedBy: null },
  { name: "_omp/desktop/v1/event/journal", required: true, deprecated: false, replacedBy: null },
];

const V1_EXTENSIONS: ExtensionFeature[] = [
  { name: "queue_steer", required: true, available: false, reasonCode: "runtime_unavailable" },
  { name: "provider_model_credential", required: true, available: false, reasonCode: "runtime_unavailable" },
  { name: "mcp_discovery", required: true, available: false, reasonCode: "runtime_unavailable" },
  { name: "todo", required: true, available: false, reasonCode: "runtime_unavailable" },
  { name: "subagent", required: true, available: false, reasonCode: "runtime_unavailable" },
  { name: "branch_checkpoint_rewind", required: true, available: false, reasonCode: "runtime_unavailable" },
  { name: "usage_compaction", required: true, available: false, reasonCode: "runtime_unavailable" },
  { name: "attachment", required: true, available: false, reasonCode: "runtime_unavailable" },
  { name: "diagnostics", required: true, available: false, reasonCode: "runtime_unavailable" },
  { name: "event_replay_recovery", required: true, available: false, reasonCode: "runtime_unavailable" },
  { name: "message_localization", required: true, available: false, reasonCode: "runtime_unavailable" },
  { name: "thinking_visibility", required: true, available: false, reasonCode: "runtime_unavailable" },
  { name: "trace_correlation", required: true, available: false, reasonCode: "runtime_unavailable" },
];

export const HOST_CAPABILITY_DESCRIPTOR: CapabilityDescriptor = {
  protocolVersion: "_omp/desktop/v1",
  extensionVersion: { major: 1, minor: 0, patch: 0 },
  methods: V1_METHODS,
  notifications: V1_NOTIFICATIONS,
  features: {
    core: ["session", "prompt", "cancel", "tool", "permission", "elicitation"],
    extensions: V1_EXTENSIONS,
  },
  schemaDigest: "sha256:0000000000000000000000000000000000000000000000000000000000000000",
  limits: {
    maxFrameBytes: 1048576,
    maxReassembledFrameBytes: 67108864,
    maxPageSize: 256,
    maxQueueDepth: 64,
  },
};

export type NegotiationResult =
  | { ok: true; negotiatedVersion: string }
  | { ok: false; errorCode: string; messageKey: string };

export function negotiateCapabilities(
  host: CapabilityDescriptor,
  runtime: CapabilityDescriptor,
): NegotiationResult {
  if (host.protocolVersion !== runtime.protocolVersion) {
    return {
      ok: false,
      errorCode: "PROTOCOL_VERSION_MISMATCH",
      messageKey: "protocol.error.version_mismatch",
    };
  }
  if (host.extensionVersion.major !== runtime.extensionVersion.major) {
    return {
      ok: false,
      errorCode: "PROTOCOL_VERSION_MISMATCH",
      messageKey: "protocol.error.major_version_mismatch",
    };
  }
  return { ok: true, negotiatedVersion: host.protocolVersion };
}
```

- [ ] **Step 4: Implement the Rust capability descriptor**

Create `src-tauri/src/protocol/mod.rs`:

```rust
pub mod capability;
pub mod ids;
pub mod error;
pub mod compatibility;
pub mod pagination;
pub mod event_journal;
pub mod queue_steer;
pub mod provider_model_credential;
pub mod mcp_discovery;
pub mod localization;
pub mod thinking_visibility;
#[cfg(test)]
pub mod contract_tests;
#[cfg(test)]
pub mod audit;
```

Create `src-tauri/src/protocol/capability.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MethodDescriptor {
    pub name: String,
    pub required: bool,
    pub deprecated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaced_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NotificationDescriptor {
    pub name: String,
    pub required: bool,
    pub deprecated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaced_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionFeature {
    pub name: String,
    pub required: bool,
    pub available: bool,
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolLimits {
    pub max_frame_bytes: u64,
    pub max_reassembled_frame_bytes: u64,
    pub max_page_size: u32,
    pub max_queue_depth: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityDescriptor {
    pub protocol_version: String,
    pub extension_version: ExtensionVersion,
    pub methods: Vec<MethodDescriptor>,
    pub notifications: Vec<NotificationDescriptor>,
    pub features: CapabilityFeatures,
    pub schema_digest: String,
    pub limits: ProtocolLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityFeatures {
    pub core: Vec<String>,
    pub extensions: Vec<ExtensionFeature>,
}

pub fn host_capability_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor {
        protocol_version: "_omp/desktop/v1".to_string(),
        extension_version: ExtensionVersion { major: 1, minor: 0, patch: 0 },
        methods: vec![
            method("_omp/desktop/v1/initialize", true),
            method("_omp/desktop/v1/queue/enqueue", true),
            method("_omp/desktop/v1/queue/dequeue", true),
            method("_omp/desktop/v1/queue/query", true),
            method("_omp/desktop/v1/steer", true),
            method("_omp/desktop/v1/provider/list", true),
            method("_omp/desktop/v1/model/list", true),
            method("_omp/desktop/v1/credential/list", true),
            method("_omp/desktop/v1/credential/beginAuth", true),
            method("_omp/desktop/v1/credential/completeAuth", true),
            method("_omp/desktop/v1/credential/revoke", true),
            method("_omp/desktop/v1/credential/health", true),
            method("_omp/desktop/v1/mcp/listSources", true),
            method("_omp/desktop/v1/mcp/discoverTools", true),
            method("_omp/desktop/v1/event/replay", true),
            method("_omp/desktop/v1/session/config", true),
        ],
        notifications: vec![
            notification("_omp/desktop/v1/queue/updated", true),
            notification("_omp/desktop/v1/queue/dequeued", true),
            notification("_omp/desktop/v1/steer/ack", true),
            notification("_omp/desktop/v1/event/journal", true),
        ],
        features: CapabilityFeatures {
            core: vec![
                "session".to_string(),
                "prompt".to_string(),
                "cancel".to_string(),
                "tool".to_string(),
                "permission".to_string(),
                "elicitation".to_string(),
            ],
            extensions: v1_extensions(),
        },
        schema_digest: "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        limits: ProtocolLimits {
            max_frame_bytes: 1_048_576,
            max_reassembled_frame_bytes: 67_108_864,
            max_page_size: 256,
            max_queue_depth: 64,
        },
    }
}

fn method(name: &str, required: bool) -> MethodDescriptor {
    MethodDescriptor {
        name: name.to_string(),
        required,
        deprecated: false,
        replaced_by: None,
    }
}

fn notification(name: &str, required: bool) -> NotificationDescriptor {
    NotificationDescriptor {
        name: name.to_string(),
        required,
        deprecated: false,
        replaced_by: None,
    }
}

fn v1_extensions() -> Vec<ExtensionFeature> {
    let names = [
        "queue_steer",
        "provider_model_credential",
        "mcp_discovery",
        "todo",
        "subagent",
        "branch_checkpoint_rewind",
        "usage_compaction",
        "attachment",
        "diagnostics",
        "event_replay_recovery",
        "message_localization",
        "thinking_visibility",
        "trace_correlation",
    ];
    names
        .iter()
        .map(|name| ExtensionFeature {
            name: name.to_string(),
            required: true,
            available: false,
            reason_code: Some("runtime_unavailable".to_string()),
        })
        .collect()
}

pub enum NegotiationResult {
    Success { negotiated_version: String },
    Failure { error_code: String, message_key: String },
}

pub fn negotiate_capabilities(host: &CapabilityDescriptor, runtime: &CapabilityDescriptor) -> NegotiationResult {
    if host.protocol_version != runtime.protocol_version {
        return NegotiationResult::Failure {
            error_code: "PROTOCOL_VERSION_MISMATCH".to_string(),
            message_key: "protocol.error.version_mismatch".to_string(),
        };
    }
    if host.extension_version.major != runtime.extension_version.major {
        return NegotiationResult::Failure {
            error_code: "PROTOCOL_VERSION_MISMATCH".to_string(),
            message_key: "protocol.error.major_version_mismatch".to_string(),
        };
    }
    NegotiationResult::Success {
        negotiated_version: host.protocol_version.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_descriptor_advertises_v1_namespace() {
        let desc = host_capability_descriptor();
        assert_eq!(desc.protocol_version, "_omp/desktop/v1");
    }

    #[test]
    fn host_descriptor_has_all_required_extensions() {
        let desc = host_capability_descriptor();
        let names: Vec<&str> = desc.features.extensions.iter().map(|e| e.name.as_str()).collect();
        let required = [
            "queue_steer", "provider_model_credential", "mcp_discovery", "todo",
            "subagent", "branch_checkpoint_rewind", "usage_compaction", "attachment",
            "diagnostics", "event_replay_recovery", "message_localization",
            "thinking_visibility", "trace_correlation",
        ];
        for r in required {
            assert!(names.contains(&r), "missing required extension: {r}");
        }
    }

    #[test]
    fn negotiation_rejects_version_mismatch() {
        let host = host_capability_descriptor();
        let mut runtime = host.clone();
        runtime.protocol_version = "_omp/desktop/v2".to_string();
        match negotiate_capabilities(&host, &runtime) {
            NegotiationResult::Failure { error_code, .. } => {
                assert_eq!(error_code, "PROTOCOL_VERSION_MISMATCH");
            }
            _ => panic!("expected failure"),
        }
    }
}
```

- [ ] **Step 5: Register the protocol module in lib.rs**

Add to `src-tauri/src/lib.rs` after `mod runtime_availability;`:

```rust
mod protocol;
```

- [ ] **Step 6: Run tests and verify they pass**

Run:

```bash
pnpm test -- src/lib/protocol/capability.test.ts
cargo test --manifest-path src-tauri/Cargo.toml protocol::capability --locked
```

Expected: both PASS.

- [ ] **Step 7: Commit capability descriptor and handshake**

Run:

```bash
git add src-tauri/src/protocol/mod.rs src-tauri/src/protocol/capability.rs src-tauri/src/lib.rs src/lib/protocol/capability.ts src/lib/protocol/capability.test.ts
git commit -m "feat: implement capability descriptor and initialize handshake"
```

---

### Task 4: Implement Stable ID Generation and Validation

**Files:**
- Create: `src-tauri/src/protocol/ids.rs`
- Create: `src/lib/protocol/ids.ts`
- Test: `src/lib/protocol/ids.test.ts`

**Interfaces:**
- Consumes: `protocol/v1/ids.schema.json` from Task 2.
- Produces: Rust `StableId` types and validators; TypeScript `StableId` validators; all IDs use opaque hex with typed prefixes.

- [ ] **Step 1: Write the failing ID validation test**

Create `src/lib/protocol/ids.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  validateSessionId,
  validateTurnId,
  validateEventId,
  validateQueueReceiptId,
  validateCredentialRefId,
  validatePermissionRequestId,
  validateProjectId,
  validateModelId,
  validateMcpSourceId,
  SESSION_ID_PATTERN,
} from "./ids";

describe("stable ID validation", () => {
  it("accepts well-formed session IDs", () => {
    expect(validateSessionId("ses_0123456789abcdef")).toBe(true);
  });

  it("rejects session IDs with wrong prefix", () => {
    expect(validateSessionId("turn_0123456789abcdef")).toBe(false);
  });

  it("rejects session IDs with wrong length", () => {
    expect(validateSessionId("ses_short")).toBe(false);
  });

  it("accepts well-formed event IDs with 24 hex chars", () => {
    expect(validateEventId("evt_0123456789abcdef01234567")).toBe(true);
  });

  it("accepts well-formed queue receipt IDs", () => {
    expect(validateQueueReceiptId("q_0123456789abcdef")).toBe(true);
  });

  it("accepts well-formed credential ref IDs", () => {
    expect(validateCredentialRefId("cred_0123456789abcdef")).toBe(true);
  });

  it("accepts well-formed permission request IDs", () => {
    expect(validatePermissionRequestId("perm_0123456789abcdef")).toBe(true);
  });

  it("accepts well-formed project IDs", () => {
    expect(validateProjectId("proj_0123456789abcdef")).toBe(true);
  });

  it("accepts well-formed model IDs", () => {
    expect(validateModelId("mdl_grok-4.5")).toBe(true);
  });

  it("accepts well-formed MCP source IDs", () => {
    expect(validateMcpSourceId("mcp_0123456789abcdef")).toBe(true);
  });

  it("exposes the raw pattern for schema reuse", () => {
    expect(SESSION_ID_PATTERN.test("ses_abcdef0123456789")).toBe(true);
  });
});
```

- [ ] **Step 2: Run the test and verify it fails**

Run:

```bash
pnpm test -- src/lib/protocol/ids.test.ts
```

Expected: FAIL because `./ids` module does not exist.

- [ ] **Step 3: Implement the TypeScript ID validators**

Create `src/lib/protocol/ids.ts`:

```ts
export const SESSION_ID_PATTERN = /^ses_[0-9a-f]{16}$/;
export const TURN_ID_PATTERN = /^turn_[0-9a-f]{16}$/;
export const EVENT_ID_PATTERN = /^evt_[0-9a-f]{24}$/;
export const PERMISSION_REQUEST_ID_PATTERN = /^perm_[0-9a-f]{16}$/;
export const QUEUE_RECEIPT_ID_PATTERN = /^q_[0-9a-f]{16}$/;
export const CREDENTIAL_REF_ID_PATTERN = /^cred_[0-9a-f]{16}$/;
export const PROJECT_ID_PATTERN = /^proj_[0-9a-f]{16}$/;
export const MODEL_ID_PATTERN = /^mdl_[A-Za-z0-9_-]+$/;
export const MCP_SOURCE_ID_PATTERN = /^mcp_[0-9a-f]{16}$/;

export type SessionId = string;
export type TurnId = string;
export type EventId = string;
export type PermissionRequestId = string;
export type QueueReceiptId = string;
export type CredentialRefId = string;
export type ProjectId = string;
export type ModelId = string;
export type McpSourceId = string;

export function validateSessionId(id: string): boolean {
  return SESSION_ID_PATTERN.test(id);
}
export function validateTurnId(id: string): boolean {
  return TURN_ID_PATTERN.test(id);
}
export function validateEventId(id: string): boolean {
  return EVENT_ID_PATTERN.test(id);
}
export function validatePermissionRequestId(id: string): boolean {
  return PERMISSION_REQUEST_ID_PATTERN.test(id);
}
export function validateQueueReceiptId(id: string): boolean {
  return QUEUE_RECEIPT_ID_PATTERN.test(id);
}
export function validateCredentialRefId(id: string): boolean {
  return CREDENTIAL_REF_ID_PATTERN.test(id);
}
export function validateProjectId(id: string): boolean {
  return PROJECT_ID_PATTERN.test(id);
}
export function validateModelId(id: string): boolean {
  return MODEL_ID_PATTERN.test(id);
}
export function validateMcpSourceId(id: string): boolean {
  return MCP_SOURCE_ID_PATTERN.test(id);
}
```

- [ ] **Step 4: Implement the Rust stable ID types**

Create `src-tauri/src/protocol/ids.rs`:

```rust
use serde::{Deserialize, Serialize};

macro_rules! define_stable_id {
    ($name:ident, $prefix:literal, $hex_len:literal) => {
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
        #[serde(try_from = "String")]
        pub struct $name(String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn generate() -> Self {
                use uuid::Uuid;
                let uuid = Uuid::new_v4();
                let hex = uuid.simple();
                let hex_str = hex.to_string();
                let truncated = &hex_str[..$hex_len.min(hex_str.len())];
                Self(format!("{}{}", $prefix, truncated))
            }

            pub fn validate(s: &str) -> bool {
                let expected_len = $prefix.len() + $hex_len;
                s.len() == expected_len
                    && s.starts_with($prefix)
                    && s[$prefix.len()..].chars().all(|c| c.is_ascii_hexdigit())
            }
        }

        impl TryFrom<String> for $name {
            type Error = String;
            fn try_from(s: String) -> Result<Self, Self::Error> {
                if Self::validate(&s) {
                    Ok(Self(s))
                } else {
                    Err(format!(
                        "invalid {}: expected {} followed by {} hex chars, got: {}",
                        stringify!($name), $prefix, $hex_len, s
                    ))
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

define_stable_id!(SessionId, "ses_", 16);
define_stable_id!(TurnId, "turn_", 16);
define_stable_id!(EventId, "evt_", 24);
define_stable_id!(PermissionRequestId, "perm_", 16);
define_stable_id!(QueueReceiptId, "q_", 16);
define_stable_id!(CredentialRefId, "cred_", 16);
define_stable_id!(ProjectId, "proj_", 16);
define_stable_id!(McpSourceId, "mcp_", 16);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(try_from = "String")]
pub struct ModelId(String);

impl ModelId {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn validate(s: &str) -> bool {
        s.starts_with("mdl_")
            && s[4..].chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            && s.len() > 4
    }
}

impl TryFrom<String> for ModelId {
    type Error = String;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        if Self::validate(&s) {
            Ok(Self(s))
        } else {
            Err(format!("invalid ModelId: {s}"))
        }
    }
}

impl std::fmt::Display for ModelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_generate_and_validate() {
        let id = SessionId::generate();
        assert!(SessionId::validate(id.as_str()));
        assert!(id.as_str().starts_with("ses_"));
    }

    #[test]
    fn session_id_rejects_wrong_prefix() {
        assert!(!SessionId::validate("turn_0123456789abcdef"));
    }

    #[test]
    fn event_id_has_24_hex_chars() {
        let id = EventId::generate();
        assert!(id.as_str().starts_with("evt_"));
        assert_eq!(id.as_str().len(), "evt_".len() + 24);
    }

    #[test]
    fn model_id_accepts_dashes() {
        assert!(ModelId::validate("mdl_grok-4.5"));
    }

    #[test]
    fn model_id_rejects_empty_suffix() {
        assert!(!ModelId::validate("mdl_"));
    }

    #[test]
    fn try_from_rejects_invalid() {
        assert!(SessionId::try_from("bad".to_string()).is_err());
    }
}
```

- [ ] **Step 5: Run tests and verify they pass**

Run:

```bash
pnpm test -- src/lib/protocol/ids.test.ts
cargo test --manifest-path src-tauri/Cargo.toml protocol::ids --locked
```

Expected: both PASS.

- [ ] **Step 6: Commit stable ID implementation**

Run:

```bash
git add src-tauri/src/protocol/ids.rs src/lib/protocol/ids.ts src/lib/protocol/ids.test.ts
git commit -m "feat: implement stable ID generation and validation"
```

---

### Task 5: Implement Stable Error Code Registry with messageKey + args

**Files:**
- Create: `src-tauri/src/protocol/error.rs`
- Create: `src/lib/protocol/error.ts`
- Test: `src/lib/protocol/error.test.ts`

**Interfaces:**
- Consumes: `protocol/v1/error.schema.json` from Task 2.
- Produces: Rust `ProtocolError` struct with stable codes; TypeScript `ProtocolError` type and registry; all errors include `messageKey + args`, `retryable`, `recoverable`, and `technicalDetail` boundary.

- [ ] **Step 1: Write the failing error registry test**

Create `src/lib/protocol/error.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  ProtocolError,
  PROTOCOL_ERRORS,
  runtimeUnavailableError,
  protocolVersionMismatchError,
  staleCursorError,
  sessionBusyError,
  steerTooLateError,
  credentialNotFoundError,
  type ProtocolErrorCode,
} from "./error";

describe("PROTOCOL_ERRORS registry", () => {
  it("includes runtime_unavailable error", () => {
    const err = PROTOCOL_ERRORS.RUNTIME_UNAVAILABLE;
    expect(err.code).toBe("RUNTIME_UNAVAILABLE");
    expect(err.category).toBe("runtime");
    expect(err.retryable).toBe(false);
    expect(err.recoverable).toBe(true);
    expect(err.messageKey).toBe("protocol.error.runtime_unavailable");
  });

  it("includes protocol_version_mismatch error", () => {
    const err = PROTOCOL_ERRORS.PROTOCOL_VERSION_MISMATCH;
    expect(err.category).toBe("protocol");
    expect(err.severity).toBe("fatal");
  });

  it("includes stale_cursor error", () => {
    const err = PROTOCOL_ERRORS.STALE_CURSOR;
    expect(err.category).toBe("session");
    expect(err.retryable).toBe(true);
  });

  it("includes session_busy error", () => {
    const err = PROTOCOL_ERRORS.SESSION_BUSY;
    expect(err.category).toBe("session");
    expect(err.retryable).toBe(true);
  });

  it("includes steer_too_late error", () => {
    const err = PROTOCOL_ERRORS.STEER_TOO_LATE;
    expect(err.category).toBe("steer");
    expect(err.retryable).toBe(false);
  });

  it("includes credential_not_found error", () => {
    const err = PROTOCOL_ERRORS.CREDENTIAL_NOT_FOUND;
    expect(err.category).toBe("credential");
    expect(err.retryable).toBe(false);
  });
});

describe("error factory functions", () => {
  it("runtimeUnavailableError sets correct args", () => {
    const err = runtimeUnavailableError();
    expect(err.code).toBe("RUNTIME_UNAVAILABLE");
    expect(err.args).toEqual({});
    expect(err.technicalDetail).toBeNull();
  });

  it("protocolVersionMismatchError includes host and runtime versions", () => {
    const err = protocolVersionMismatchError("1.0.0", "2.0.0");
    expect(err.args).toEqual({ hostVersion: "1.0.0", runtimeVersion: "2.0.0" });
  });

  it("staleCursorError includes snapshot info", () => {
    const err = staleCursorError("ses_0123456789abcdef", 42);
    expect(err.args).toEqual({ sessionId: "ses_0123456789abcdef", commitSeq: 42 });
  });

  it("steerTooLateError includes target turn", () => {
    const err = steerTooLateError("turn_0123456789abcdef");
    expect(err.args).toEqual({ targetTurnId: "turn_0123456789abcdef" });
  });
});
```

- [ ] **Step 2: Run the test and verify it fails**

Run:

```bash
pnpm test -- src/lib/protocol/error.test.ts
```

Expected: FAIL because `./error` module does not exist.

- [ ] **Step 3: Implement the TypeScript error registry**

Create `src/lib/protocol/error.ts`:

```ts
export type ErrorCategory =
  | "protocol" | "runtime" | "provider" | "credential" | "permission"
  | "session" | "queue" | "steer" | "mcp" | "config" | "transport" | "internal";

export type ErrorSeverity = "fatal" | "error" | "warning" | "info";

export type RecoveryAction =
  | "retry" | "reconnect" | "reconfigure" | "contact_support"
  | "restart_session" | "restart_runtime" | "none";

export interface ProtocolError {
  code: string;
  category: ErrorCategory;
  severity: ErrorSeverity;
  retryable: boolean;
  recoverable: boolean;
  messageKey: string;
  args: Record<string, string | number | boolean | null>;
  technicalDetail: string | null;
  recoveryActions: RecoveryAction[];
}

export type ProtocolErrorCode = keyof typeof PROTOCOL_ERRORS;

interface ErrorTemplate {
  code: string;
  category: ErrorCategory;
  severity: ErrorSeverity;
  retryable: boolean;
  recoverable: boolean;
  messageKey: string;
  recoveryActions: RecoveryAction[];
}

export const PROTOCOL_ERRORS = {
  RUNTIME_UNAVAILABLE: {
    code: "RUNTIME_UNAVAILABLE",
    category: "runtime" as const,
    severity: "error" as const,
    retryable: false,
    recoverable: true,
    messageKey: "protocol.error.runtime_unavailable",
    recoveryActions: ["restart_runtime" as const],
  },
  PROTOCOL_VERSION_MISMATCH: {
    code: "PROTOCOL_VERSION_MISMATCH",
    category: "protocol" as const,
    severity: "fatal" as const,
    retryable: false,
    recoverable: false,
    messageKey: "protocol.error.version_mismatch",
    recoveryActions: ["reconfigure" as const],
  },
  STALE_CURSOR: {
    code: "STALE_CURSOR",
    category: "session" as const,
    severity: "warning" as const,
    retryable: true,
    recoverable: true,
    messageKey: "protocol.error.stale_cursor",
    recoveryActions: ["retry" as const],
  },
  SESSION_BUSY: {
    code: "SESSION_BUSY",
    category: "session" as const,
    severity: "warning" as const,
    retryable: true,
    recoverable: true,
    messageKey: "protocol.error.session_busy",
    recoveryActions: ["retry" as const],
  },
  STEER_TOO_LATE: {
    code: "STEER_TOO_LATE",
    category: "steer" as const,
    severity: "error" as const,
    retryable: false,
    recoverable: true,
    messageKey: "protocol.error.steer_too_late",
    recoveryActions: ["none" as const],
  },
  STEER_CONFLICT: {
    code: "STEER_CONFLICT",
    category: "steer" as const,
    severity: "error" as const,
    retryable: false,
    recoverable: true,
    messageKey: "protocol.error.steer_conflict",
    recoveryActions: ["retry" as const],
  },
  CREDENTIAL_NOT_FOUND: {
    code: "CREDENTIAL_NOT_FOUND",
    category: "credential" as const,
    severity: "error" as const,
    retryable: false,
    recoverable: true,
    messageKey: "protocol.error.credential_not_found",
    recoveryActions: ["reconfigure" as const],
  },
  CREDENTIAL_EXPIRED: {
    code: "CREDENTIAL_EXPIRED",
    category: "credential" as const,
    severity: "error" as const,
    retryable: false,
    recoverable: true,
    messageKey: "protocol.error.credential_expired",
    recoveryActions: ["reconfigure" as const],
  },
  QUEUE_FULL: {
    code: "QUEUE_FULL",
    category: "queue" as const,
    severity: "error" as const,
    retryable: true,
    recoverable: true,
    messageKey: "protocol.error.queue_full",
    recoveryActions: ["retry" as const],
  },
  PERMISSION_DENIED: {
    code: "PERMISSION_DENIED",
    category: "permission" as const,
    severity: "error" as const,
    retryable: false,
    recoverable: true,
    messageKey: "protocol.error.permission_denied",
    recoveryActions: ["none" as const],
  },
  MCP_CONNECTION_FAILED: {
    code: "MCP_CONNECTION_FAILED",
    category: "mcp" as const,
    severity: "error" as const,
    retryable: true,
    recoverable: true,
    messageKey: "protocol.error.mcp_connection_failed",
    recoveryActions: ["reconnect" as const],
  },
  EVENT_JOURNAL_GAP: {
    code: "EVENT_JOURNAL_GAP",
    category: "session" as const,
    severity: "error" as const,
    retryable: true,
    recoverable: true,
    messageKey: "protocol.error.event_journal_gap",
    recoveryActions: ["reconnect" as const, "restart_session" as const],
  },
} satisfies Record<string, ErrorTemplate>;

function fromTemplate(template: ErrorTemplate, args: Record<string, string | number | boolean | null> = {}, technicalDetail: string | null = null): ProtocolError {
  return {
    code: template.code,
    category: template.category,
    severity: template.severity,
    retryable: template.retryable,
    recoverable: template.recoverable,
    messageKey: template.messageKey,
    args,
    technicalDetail,
    recoveryActions: template.recoveryActions,
  };
}

export function runtimeUnavailableError(): ProtocolError {
  return fromTemplate(PROTOCOL_ERRORS.RUNTIME_UNAVAILABLE);
}

export function protocolVersionMismatchError(hostVersion: string, runtimeVersion: string): ProtocolError {
  return fromTemplate(PROTOCOL_ERRORS.PROTOCOL_VERSION_MISMATCH, { hostVersion, runtimeVersion });
}

export function staleCursorError(sessionId: string, commitSeq: number): ProtocolError {
  return fromTemplate(PROTOCOL_ERRORS.STALE_CURSOR, { sessionId, commitSeq });
}

export function sessionBusyError(sessionId: string): ProtocolError {
  return fromTemplate(PROTOCOL_ERRORS.SESSION_BUSY, { sessionId });
}

export function steerTooLateError(targetTurnId: string): ProtocolError {
  return fromTemplate(PROTOCOL_ERRORS.STEER_TOO_LATE, { targetTurnId });
}

export function steerConflictError(targetTurnId: string): ProtocolError {
  return fromTemplate(PROTOCOL_ERRORS.STEER_CONFLICT, { targetTurnId });
}

export function credentialNotFoundError(credentialRefId: string): ProtocolError {
  return fromTemplate(PROTOCOL_ERRORS.CREDENTIAL_NOT_FOUND, { credentialRefId });
}

export function queueFullError(maxDepth: number): ProtocolError {
  return fromTemplate(PROTOCOL_ERRORS.QUEUE_FULL, { maxDepth });
}

export function eventJournalGapError(sessionId: string, lastKnownEventId: string): ProtocolError {
  return fromTemplate(PROTOCOL_ERRORS.EVENT_JOURNAL_GAP, { sessionId, lastKnownEventId });
}
```

- [ ] **Step 4: Implement the Rust error registry**

Create `src-tauri/src/protocol/error.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ErrorCategory {
    Protocol,
    Runtime,
    Provider,
    Credential,
    Permission,
    Session,
    Queue,
    Steer,
    Mcp,
    Config,
    Transport,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ErrorSeverity {
    Fatal,
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryAction {
    Retry,
    Reconnect,
    Reconfigure,
    ContactSupport,
    RestartSession,
    RestartRuntime,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolError {
    pub code: String,
    pub category: ErrorCategory,
    pub severity: ErrorSeverity,
    pub retryable: bool,
    pub recoverable: bool,
    pub message_key: String,
    pub args: serde_json::Value,
    pub technical_detail: Option<String>,
    pub recovery_actions: Vec<RecoveryAction>,
}

impl ProtocolError {
    pub fn new(code: &str, category: ErrorCategory, severity: ErrorSeverity, retryable: bool, recoverable: bool, message_key: &str) -> Self {
        Self {
            code: code.to_string(),
            category,
            severity,
            retryable,
            recoverable,
            message_key: message_key.to_string(),
            args: serde_json::json!({}),
            technical_detail: None,
            recovery_actions: vec![],
        }
    }

    pub fn with_args(mut self, args: serde_json::Value) -> Self {
        self.args = args;
        self
    }

    pub fn with_technical_detail(mut self, detail: impl Into<String>) -> Self {
        self.technical_detail = Some(detail.into());
        self
    }

    pub fn with_recovery_actions(mut self, actions: Vec<RecoveryAction>) -> Self {
        self.recovery_actions = actions;
        self
    }
}

pub fn runtime_unavailable() -> ProtocolError {
    ProtocolError::new(
        "RUNTIME_UNAVAILABLE",
        ErrorCategory::Runtime,
        ErrorSeverity::Error,
        false,
        true,
        "protocol.error.runtime_unavailable",
    )
    .with_recovery_actions(vec![RecoveryAction::RestartRuntime])
}

pub fn protocol_version_mismatch(host_version: &str, runtime_version: &str) -> ProtocolError {
    ProtocolError::new(
        "PROTOCOL_VERSION_MISMATCH",
        ErrorCategory::Protocol,
        ErrorSeverity::Fatal,
        false,
        false,
        "protocol.error.version_mismatch",
    )
    .with_args(serde_json::json!({
        "hostVersion": host_version,
        "runtimeVersion": runtime_version,
    }))
    .with_recovery_actions(vec![RecoveryAction::Reconfigure])
}

pub fn stale_cursor(session_id: &str, commit_seq: u64) -> ProtocolError {
    ProtocolError::new(
        "STALE_CURSOR",
        ErrorCategory::Session,
        ErrorSeverity::Warning,
        true,
        true,
        "protocol.error.stale_cursor",
    )
    .with_args(serde_json::json!({
        "sessionId": session_id,
        "commitSeq": commit_seq,
    }))
    .with_recovery_actions(vec![RecoveryAction::Retry])
}

pub fn session_busy(session_id: &str) -> ProtocolError {
    ProtocolError::new(
        "SESSION_BUSY",
        ErrorCategory::Session,
        ErrorSeverity::Warning,
        true,
        true,
        "protocol.error.session_busy",
    )
    .with_args(serde_json::json!({ "sessionId": session_id }))
    .with_recovery_actions(vec![RecoveryAction::Retry])
}

pub fn steer_too_late(target_turn_id: &str) -> ProtocolError {
    ProtocolError::new(
        "STEER_TOO_LATE",
        ErrorCategory::Steer,
        ErrorSeverity::Error,
        false,
        true,
        "protocol.error.steer_too_late",
    )
    .with_args(serde_json::json!({ "targetTurnId": target_turn_id }))
    .with_recovery_actions(vec![RecoveryAction::None])
}

pub fn event_journal_gap(session_id: &str, last_known_event_id: &str) -> ProtocolError {
    ProtocolError::new(
        "EVENT_JOURNAL_GAP",
        ErrorCategory::Session,
        ErrorSeverity::Error,
        true,
        true,
        "protocol.error.event_journal_gap",
    )
    .with_args(serde_json::json!({
        "sessionId": session_id,
        "lastKnownEventId": last_known_event_id,
    }))
    .with_recovery_actions(vec![RecoveryAction::Reconnect, RecoveryAction::RestartSession])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_unavailable_has_stable_code() {
        let err = runtime_unavailable();
        assert_eq!(err.code, "RUNTIME_UNAVAILABLE");
        assert!(!err.retryable);
        assert!(err.recoverable);
    }

    #[test]
    fn protocol_version_mismatch_is_fatal() {
        let err = protocol_version_mismatch("1.0.0", "2.0.0");
        assert_eq!(err.severity, ErrorSeverity::Fatal);
        assert!(!err.recoverable);
    }

    #[test]
    fn stale_cursor_is_retryable() {
        let err = stale_cursor("ses_0123456789abcdef", 42);
        assert!(err.retryable);
    }

    #[test]
    fn serializes_to_camel_case() {
        let err = runtime_unavailable();
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"messageKey\""));
        assert!(json.contains("\"recoveryActions\""));
    }
}
```

- [ ] **Step 5: Run tests and verify they pass**

Run:

```bash
pnpm test -- src/lib/protocol/error.test.ts
cargo test --manifest-path src-tauri/Cargo.toml protocol::error --locked
```

Expected: both PASS.

- [ ] **Step 6: Commit the error registry**

Run:

```bash
git add src-tauri/src/protocol/error.rs src/lib/protocol/error.ts src/lib/protocol/error.test.ts
git commit -m "feat: implement stable error code registry with messageKey and args"
```

---

### Task 6: Implement Compatibility and Deprecation Rules

**Files:**
- Create: `src-tauri/src/protocol/compatibility.rs`
- Create: `src/lib/protocol/compatibility.ts`
- Test: `src/lib/protocol/compatibility.test.ts`

**Interfaces:**
- Consumes: `protocol/v1/compatibility.schema.json` from Task 2.
- Produces: Rust `CompatibilityRules` struct; TypeScript `CompatibilityRules` type; major/minor negotiation, unknown-field policy, deprecation cycle tracking.

- [ ] **Step 1: Write the failing compatibility test**

Create `src/lib/protocol/compatibility.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  negotiateVersion,
  isCompatible,
  handleUnknownField,
  getDeprecationStatus,
  HOST_COMPATIBILITY,
  type CompatibilityRules,
} from "./compatibility";

describe("negotiateVersion", () => {
  it("returns the lower minor version when majors match", () => {
    const result = negotiateVersion(
      { major: 1, minor: 2, patch: 0 },
      { major: 1, minor: 0, patch: 5 },
    );
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.negotiated).toEqual({ major: 1, minor: 0, patch: 5 });
    }
  });

  it("fails when majors differ", () => {
    const result = negotiateVersion(
      { major: 1, minor: 0, patch: 0 },
      { major: 2, minor: 0, patch: 0 },
    );
    expect(result.ok).toBe(false);
  });
});

describe("isCompatible", () => {
  it("returns true for same major version", () => {
    expect(isCompatible({ major: 1, minor: 0, patch: 0 }, { major: 1, minor: 5, patch: 0 })).toBe(true);
  });

  it("returns false for different major version", () => {
    expect(isCompatible({ major: 1, minor: 0, patch: 0 }, { major: 2, minor: 0, patch: 0 })).toBe(false);
  });
});

describe("handleUnknownField", () => {
  it("ignores unknown fields by default", () => {
    expect(handleUnknownField("ignore", "extraField")).toBe("ignored");
  });

  it("rejects unknown fields when policy is reject", () => {
    expect(handleUnknownField("reject", "extraField")).toBe("rejected");
  });

  it("warns on unknown fields when policy is warn", () => {
    expect(handleUnknownField("warn", "extraField")).toBe("warned");
  });
});

describe("getDeprecationStatus", () => {
  it("returns active for non-deprecated methods", () => {
    const deprecations: CompatibilityRules["deprecations"] = [];
    expect(getDeprecationStatus("_omp/desktop/v1/initialize", deprecations)).toBe("active");
  });

  it("returns deprecated for deprecated methods", () => {
    const deprecations = [
      { name: "_omp/desktop/v1/oldMethod", deprecatedIn: "1.0.0", removedIn: null, replacement: "_omp/desktop/v1/newMethod" },
    ];
    expect(getDeprecationStatus("_omp/desktop/v1/oldMethod", deprecations)).toBe("deprecated");
  });

  it("returns removed for methods past their removal version", () => {
    const deprecations = [
      { name: "_omp/desktop/v1/oldMethod", deprecatedIn: "0.9.0", removedIn: "1.0.0", replacement: null },
    ];
    expect(getDeprecationStatus("_omp/desktop/v1/oldMethod", deprecations)).toBe("removed");
  });
});

describe("HOST_COMPATIBILITY", () => {
  it("uses ignore policy for unknown fields", () => {
    expect(HOST_COMPATIBILITY.unknownFieldPolicy).toBe("ignore");
  });
});
```

- [ ] **Step 2: Run the test and verify it fails**

Run:

```bash
pnpm test -- src/lib/protocol/compatibility.test.ts
```

Expected: FAIL because `./compatibility` module does not exist.

- [ ] **Step 3: Implement the TypeScript compatibility rules**

Create `src/lib/protocol/compatibility.ts`:

```ts
export interface SemVer {
  major: number;
  minor: number;
  patch: number;
}

export type UnknownFieldPolicy = "ignore" | "reject" | "warn";

export interface Deprecation {
  name: string;
  deprecatedIn: string;
  removedIn: string | null;
  replacement: string | null;
}

export interface CompatibilityRules {
  hostVersion: SemVer;
  runtimeVersion: SemVer | null;
  negotiatedVersion: string;
  unknownFieldPolicy: UnknownFieldPolicy;
  deprecations: Deprecation[];
}

export type VersionNegotiationResult =
  | { ok: true; negotiated: SemVer }
  | { ok: false; reason: string };

export function negotiateVersion(host: SemVer, runtime: SemVer): VersionNegotiationResult {
  if (host.major !== runtime.major) {
    return {
      ok: false,
      reason: `Major version mismatch: host=${host.major}, runtime=${runtime.major}`,
    };
  }
  const minor = Math.min(host.minor, runtime.minor);
  const patch = Math.min(host.patch, runtime.patch);
  return { ok: true, negotiated: { major: host.major, minor, patch } };
}

export function isCompatible(host: SemVer, runtime: SemVer): boolean {
  return host.major === runtime.major;
}

export function handleUnknownField(policy: UnknownFieldPolicy, _fieldName: string): "ignored" | "rejected" | "warned" {
  switch (policy) {
    case "ignore":
      return "ignored";
    case "reject":
      return "rejected";
    case "warn":
      return "warned";
  }
}

export function getDeprecationStatus(name: string, deprecations: Deprecation[]): "active" | "deprecated" | "removed" {
  const dep = deprecations.find((d) => d.name === name);
  if (!dep) return "active";
  if (dep.removedIn !== null) return "removed";
  return "deprecated";
}

export const HOST_COMPATIBILITY: CompatibilityRules = {
  hostVersion: { major: 1, minor: 0, patch: 0 },
  runtimeVersion: null,
  negotiatedVersion: "_omp/desktop/v1",
  unknownFieldPolicy: "ignore",
  deprecations: [],
};
```

- [ ] **Step 4: Implement the Rust compatibility rules**

Create `src-tauri/src/protocol/compatibility.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SemVer {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UnknownFieldPolicy {
    Ignore,
    Reject,
    Warn,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Deprecation {
    pub name: String,
    pub deprecated_in: String,
    pub removed_in: Option<String>,
    pub replacement: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityRules {
    pub host_version: SemVer,
    pub runtime_version: Option<SemVer>,
    pub negotiated_version: String,
    pub unknown_field_policy: UnknownFieldPolicy,
    pub deprecations: Vec<Deprecation>,
}

pub enum VersionNegotiationResult {
    Success { negotiated: SemVer },
    Failure { reason: String },
}

pub fn negotiate_version(host: SemVer, runtime: SemVer) -> VersionNegotiationResult {
    if host.major != runtime.major {
        return VersionNegotiationResult::Failure {
            reason: format!("Major version mismatch: host={}, runtime={}", host.major, runtime.major),
        };
    }
    VersionNegotiationResult::Success {
        negotiated: SemVer {
            major: host.major,
            minor: host.minor.min(runtime.minor),
            patch: host.patch.min(runtime.patch),
        },
    }
}

pub fn is_compatible(host: SemVer, runtime: SemVer) -> bool {
    host.major == runtime.major
}

pub enum UnknownFieldResult {
    Ignored,
    Rejected,
    Warned,
}

pub fn handle_unknown_field(policy: UnknownFieldPolicy, _field_name: &str) -> UnknownFieldResult {
    match policy {
        UnknownFieldPolicy::Ignore => UnknownFieldResult::Ignored,
        UnknownFieldPolicy::Reject => UnknownFieldResult::Rejected,
        UnknownFieldPolicy::Warn => UnknownFieldResult::Warned,
    }
}

pub enum DeprecationStatus {
    Active,
    Deprecated,
    Removed,
}

pub fn get_deprecation_status(name: &str, deprecations: &[Deprecation]) -> DeprecationStatus {
    match deprecations.iter().find(|d| d.name == name) {
        None => DeprecationStatus::Active,
        Some(d) if d.removed_in.is_some() => DeprecationStatus::Removed,
        Some(_) => DeprecationStatus::Deprecated,
    }
}

pub fn host_compatibility() -> CompatibilityRules {
    CompatibilityRules {
        host_version: SemVer { major: 1, minor: 0, patch: 0 },
        runtime_version: None,
        negotiated_version: "_omp/desktop/v1".to_string(),
        unknown_field_policy: UnknownFieldPolicy::Ignore,
        deprecations: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negotiate_same_major_returns_lower_minor() {
        let result = negotiate_version(
            SemVer { major: 1, minor: 2, patch: 0 },
            SemVer { major: 1, minor: 0, patch: 5 },
        );
        match result {
            VersionNegotiationResult::Success { negotiated } => {
                assert_eq!(negotiated.minor, 0);
            }
            _ => panic!("expected success"),
        }
    }

    #[test]
    fn negotiate_different_major_fails() {
        let result = negotiate_version(
            SemVer { major: 1, minor: 0, patch: 0 },
            SemVer { major: 2, minor: 0, patch: 0 },
        );
        match result {
            VersionNegotiationResult::Failure { .. } => {}
            _ => panic!("expected failure"),
        }
    }

    #[test]
    fn deprecation_status_for_removed_method() {
        let deprecations = vec![Deprecation {
            name: "old".to_string(),
            deprecated_in: "0.9.0".to_string(),
            removed_in: Some("1.0.0".to_string()),
            replacement: None,
        }];
        match get_deprecation_status("old", &deprecations) {
            DeprecationStatus::Removed => {}
            _ => panic!("expected removed"),
        }
    }
}
```

- [ ] **Step 5: Run tests and verify they pass**

Run:

```bash
pnpm test -- src/lib/protocol/compatibility.test.ts
cargo test --manifest-path src-tauri/Cargo.toml protocol::compatibility --locked
```

Expected: both PASS.

- [ ] **Step 6: Commit compatibility rules**

Run:

```bash
git add src-tauri/src/protocol/compatibility.rs src/lib/protocol/compatibility.ts src/lib/protocol/compatibility.test.ts
git commit -m "feat: implement compatibility and deprecation rules"
```

---

### Task 7: Implement Cursor Pagination and Snapshot Boundaries

**Files:**
- Create: `src-tauri/src/protocol/pagination.rs`
- Create: `src/lib/protocol/pagination.ts`
- Test: `src/lib/protocol/pagination.test.ts`

**Interfaces:**
- Consumes: `protocol/v1/pagination.schema.json` from Task 2.
- Produces: Rust `CursorPage<T>` generic; TypeScript `CursorPage<T>` generic; opaque cursor encoding/decoding; snapshot boundary validation; stale cursor detection.

- [ ] **Step 1: Write the failing pagination test**

Create `src/lib/protocol/pagination.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  encodeCursor,
  decodeCursor,
  isStaleCursor,
  createPage,
  type CursorPage,
  type SnapshotBoundary,
} from "./pagination";

describe("cursor encoding", () => {
  it("encodes and decodes a cursor round-trip", () => {
    const boundary: SnapshotBoundary = { sessionId: "ses_0123456789abcdef", commitSeq: 42 };
    const cursor = encodeCursor(boundary, 10);
    expect(cursor).toBeTruthy();
    const decoded = decodeCursor(cursor);
    expect(decoded.ok).toBe(true);
    if (decoded.ok) {
      expect(decoded.boundary.sessionId).toBe("ses_0123456789abcdef");
      expect(decoded.boundary.commitSeq).toBe(42);
      expect(decoded.offset).toBe(10);
    }
  });

  it("rejects tampered cursors", () => {
    const decoded = decodeCursor("not-a-valid-cursor");
    expect(decoded.ok).toBe(false);
  });
});

describe("isStaleCursor", () => {
  it("returns true when commitSeq differs", () => {
    const boundary: SnapshotBoundary = { sessionId: "ses_0123456789abcdef", commitSeq: 42 };
    const cursor = encodeCursor(boundary, 0);
    expect(isStaleCursor(cursor, { sessionId: "ses_0123456789abcdef", commitSeq: 50 })).toBe(true);
  });

  it("returns false when boundary matches", () => {
    const boundary: SnapshotBoundary = { sessionId: "ses_0123456789abcdef", commitSeq: 42 };
    const cursor = encodeCursor(boundary, 10);
    expect(isStaleCursor(cursor, boundary)).toBe(false);
  });
});

describe("createPage", () => {
  it("creates a page with hasMore=false when no next cursor", () => {
    const items = [1, 2, 3];
    const page = createPage(items, false, null, { sessionId: "ses_0123456789abcdef", commitSeq: 0 });
    expect(page.items).toEqual(items);
    expect(page.hasMore).toBe(false);
    expect(page.nextCursor).toBeNull();
  });

  it("creates a page with hasMore=true and next cursor", () => {
    const items = [1, 2];
    const boundary: SnapshotBoundary = { sessionId: "ses_0123456789abcdef", commitSeq: 5 };
    const nextCursor = encodeCursor(boundary, 2);
    const page = createPage(items, true, nextCursor, boundary);
    expect(page.hasMore).toBe(true);
    expect(page.nextCursor).toBe(nextCursor);
  });
});
```

- [ ] **Step 2: Run the test and verify it fails**

Run:

```bash
pnpm test -- src/lib/protocol/pagination.test.ts
```

Expected: FAIL because `./pagination` module does not exist.

- [ ] **Step 3: Implement the TypeScript pagination**

Create `src/lib/protocol/pagination.ts`:

```ts
export interface SnapshotBoundary {
  sessionId: string;
  commitSeq: number;
}

export interface CursorPage<T> {
  items: T[];
  hasMore: boolean;
  nextCursor: string | null;
  snapshotBoundary: SnapshotBoundary;
  totalItems?: number | null;
}

interface CursorPayload {
  boundary: SnapshotBoundary;
  offset: number;
}

export function encodeCursor(boundary: SnapshotBoundary, offset: number): string {
  const payload: CursorPayload = { boundary, offset };
  const json = JSON.stringify(payload);
  return "cur_" + btoa(json);
}

export type CursorDecodeResult =
  | { ok: true; boundary: SnapshotBoundary; offset: number }
  | { ok: false; reason: string };

export function decodeCursor(cursor: string): CursorDecodeResult {
  if (!cursor.startsWith("cur_")) {
    return { ok: false, reason: "invalid cursor prefix" };
  }
  try {
    const json = atob(cursor.slice(4));
    const payload = JSON.parse(json) as CursorPayload;
    if (!payload.boundary || typeof payload.boundary.sessionId !== "string" || typeof payload.boundary.commitSeq !== "number") {
      return { ok: false, reason: "missing boundary fields" };
    }
    if (typeof payload.offset !== "number" || payload.offset < 0) {
      return { ok: false, reason: "invalid offset" };
    }
    return { ok: true, boundary: payload.boundary, offset: payload.offset };
  } catch {
    return { ok: false, reason: "cursor parse failed" };
  }
}

export function isStaleCursor(cursor: string, currentBoundary: SnapshotBoundary): boolean {
  const decoded = decodeCursor(cursor);
  if (!decoded.ok) return true;
  return (
    decoded.boundary.sessionId !== currentBoundary.sessionId ||
    decoded.boundary.commitSeq !== currentBoundary.commitSeq
  );
}

export function createPage<T>(
  items: T[],
  hasMore: boolean,
  nextCursor: string | null,
  boundary: SnapshotBoundary,
  totalItems?: number,
): CursorPage<T> {
  return {
    items,
    hasMore,
    nextCursor,
    snapshotBoundary: boundary,
    totalItems: totalItems ?? null,
  };
}
```

- [ ] **Step 4: Implement the Rust pagination**

Create `src-tauri/src/protocol/pagination.rs`:

```rust
use base64::{engine::general_purpose, Engine};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotBoundary {
    pub session_id: String,
    pub commit_seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CursorPage<T> {
    pub items: Vec<T>,
    pub has_more: bool,
    pub next_cursor: Option<String>,
    pub snapshot_boundary: SnapshotBoundary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_items: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CursorPayload {
    boundary: SnapshotBoundary,
    offset: u64,
}

pub enum CursorError {
    InvalidPrefix,
    DecodeFailed,
    InvalidPayload,
}

pub fn encode_cursor(boundary: &SnapshotBoundary, offset: u64) -> String {
    let payload = CursorPayload {
        boundary: boundary.clone(),
        offset,
    };
    let json = serde_json::to_string(&payload).unwrap_or_default();
    let encoded = general_purpose::STANDARD.encode(json.as_bytes());
    format!("cur_{}", encoded)
}

pub fn decode_cursor(cursor: &str) -> Result<(SnapshotBoundary, u64), CursorError> {
    if !cursor.starts_with("cur_") {
        return Err(CursorError::InvalidPrefix);
    }
    let encoded = &cursor[4..];
    let bytes = general_purpose::STANDARD.decode(encoded).map_err(|_| CursorError::DecodeFailed)?;
    let payload: CursorPayload = serde_json::from_slice(&bytes).map_err(|_| CursorError::InvalidPayload)?;
    Ok((payload.boundary, payload.offset))
}

pub fn is_stale_cursor(cursor: &str, current_boundary: &SnapshotBoundary) -> bool {
    match decode_cursor(cursor) {
        Ok((boundary, _)) => boundary != *current_boundary,
        Err(_) => true,
    }
}

impl<T> CursorPage<T> {
    pub fn new(
        items: Vec<T>,
        has_more: bool,
        next_cursor: Option<String>,
        boundary: SnapshotBoundary,
    ) -> Self {
        Self {
            items,
            has_more,
            next_cursor,
            snapshot_boundary: boundary,
            total_items: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_round_trip() {
        let boundary = SnapshotBoundary {
            session_id: "ses_0123456789abcdef".to_string(),
            commit_seq: 42,
        };
        let cursor = encode_cursor(&boundary, 10);
        let (decoded_boundary, offset) = decode_cursor(&cursor).unwrap();
        assert_eq!(decoded_boundary, boundary);
        assert_eq!(offset, 10);
    }

    #[test]
    fn invalid_cursor_rejected() {
        assert!(matches!(decode_cursor("bad"), Err(CursorError::InvalidPrefix)));
    }

    #[test]
    fn stale_cursor_detected() {
        let boundary = SnapshotBoundary {
            session_id: "ses_0123456789abcdef".to_string(),
            commit_seq: 42,
        };
        let cursor = encode_cursor(&boundary, 0);
        let different = SnapshotBoundary {
            session_id: "ses_0123456789abcdef".to_string(),
            commit_seq: 50,
        };
        assert!(is_stale_cursor(&cursor, &different));
        assert!(!is_stale_cursor(&cursor, &boundary));
    }
}
```

- [ ] **Step 5: Run tests and verify they pass**

Run:

```bash
pnpm test -- src/lib/protocol/pagination.test.ts
cargo test --manifest-path src-tauri/Cargo.toml protocol::pagination --locked
```

Expected: both PASS.

- [ ] **Step 6: Commit cursor pagination**

Run:

```bash
git add src-tauri/src/protocol/pagination.rs src/lib/protocol/pagination.ts src/lib/protocol/pagination.test.ts
git commit -m "feat: implement cursor pagination and snapshot boundaries"
```

---

### Task 8: Implement Event Journal Replay with Stable Event IDs

**Files:**
- Create: `src-tauri/src/protocol/event_journal.rs`
- Create: `src/lib/protocol/eventJournal.ts`
- Test: `src/lib/protocol/eventJournal.test.ts`

**Interfaces:**
- Consumes: `protocol/v1/event-journal.schema.json` from Task 2.
- Produces: Rust `JournalEvent` struct and replay logic; TypeScript `JournalEvent` type and replay helpers; stable event IDs that survive connection restart; replay cursor, journal commit point, active turn status; gap detection and duplicate delivery handling.

- [ ] **Step 1: Write the failing event journal test**

Create `src/lib/protocol/eventJournal.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  type JournalEvent,
  type ReplayState,
  type ActiveTurnStatus,
  createReplayState,
  applyEvent,
  detectGap,
  isDuplicate,
  getReplayCursor,
  type CommitPoint,
} from "./eventJournal";

describe("createReplayState", () => {
  it("starts with empty journal and unknown active turn", () => {
    const state = createReplayState("ses_0123456789abcdef");
    expect(state.sessionId).toBe("ses_0123456789abcdef");
    expect(state.events).toEqual([]);
    expect(state.lastEventId).toBeNull();
    expect(state.lastCommitSeq).toBe(0);
    expect(state.activeTurnStatus).toBe("unknown");
  });
});

describe("applyEvent", () => {
  it("appends an event and updates lastEventId", () => {
    let state = createReplayState("ses_0123456789abcdef");
    const event: JournalEvent = {
      eventId: "evt_0123456789abcdef01234567",
      sessionId: "ses_0123456789abcdef",
      turnId: "turn_0123456789abcdef",
      eventType: "agent_start",
      timestamp: "2026-07-29T00:00:00.000Z",
      seq: 0,
      replayCursor: null,
      commitPoint: true,
    };
    state = applyEvent(state, event);
    expect(state.events).toHaveLength(1);
    expect(state.lastEventId).toBe("evt_0123456789abcdef01234567");
    expect(state.lastCommitSeq).toBe(0);
  });

  it("updates active turn status on agent_start", () => {
    let state = createReplayState("ses_0123456789abcdef");
    state = applyEvent(state, {
      eventId: "evt_0123456789abcdef01234567",
      sessionId: "ses_0123456789abcdef",
      turnId: "turn_0123456789abcdef",
      eventType: "agent_start",
      timestamp: "2026-07-29T00:00:00.000Z",
      seq: 0,
      replayCursor: null,
      commitPoint: false,
    });
    expect(state.activeTurnStatus).toBe("active");
    expect(state.activeTurnId).toBe("turn_0123456789abcdef");
  });

  it("updates active turn status on agent_end", () => {
    let state = createReplayState("ses_0123456789abcdef");
    state = applyEvent(state, {
      eventId: "evt_0123456789abcdef01234567",
      sessionId: "ses_0123456789abcdef",
      turnId: "turn_0123456789abcdef",
      eventType: "agent_start",
      timestamp: "2026-07-29T00:00:00.000Z",
      seq: 0,
      replayCursor: null,
      commitPoint: false,
    });
    state = applyEvent(state, {
      eventId: "evt_0123456789abcdef01234568",
      sessionId: "ses_0123456789abcdef",
      turnId: "turn_0123456789abcdef",
      eventType: "agent_end",
      timestamp: "2026-07-29T00:01:00.000Z",
      seq: 1,
      replayCursor: "cursor_abc",
      commitPoint: true,
    });
    expect(state.activeTurnStatus).toBe("completed");
    expect(state.activeTurnId).toBeNull();
  });
});

describe("detectGap", () => {
  it("returns false for consecutive events", () => {
    let state = createReplayState("ses_0123456789abcdef");
    state = applyEvent(state, {
      eventId: "evt_0123456789abcdef01234567",
      sessionId: "ses_0123456789abcdef",
      turnId: null,
      eventType: "test",
      timestamp: "2026-07-29T00:00:00.000Z",
      seq: 0,
      replayCursor: null,
      commitPoint: false,
    });
    expect(detectGap(state, 1)).toBe(false);
  });

  it("returns true for a gap in sequence", () => {
    let state = createReplayState("ses_0123456789abcdef");
    expect(detectGap(state, 5)).toBe(true);
  });
});

describe("isDuplicate", () => {
  it("returns true for an already-seen event ID", () => {
    let state = createReplayState("ses_0123456789abcdef");
    const event: JournalEvent = {
      eventId: "evt_0123456789abcdef01234567",
      sessionId: "ses_0123456789abcdef",
      turnId: null,
      eventType: "test",
      timestamp: "2026-07-29T00:00:00.000Z",
      seq: 0,
      replayCursor: null,
      commitPoint: false,
    };
    state = applyEvent(state, event);
    expect(isDuplicate(state, event.eventId)).toBe(true);
  });

  it("returns false for a new event ID", () => {
    const state = createReplayState("ses_0123456789abcdef");
    expect(isDuplicate(state, "evt_new0123456789abcdef0123456")).toBe(false);
  });
});

describe("getReplayCursor", () => {
  it("returns the last commit point cursor", () => {
    let state = createReplayState("ses_0123456789abcdef");
    state = applyEvent(state, {
      eventId: "evt_0123456789abcdef01234567",
      sessionId: "ses_0123456789abcdef",
      turnId: null,
      eventType: "test",
      timestamp: "2026-07-29T00:00:00.000Z",
      seq: 0,
      replayCursor: "cursor_first",
      commitPoint: true,
    });
    state = applyEvent(state, {
      eventId: "evt_0123456789abcdef01234568",
      sessionId: "ses_0123456789abcdef",
      turnId: null,
      eventType: "test",
      timestamp: "2026-07-29T00:01:00.000Z",
      seq: 1,
      replayCursor: "cursor_second",
      commitPoint: false,
    });
    expect(getReplayCursor(state)).toBe("cursor_first");
  });
});
```

- [ ] **Step 2: Run the test and verify it fails**

Run:

```bash
pnpm test -- src/lib/protocol/eventJournal.test.ts
```

Expected: FAIL because `./eventJournal` module does not exist.

- [ ] **Step 3: Implement the TypeScript event journal**

Create `src/lib/protocol/eventJournal.ts`:

```ts
export type ActiveTurnStatus = "unknown" | "active" | "completed" | "interrupted";

export interface JournalEvent {
  eventId: string;
  sessionId: string;
  turnId: string | null;
  eventType: string;
  timestamp: string;
  seq: number;
  replayCursor: string | null;
  commitPoint: boolean | null;
}

export interface CommitPoint {
  eventId: string;
  replayCursor: string;
  seq: number;
}

export interface ReplayState {
  sessionId: string;
  events: JournalEvent[];
  seenEventIds: Set<string>;
  lastEventId: string | null;
  lastSeq: number;
  lastCommitSeq: number;
  lastCommitCursor: string | null;
  activeTurnStatus: ActiveTurnStatus;
  activeTurnId: string | null;
}

export function createReplayState(sessionId: string): ReplayState {
  return {
    sessionId,
    events: [],
    seenEventIds: new Set(),
    lastEventId: null,
    lastSeq: -1,
    lastCommitSeq: 0,
    lastCommitCursor: null,
    activeTurnStatus: "unknown",
    activeTurnId: null,
  };
}

export function applyEvent(state: ReplayState, event: JournalEvent): ReplayState {
  const newSeen = new Set(state.seenEventIds);
  newSeen.add(event.eventId);
  const newState: ReplayState = {
    ...state,
    events: [...state.events, event],
    seenEventIds: newSeen,
    lastEventId: event.eventId,
    lastSeq: event.seq,
    lastCommitSeq: event.commitPoint === true ? event.seq : state.lastCommitSeq,
    lastCommitCursor: event.commitPoint === true && event.replayCursor ? event.replayCursor : state.lastCommitCursor,
  };
  if (event.eventType === "agent_start" && event.turnId) {
    newState.activeTurnStatus = "active";
    newState.activeTurnId = event.turnId;
  } else if (event.eventType === "agent_end") {
    newState.activeTurnStatus = "completed";
    newState.activeTurnId = null;
  } else if (event.eventType === "turn_interrupted") {
    newState.activeTurnStatus = "interrupted";
  }
  return newState;
}

export function detectGap(state: ReplayState, incomingSeq: number): boolean {
  return incomingSeq > state.lastSeq + 1;
}

export function isDuplicate(state: ReplayState, eventId: string): boolean {
  return state.seenEventIds.has(eventId);
}

export function getReplayCursor(state: ReplayState): string | null {
  return state.lastCommitCursor;
}

export function getCommitPoint(state: ReplayState): CommitPoint | null {
  if (state.lastCommitCursor === null) return null;
  const commitEvent = [...state.events].reverse().find((e) => e.commitPoint === true);
  if (!commitEvent) return null;
  return {
    eventId: commitEvent.eventId,
    replayCursor: state.lastCommitCursor,
    seq: state.lastCommitSeq,
  };
}
```

- [ ] **Step 4: Implement the Rust event journal**

Create `src-tauri/src/protocol/event_journal.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ActiveTurnStatus {
    Unknown,
    Active,
    Completed,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JournalEvent {
    pub event_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub event_type: String,
    pub timestamp: String,
    pub seq: u64,
    pub replay_cursor: Option<String>,
    pub commit_point: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommitPoint {
    pub event_id: String,
    pub replay_cursor: String,
    pub seq: u64,
}

#[derive(Debug, Clone)]
pub struct ReplayState {
    pub session_id: String,
    pub events: Vec<JournalEvent>,
    seen_event_ids: HashSet<String>,
    pub last_event_id: Option<String>,
    pub last_seq: i64,
    pub last_commit_seq: u64,
    pub last_commit_cursor: Option<String>,
    pub active_turn_status: ActiveTurnStatus,
    pub active_turn_id: Option<String>,
}

impl ReplayState {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            events: Vec::new(),
            seen_event_ids: HashSet::new(),
            last_event_id: None,
            last_seq: -1,
            last_commit_seq: 0,
            last_commit_cursor: None,
            active_turn_status: ActiveTurnStatus::Unknown,
            active_turn_id: None,
        }
    }

    pub fn apply_event(&mut self, event: JournalEvent) {
        self.seen_event_ids.insert(event.event_id.clone());
        if event.commit_point == Some(true) {
            self.last_commit_seq = event.seq;
            if let Some(ref cursor) = event.replay_cursor {
                self.last_commit_cursor = Some(cursor.clone());
            }
        }
        if event.event_type == "agent_start" {
            if let Some(ref turn_id) = event.turn_id {
                self.active_turn_status = ActiveTurnStatus::Active;
                self.active_turn_id = Some(turn_id.clone());
            }
        } else if event.event_type == "agent_end" {
            self.active_turn_status = ActiveTurnStatus::Completed;
            self.active_turn_id = None;
        } else if event.event_type == "turn_interrupted" {
            self.active_turn_status = ActiveTurnStatus::Interrupted;
        }
        self.last_event_id = Some(event.event_id.clone());
        self.last_seq = event.seq as i64;
        self.events.push(event);
    }

    pub fn detect_gap(&self, incoming_seq: u64) -> bool {
        (incoming_seq as i64) > self.last_seq + 1
    }

    pub fn is_duplicate(&self, event_id: &str) -> bool {
        self.seen_event_ids.contains(event_id)
    }

    pub fn replay_cursor(&self) -> Option<&str> {
        self.last_commit_cursor.as_deref()
    }

    pub fn commit_point(&self) -> Option<CommitPoint> {
        let cursor = self.last_commit_cursor.as_ref()?;
        let commit_event = self.events.iter().rev().find(|e| e.commit_point == Some(true))?;
        Some(CommitPoint {
            event_id: commit_event.event_id.clone(),
            replay_cursor: cursor.clone(),
            seq: self.last_commit_seq,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(event_id: &str, seq: u64, event_type: &str, commit: bool) -> JournalEvent {
        JournalEvent {
            event_id: event_id.to_string(),
            session_id: "ses_0123456789abcdef".to_string(),
            turn_id: if event_type == "agent_start" { Some("turn_0123456789abcdef".to_string()) } else { None },
            event_type: event_type.to_string(),
            timestamp: "2026-07-29T00:00:00.000Z".to_string(),
            seq,
            replay_cursor: if commit { Some(format!("cursor_{}", seq)) } else { None },
            commit_point: Some(commit),
        }
    }

    #[test]
    fn replay_state_starts_empty() {
        let state = ReplayState::new("ses_0123456789abcdef");
        assert_eq!(state.active_turn_status, ActiveTurnStatus::Unknown);
        assert!(state.last_event_id.is_none());
    }

    #[test]
    fn apply_event_updates_state() {
        let mut state = ReplayState::new("ses_0123456789abcdef");
        state.apply_event(make_event("evt_0123456789abcdef01234567", 0, "agent_start", true));
        assert_eq!(state.active_turn_status, ActiveTurnStatus::Active);
        assert!(state.is_duplicate("evt_0123456789abcdef01234567"));
    }

    #[test]
    fn gap_detection() {
        let mut state = ReplayState::new("ses_0123456789abcdef");
        state.apply_event(make_event("evt_0123456789abcdef01234567", 0, "test", false));
        assert!(!state.detect_gap(1));
        assert!(state.detect_gap(5));
    }

    #[test]
    fn commit_point_tracking() {
        let mut state = ReplayState::new("ses_0123456789abcdef");
        state.apply_event(make_event("evt_0123456789abcdef01234567", 0, "test", true));
        state.apply_event(make_event("evt_0123456789abcdef01234568", 1, "test", false));
        assert_eq!(state.replay_cursor(), Some("cursor_0"));
        let cp = state.commit_point().unwrap();
        assert_eq!(cp.seq, 0);
    }
}
```

- [ ] **Step 5: Run tests and verify they pass**

Run:

```bash
pnpm test -- src/lib/protocol/eventJournal.test.ts
cargo test --manifest-path src-tauri/Cargo.toml protocol::event_journal --locked
```

Expected: both PASS.

- [ ] **Step 6: Commit the event journal**

Run:

```bash
git add src-tauri/src/protocol/event_journal.rs src/lib/protocol/eventJournal.ts src/lib/protocol/eventJournal.test.ts
git commit -m "feat: implement event journal replay with stable event IDs"
```

---

### Task 9: Implement Queue and Steer Extension

**Files:**
- Create: `src-tauri/src/protocol/queue_steer.rs`
- Create: `src/lib/protocol/queueSteer.ts`
- Test: `src/lib/protocol/queueSteer.test.ts`

**Interfaces:**
- Consumes: `protocol/v1/queue-steer.schema.json` from Task 2; stable IDs from Task 4; error codes from Task 5.
- Produces: Rust `QueueReceipt`, `SteerRequest`, `SteerAck` types; TypeScript equivalents; queue state machine with FIFO/priority, accept/reject, cancel/dequeue; steer target binding, ack, application order, too-late/conflict errors; restart receipt query with submitted/unsubmitted boundaries and no auto-resend rule.

- [ ] **Step 1: Write the failing queue and steer test**

Create `src/lib/protocol/queueSteer.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  type QueueReceipt,
  type QueueState,
  type SteerRequest,
  type SteerAck,
  QueueStatus,
  createQueueState,
  enqueue,
  dequeue,
  cancelReceipt,
  queryReceipt,
  applySteer,
  SteerResult,
  isSubmitted,
} from "./queueSteer";

describe("createQueueState", () => {
  it("starts empty with max depth from limits", () => {
    const state = createQueueState("ses_0123456789abcdef", 64);
    expect(state.sessionId).toBe("ses_0123456789abcdef");
    expect(state.receipts).toEqual([]);
    expect(state.maxDepth).toBe(64);
  });
});

describe("enqueue", () => {
  it("accepts a prompt and returns a receipt with accepted status", () => {
    const state = createQueueState("ses_0123456789abcdef", 64);
    const result = enqueue(state, {
      message: "Hello",
      turnId: "turn_0123456789abcdef",
      priority: null,
    });
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.receipt.status).toBe(QueueStatus.Accepted);
      expect(result.receipt.position).toBe(0);
      expect(result.receipt.sessionId).toBe("ses_0123456789abcdef");
    }
  });

  it("rejects when queue is full", () => {
    let state = createQueueState("ses_0123456789abcdef", 1);
    const r1 = enqueue(state, { message: "first", turnId: "turn_0123456789abcdef", priority: null });
    expect(r1.ok).toBe(true);
    if (r1.ok) {
      state = r1.state;
    }
    const r2 = enqueue(state, { message: "second", turnId: "turn_0123456789abcdef2", priority: null });
    expect(r2.ok).toBe(false);
    if (!r2.ok) {
      expect(r2.errorCode).toBe("QUEUE_FULL");
    }
  });

  it("places higher priority before lower priority", () => {
    let state = createQueueState("ses_0123456789abcdef", 64);
    const r1 = enqueue(state, { message: "low", turnId: "turn_0123456789abcdef", priority: 0 });
    if (r1.ok) state = r1.state;
    const r2 = enqueue(state, { message: "high", turnId: "turn_0123456789abcdef2", priority: 10 });
    if (r2.ok) state = r2.state;
    expect(state.receipts[0].receiptId).toBe(r2.ok ? r2.receipt.receiptId : "");
  });
});

describe("dequeue", () => {
  it("removes and returns the first accepted receipt", () => {
    let state = createQueueState("ses_0123456789abcdef", 64);
    const r = enqueue(state, { message: "hello", turnId: "turn_0123456789abcdef", priority: null });
    if (r.ok) state = r.state;
    const result = dequeue(state);
    expect(result.receipt).not.toBeNull();
    expect(result.receipt?.status).toBe(QueueStatus.Dequeued);
    expect(result.state.receipts).toHaveLength(0);
  });

  it("returns null when queue is empty", () => {
    const state = createQueueState("ses_0123456789abcdef", 64);
    const result = dequeue(state);
    expect(result.receipt).toBeNull();
  });
});

describe("cancelReceipt", () => {
  it("marks a receipt as cancelled", () => {
    let state = createQueueState("ses_0123456789abcdef", 64);
    const r = enqueue(state, { message: "hello", turnId: "turn_0123456789abcdef", priority: null });
    if (r.ok) state = r.state;
    const receiptId = r.ok ? r.receipt.receiptId : "";
    state = cancelReceipt(state, receiptId);
    const queried = queryReceipt(state, receiptId);
    expect(queried?.status).toBe(QueueStatus.Cancelled);
  });
});

describe("isSubmitted", () => {
  it("returns true for accepted receipts", () => {
    let state = createQueueState("ses_0123456789abcdef", 64);
    const r = enqueue(state, { message: "hello", turnId: "turn_0123456789abcdef", priority: null });
    if (r.ok) state = r.state;
    const receiptId = r.ok ? r.receipt.receiptId : "";
    expect(isSubmitted(state, receiptId)).toBe(true);
  });

  it("returns false for dequeued receipts (already submitted to runtime)", () => {
    let state = createQueueState("ses_0123456789abcdef", 64);
    const r = enqueue(state, { message: "hello", turnId: "turn_0123456789abcdef", priority: null });
    if (r.ok) state = r.state;
    const receiptId = r.ok ? r.receipt.receiptId : "";
    const dq = dequeue(state);
    state = dq.state;
    expect(isSubmitted(state, receiptId)).toBe(false);
  });
});

describe("applySteer", () => {
  it("acks a steer targeting an active turn", () => {
    const request: SteerRequest = {
      targetTurnId: "turn_0123456789abcdef",
      message: "Also check tests",
      ackDeadline: 5000,
    };
    const result = applySteer(request, "turn_0123456789abcdef", true, 0);
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.ack.targetTurnId).toBe("turn_0123456789abcdef");
      expect(result.ack.appliedOrder).toBe(0);
    }
  });

  it("returns too_late when turn is not active", () => {
    const request: SteerRequest = {
      targetTurnId: "turn_0123456789abcdef",
      message: "Too late",
      ackDeadline: 5000,
    };
    const result = applySteer(request, "turn_0123456789abcdef", false, 0);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.errorCode).toBe("STEER_TOO_LATE");
    }
  });

  it("returns conflict when target turn does not match active turn", () => {
    const request: SteerRequest = {
      targetTurnId: "turn_aaaaaaaaaaaaaaaa",
      message: "Wrong turn",
      ackDeadline: 5000,
    };
    const result = applySteer(request, "turn_0123456789abcdef", true, 0);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.errorCode).toBe("STEER_CONFLICT");
    }
  });
});
```

- [ ] **Step 2: Run the test and verify it fails**

Run:

```bash
pnpm test -- src/lib/protocol/queueSteer.test.ts
```

Expected: FAIL because `./queueSteer` module does not exist.

- [ ] **Step 3: Implement the TypeScript queue and steer**

Create `src/lib/protocol/queueSteer.ts`:

```ts
export enum QueueStatus {
  Accepted = "accepted",
  Rejected = "rejected",
  Dequeued = "dequeued",
  Cancelled = "cancelled",
}

export interface QueueReceipt {
  receiptId: string;
  sessionId: string;
  turnId: string;
  status: QueueStatus;
  position: number;
  priority: number | null;
  submittedAt: string;
}

export interface QueueEntry {
  message: string;
  turnId: string;
  priority: number | null;
}

export interface QueueState {
  sessionId: string;
  receipts: QueueReceipt[];
  maxDepth: number;
  steerCounter: number;
}

export interface SteerRequest {
  targetTurnId: string;
  message: string;
  ackDeadline: number;
  images?: unknown[];
}

export interface SteerAck {
  targetTurnId: string;
  appliedOrder: number;
  acknowledgedAt: string;
}

export type SteerResult =
  | { ok: true; ack: SteerAck }
  | { ok: false; errorCode: string };

export type EnqueueResult =
  | { ok: true; receipt: QueueReceipt; state: QueueState }
  | { ok: false; errorCode: string };

let receiptCounter = 0;

function generateReceiptId(): string {
  const hex = (receiptCounter++.toString(16).padStart(16, "0")).slice(-16);
  return `q_${hex}`;
}

export function createQueueState(sessionId: string, maxDepth: number): QueueState {
  return { sessionId, receipts: [], maxDepth, steerCounter: 0 };
}

export function enqueue(state: QueueState, entry: QueueEntry): EnqueueResult {
  if (state.receipts.filter((r) => r.status === QueueStatus.Accepted).length >= state.maxDepth) {
    return { ok: false, errorCode: "QUEUE_FULL" };
  }
  const receipt: QueueReceipt = {
    receiptId: generateReceiptId(),
    sessionId: state.sessionId,
    turnId: entry.turnId,
    status: QueueStatus.Accepted,
    position: state.receipts.filter((r) => r.status === QueueStatus.Accepted).length,
    priority: entry.priority,
    submittedAt: new Date().toISOString(),
  };
  const newReceipts = [...state.receipts, receipt];
  newReceipts.sort((a, b) => {
    const pa = a.priority ?? 0;
    const pb = b.priority ?? 0;
    if (pa !== pb) return pb - pa;
    return a.position - b.position;
  });
  newReceipts.forEach((r, i) => {
    if (r.status === QueueStatus.Accepted) r.position = i;
  });
  return { ok: true, receipt, state: { ...state, receipts: newReceipts } };
}

export function dequeue(state: QueueState): { receipt: QueueReceipt | null; state: QueueState } {
  const idx = state.receipts.findIndex((r) => r.status === QueueStatus.Accepted);
  if (idx === -1) return { receipt: null, state };
  const receipt = { ...state.receipts[idx], status: QueueStatus.Dequeued };
  const newReceipts = [...state.receipts];
  newReceipts[idx] = receipt;
  return { receipt, state: { ...state, receipts: newReceipts } };
}

export function cancelReceipt(state: QueueState, receiptId: string): QueueState {
  const newReceipts = state.receipts.map((r) =>
    r.receiptId === receiptId && r.status === QueueStatus.Accepted
      ? { ...r, status: QueueStatus.Cancel