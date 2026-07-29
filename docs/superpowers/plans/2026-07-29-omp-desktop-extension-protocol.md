# OMP Desktop Extension Protocol Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish the versioned `_omp/desktop/v1/*` Extension Protocol that Desktop and the bundled OMP Runtime negotiate over ACP, replacing the six ad-hoc `_omp/*` methods with a versioned, typed, schema-validated namespace, and laying the typed surface that Plans 3–5 consume.

**Architecture:** Add an OMP Fork patch that introduces the `_omp/desktop/v1/*` namespace in the submodule's `AcpAgent.extMethod` dispatcher alongside a capability descriptor, JSON Schemas, stable ID rules, error registry, and compatibility/replay rules. On the Desktop host side, introduce an `OmpExtension` client that negotiates the descriptor during `initialize`, dispatches typed requests, and degrades to conservative behavior when the capability is absent. Both sides share a single TypeScript schema source that generates Rust bindings (via `serde` derive) and TS types.

**Tech Stack:** TypeScript, Rust, JSON Schema (2020-12), `serde`/`serde_json`, ACP SDK (`@agentclientprotocol/sdk`), Tauri 2, pnpm, bun (submodule), Vitest, `cargo test`.

---

## Global Constraints

- The spec for this plan is `docs/superpowers/specs/2026-07-28-omp-desktop-design.md` §5 (OMP Desktop Extension Protocol). Read it before starting any task.
- Namespace: `_omp/desktop/v1/*`. The existing unversioned `_omp/*` methods are **compatibility aliases** that must keep working during Plan 2; they are deprecated and will be removed in Plan 3.
- Do not break the fail-closed behavior from Plan 1: when the runtime is not spawned, all extension calls must return `runtime_unavailable` from the Desktop side without touching the wire.
- Do not invent OMP product capabilities that the submodule does not already expose. Plan 2 is a **protocol and contract layer** — it wraps existing OMP data (sessions, projects, usage, extensions, provider/model/credential catalog) in versioned, typed envelopes. It does not add new product features.
- The OMP submodule is pinned at `667111575ebba136dadfd6989379e7f67e0d40d9`. Plan 2 work in the submodule is delivered as a **patch** recorded in `provenance/omp-patches.json` and committed to the team Fork branch `desktop-v1-protocol`. The superproject submodule pointer advances to the patched commit.
- The single source of truth for schemas is `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/schema/` (TypeScript files exporting Zod-equivalent raw JSON Schema objects). Rust bindings are generated at build time via a `build.rs` step that emits `serde`-compatible types into `src-tauri/src/omp_desktop_v1/generated.rs`. **Do not hand-edit generated files.**
- Brand policy still applies: lowercase `omp` is allowed only in protocol namespaces, crate/package identifiers, paths, and commands. User-visible text is `OMP`.
- Application version remains `0.1.9` during Plan 2.
- Do not remove `GROK_APP_MOCK_ACP` or the `mock_acp` module; the mock transport is the primary test surface for Desktop-side contract tests.
- Plan 2 does not implement the real runtime Supervisor, credential migration, channel adapters, packaging, or i18n redesign. It only defines and contract-tests the protocol.

---

## File and Module Map

**Create (OMP submodule patch — applied to `runtime/oh-my-pi/`)**

- `packages/coding-agent/src/modes/acp/desktop-v1/index.ts` — namespace entrypoint, re-exports.
- `packages/coding-agent/src/modes/acp/desktop-v1/capability.ts` — `DesktopV1Capability` descriptor returned from `initialize`.
- `packages/coding-agent/src/modes/acp/desktop-v1/schema/methods.ts` — per-method JSON Schema (request/result) for every `_omp/desktop/v1/*` method.
- `packages/coding-agent/src/modes/acp/desktop-v1/schema/notifications.ts` — per-notification JSON Schema for every `_omp/desktop/v1/*` notification.
- `packages/coding-agent/src/modes/acp/desktop-v1/schema/ids.ts` — stable ID format rules (session, turn, event, permission, queue, credential, project, model, mcp source).
- `packages/coding-agent/src/modes/acp/desktop-v1/schema/errors.ts` — stable error code table with `messageKey` + `args` shape.
- `packages/coding-agent/src/modes/acp/desktop-v1/schema/pagination.ts` — cursor, page, snapshot boundary schemas.
- `packages/coding-agent/src/modes/acp/desktop-v1/schema/journal.ts` — stable event ID, replay cursor, commit point, gap rules.
- `packages/coding-agent/src/modes/acp/desktop-v1/schema/index.ts` — re-export + `schemaDigest` computation.
- `packages/coding-agent/src/modes/acp/desktop-v1/dispatcher.ts` — routes `_omp/desktop/v1/*` to handlers, validates params/result against schema.
- `packages/coding-agent/src/modes/acp/desktop-v1/handlers/sessions.ts` — `sessions.listAll`, `sessions.byCwd`, `projects.list`.
- `packages/coding-agent/src/modes/acp/desktop-v1/handlers/usage.ts` — `usage.reports`.
- `packages/coding-agent/src/modes/acp/desktop-v1/handlers/extensions.ts` — `extensions.list`, `extensions.toggle`.
- `packages/coding-agent/src/modes/acp/desktop-v1/handlers/providers.ts` — `providers.list`, `providers.models`.
- `packages/coding-agent/src/modes/acp/desktop-v1/handlers/credentials.ts` — `credentials.list`, `credentials.beginAuth`, `credentials.completeAuth`, `credentials.cancelAuth`, `credentials.replace`, `credentials.revoke`, `credentials.health`, `credentials.migrationStatus`.
- `packages/coding-agent/src/modes/acp/desktop-v1/handlers/mcp.ts` — `mcp.list`, `mcp.discover`.
- `packages/coding-agent/src/modes/acp/desktop-v1/handlers/sessionConfig.ts` — `sessionConfig.get`, `sessionConfig.set`.
- `packages/coding-agent/src/modes/acp/desktop-v1/handlers/queue.ts` — `queue.enqueue`, `queue.cancel` (returns `runtime_unavailable` until Plan 3 Supervisor lands).
- `packages/coding-agent/src/modes/acp/desktop-v1/handlers/steer.ts` — `steer.send` (returns `runtime_unavailable` until Plan 3).
- `packages/coding-agent/src/modes/acp/desktop-v1/handlers/diagnostics.ts` — `diagnostics.selfCheck`.
- `packages/coding-agent/src/modes/acp/desktop-v1/handlers/index.ts` — handler registry.
- `packages/coding-agent/src/modes/acp/desktop-v1/compat.ts` — maps legacy `_omp/*` calls to v1 handlers and v1 results back to legacy shape.
- `packages/coding-agent/src/modes/acp/desktop-v1/errors.ts` — `DesktopV1Error` class with stable code, category, severity, `messageKey`, `args`, `recoverable`, `retryable`.
- `packages/coding-agent/src/modes/acp/desktop-v1/types.ts` — shared TS types not generated from schema.
- `packages/coding-agent/test/desktop-v1/dispatcher.test.ts` — dispatcher routing and schema validation tests.
- `packages/coding-agent/test/desktop-v1/compat.test.ts` — legacy alias mapping tests.
- `packages/coding-agent/test/desktop-v1/contract/sessions.test.ts` — contract test for sessions.* and projects.*.
- `packages/coding-agent/test/desktop-v1/contract/usage.test.ts` — contract test for usage.*.
- `packages/coding-agent/test/desktop-v1/contract/extensions.test.ts` — contract test for extensions.*.
- `packages/coding-agent/test/desktop-v1/contract/providers.test.ts` — contract test for providers.* and models.
- `packages/coding-agent/test/desktop-v1/contract/credentials.test.ts` — contract test for credentials.* (uses mock auth store).
- `packages/coding-agent/test/desktop-v1/contract/mcp.test.ts` — contract test for mcp.*.
- `packages/coding-agent/test/desktop-v1/contract/queue-steer.test.ts` — contract test asserting `runtime_unavailable` for queue/steer.
- `packages/coding-agent/test/desktop-v1/contract/diagnostics.test.ts` — contract test for diagnostics.selfCheck.
- `packages/coding-agent/test/desktop-v1/contract/journal.test.ts` — contract test for journal replay rules (uses mock session entries).

**Create (OMP submodule patch — schema generation)**

- `packages/coding-agent/src/modes/acp/desktop-v1/schema/codegen.ts` — emits a single `schema-bundle.json` consumed by the Rust `build.rs`.
- `packages/coding-agent/scripts/gen-desktop-v1-schema.ts` — CLI entry that writes `packages/coding-agent/src/modes/acp/desktop-v1/schema/generated/schema-bundle.json`.

**Create (Desktop host)**

- `src-tauri/src/omp_desktop_v1/mod.rs` — `OmpExtension` client struct, capability cache, request dispatch.
- `src-tauri/src/omp_desktop_v1/capability.rs` — `DesktopV1Capability` Rust mirror (generated + hand-validated).
- `src-tauri/src/omp_desktop_v1/generated.rs` — generated `serde` types (committed, not gitignored, so builds without a codegen step).
- `src-tauri/src/omp_desktop_v1/errors.rs` — `DesktopV1Error` Rust mirror and `From<AgentError>` mapping.
- `src-tauri/src/omp_desktop_v1/ids.rs` — stable ID validation helpers.
- `src-tauri/src/omp_desktop_v1/compat.rs` — legacy `_omp/*` shim that forwards to v1 when capability present, falls back to legacy when absent.
- `src-tauri/src/omp_desktop_v1/build.rs` — copies `schema-bundle.json` into `OUT_DIR` and runs `schemars`/`serde_json` derive setup (no network).
- `src-tauri/src/omp_desktop_v1/tests/contract.rs` — Rust-side contract tests using `mock_acp` transport.
- `src-tauri/src/omp_desktop_v1/tests/fixtures/` — golden JSON fixtures shared with TS contract tests.

**Create (Desktop frontend)**

- `src/lib/ompDesktopV1/index.ts` — typed client wrapper around `AcpClient.request`.
- `src/lib/ompDesktopV1/capability.ts` — capability negotiation result type.
- `src/lib/ompDesktopV1/methods.ts` — typed method signatures for every v1 method.
- `src/lib/ompDesktopV1/errors.ts` — error class mirror.
- `src/lib/ompDesktopV1/contract.test.ts` — frontend contract tests using `mock_acp`.

**Modify**

- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/acp-agent.ts` — advertise `desktopV1` capability in `initialize`; route `_omp/desktop/v1/*` to the dispatcher; keep `_omp/*` legacy routing via `compat.ts`.
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/acp-agent.ts` `extMethod` switch — add a branch for the `_omp/desktop/v1/` prefix that delegates to `desktop-v1/dispatcher.ts`.
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/acp-agent.ts` `initialize` — add `agentCapabilities.desktopV1: { schemaVersion: 1, schemaDigest, methods, notifications }` when the feature is enabled (gated on `OMP_DESKTOP_V1_PROTOCOL=1` env in Plan 2 to allow staged rollout).
- `src-tauri/src/acp_client.rs` — remove dead `x.ai/rewind/*` private bindings (lines 1157–1197); add `request_extension_v1(method, params)` helper; fix stale header comment; remove `grok agent stdio` reference in `lib.rs:1`.
- `src-tauri/src/lib.rs` — register `omp_desktop_v1` module; remove stale module doc.
- `src-tauri/src/commands.rs` — `skills_list`, `inspect_mcp`, `project_inspect`, `agents_list`, `plugins_list` route through `OmpExtension` when capability present; fall back to `runtime_unavailable` when absent.
- `src/lib/api.ts` — add typed wrappers for v1 methods; keep existing legacy wrappers as thin compat shims that call v1 when capability present.
- `src/lib/runtimeAvailability.ts` — replace hardcoded constant with a live read from the `runtime_availability` Tauri command (the Rust side gains a real value once capability is negotiated in Plan 3).
- `src/App.tsx` — replace `runtimeAvailability` import with async capability probe; keep fail-closed banner until capability negotiation succeeds.
- `provenance/omp-patches.json` — add the Plan 2 patch entry.
- `src-tauri/Cargo.toml` — add `schemars = "0.8"` and `serde_json = "1"` (already present) to `[dependencies]`.

**Delete**

- `src-tauri/src/acp_client.rs` lines 1157–1197 (`rewind_points`, `rewind_execute` private `x.ai/rewind/*` bindings — dead code per Plan 1 audit).

---

### Task 1: Freeze the Extension Protocol Surface Inventory

**Files:**
- Create: `docs/superpowers/plans/2026-07-29-omp-desktop-extension-protocol-inventory.md`
- Read: `docs/superpowers/specs/2026-07-28-omp-desktop-design.md` §5
- Read: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/acp-agent.ts`

**Interfaces:**
- Consumes: Plan 1 baseline; master design §5.
- Produces: A frozen table of every v1 method and notification that Plan 2 will implement, with stable ID, params shape, result shape, error codes, and capability category.

- [ ] **Step 1: Write the inventory document**

Create `docs/superpowers/plans/2026-07-29-omp-desktop-extension-protocol-inventory.md` containing:

```md
# OMP Desktop Extension Protocol v1 Inventory

## Methods (request → result)

| Method | Params | Result | Errors | Capability |
|---|---|---|---|---|
| `_omp/desktop/v1/sessions.listAll` | `{ limit?: int≤5000 }` | `{ sessions: SessionInfo[], total: int, cursor?: string }` | `invalid_params, runtime_unavailable` | sessions.list |
| `_omp/desktop/v1/sessions.byCwd` | `{ cwd: string, limit?: int≤500 }` | `{ sessions: SessionInfo[], cursor?: string }` | `invalid_params, runtime_unavailable` | sessions.list |
| `_omp/desktop/v1/projects.list` | `{}` | `{ projects: ProjectInfo[], totalSessions: int }` | `runtime_unavailable` | sessions.list |
| `_omp/desktop/v1/usage.reports` | `{}` | `{ reports: UsageReport[] }` | `runtime_unavailable` | usage |
| `_omp/desktop/v1/extensions.list` | `{ cwd?: string }` | `{ extensions: ExtensionInfo[] }` | `runtime_unavailable` | extensions |
| `_omp/desktop/v1/extensions.toggle` | `{ providerId: string, enabled?: bool }` | `{ enabled: bool }` | `invalid_params, not_found, runtime_unavailable` | extensions |
| `_omp/desktop/v1/providers.list` | `{}` | `{ providers: ProviderInfo[] }` | `runtime_unavailable` | providers |
| `_omp/desktop/v1/providers.models` | `{ providerId?: string }` | `{ models: ModelInfo[] }` | `runtime_unavailable` | providers |
| `_omp/desktop/v1/credentials.list` | `{ providerId?: string }` | `{ credentials: CredentialMetadata[] }` | `runtime_unavailable` | credentials |
| `_omp/desktop/v1/credentials.beginAuth` | `{ providerId: string, method: string }` | `{ authId: string, status: "pending" }` | `invalid_params, runtime_unavailable` | credentials |
| `_omp/desktop/v1/credentials.completeAuth` | `{ authId: string, code: string }` | `{ status: "active" }` | `invalid_params, auth_failed, runtime_unavailable` | credentials |
| `_omp/desktop/v1/credentials.cancelAuth` | `{ authId: string }` | `{ status: "cancelled" }` | `invalid_params, runtime_unavailable` | credentials |
| `_omp/desktop/v1/credentials.replace` | `{ credentialId: string }` | `{ status: "active" }` | `invalid_params, not_found, runtime_unavailable` | credentials |
| `_omp/desktop/v1/credentials.revoke` | `{ credentialId: string }` | `{ status: "revoked" }` | `invalid_params, not_found, runtime_unavailable` | credentials |
| `_omp/desktop/v1/credentials.health` | `{ credentialId?: string }` | `{ healthy: bool[], unhealthy: bool[] }` | `runtime_unavailable` | credentials |
| `_omp/desktop/v1/credentials.migrationStatus` | `{}` | `{ migrated: int, pending: int, failed: int, details: MigrationDetail[] }` | `runtime_unavailable` | credentials |
| `_omp/desktop/v1/mcp.list` | `{ cwd?: string }` | `{ sources: McpSourceInfo[] }` | `runtime_unavailable` | mcp |
| `_omp/desktop/v1/mcp.discover` | `{ cwd: string }` | `{ sources: McpSourceInfo[] }` | `invalid_params, runtime_unavailable` | mcp |
| `_omp/desktop/v1/sessionConfig.get` | `{ sessionId?: string }` | `{ config: SessionConfig }` | `runtime_unavailable` | sessionConfig |
| `_omp/desktop/v1/sessionConfig.set` | `{ sessionId?: string, config: Partial<SessionConfig> }` | `{ config: SessionConfig }` | `invalid_params, runtime_unavailable` | sessionConfig |
| `_omp/desktop/v1/queue.enqueue` | `{ sessionId: string, prompt: string }` | `{ receiptId: string, status: "queued" }` | `runtime_unavailable` | queue (gated) |
| `_omp/desktop/v1/queue.cancel` | `{ receiptId: string }` | `{ status: "cancelled" }` | `runtime_unavailable, not_found` | queue (gated) |
| `_omp/desktop/v1/steer.send` | `{ turnId: string, message: string }` | `{ status: "accepted" }` | `runtime_unavailable, too_late` | steer (gated) |
| `_omp/desktop/v1/diagnostics.selfCheck` | `{}` | `{ checks: DiagnosticCheck[] }` | `runtime_unavailable` | diagnostics |

## Notifications (no result)

| Notification | Params | Capability |
|---|---|---|
| `_omp/desktop/v1/queue.updated` | `{ receiptId: string, status: "accepted"\|"rejected"\|"cancelled", sessionId: string }` | queue |
| `_omp/desktop/v1/turn.status` | `{ sessionId: string, turnId: string, status: "active"\|"idle"\|"interrupted"\|"unknown", commitPoint?: string }` | turn |
| `_omp/desktop/v1/journal.commit` | `{ sessionId: string, commitPoint: string, stableEventId: string }` | journal |
| `_omp/desktop/v1/credential.expired` | `{ credentialId: string, providerId: string, reason: string }` | credentials |

## Stable ID formats

| Entity | Format | Example |
|---|---|---|
| Session | `sess_<26char base32>` | `sess_abc123def456ghi789jkl012mno` |
| Turn | `turn_<26char base32>` | `turn_xyz789abc012def345ghi678jkl` |
| Event | `evt_<26char base32>` | `evt_qrs456tuv789wxy012abc345def` |
| Permission request | `perm_<26char base32>` | `perm_mno012pqr345stu678vwx901yza` |
| Queue receipt | `rcpt_<26char base32>` | `rcpt_bcd123efg456hij789klm012nop` |
| Credential | `cred_<26char base32>` | `cred_pqr456stu789vwx012abc345def` |
| Project | `proj_<sha1 of cwd>` | `proj_a1b2c3d4e5f6789012345678901234567890abcd` |
| Model | `<providerId>/<modelId>` | `xai/grok-4.5` |
| MCP source | `mcp_<sha1 of sourceId>` | `mcp_f1e2d3c4b5a697887766554433221100ffeeddcc` |

## Error code registry

| Code | Category | Severity | Recoverable | Retryable | messageKey |
|---|---|---|---|---|---|
| `runtime_unavailable` | runtime | error | false | false | `runtime.unavailable` |
| `invalid_params` | validation | error | true | false | `validation.invalidParams` |
| `not_found` | state | error | true | false | `state.notFound` |
| `auth_failed` | auth | error | true | false | `auth.failed` |
| `capability_missing` | capability | error | false | false | `capability.missing` |
| `too_late` | timing | warning | false | false | `timing.tooLate` |
| `schema_digest_mismatch` | compatibility | error | false | false | `compat.schemaDigestMismatch` |
| `unknown_method` | compatibility | error | false | false | `compat.unknownMethod` |
| `journal_gap` | recovery | warning | true | true | `recovery.journalGap` |

## Compatibility rules

- Major version bump required for: removing a method, changing a param type, changing a result type, changing an error code's meaning.
- Minor version bump allowed for: adding a new method, adding an optional param, adding a new result field, adding a new error code.
- Unknown fields in params MUST be ignored by the handler (forward compatibility).
- Unknown fields in results MUST be ignored by the client (forward compatibility).
- A method marked deprecated in v1.N MUST remain available until v2.0.
- Legacy `_omp/*` calls are accepted and mapped to v1 handlers via `compat.ts`; the response is reshaped to legacy shape for backward compatibility.
```

- [ ] **Step 2: Commit the inventory**

```bash
git add docs/superpowers/plans/2026-07-29-omp-desktop-extension-protocol-inventory.md
git commit -m "docs: freeze OMP Desktop v1 Extension Protocol inventory"
```

Expected: commit succeeds; inventory is the authoritative reference for all subsequent tasks.

---

### Task 2: Implement the v1 Schema Bundle and Codegen

**Files:**
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/schema/methods.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/schema/notifications.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/schema/ids.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/schema/errors.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/schema/pagination.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/schema/journal.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/schema/index.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/schema/codegen.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/scripts/gen-desktop-v1-schema.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/schema/generated/schema-bundle.json` (committed output)

**Interfaces:**
- Consumes: inventory from Task 1.
- Produces: `schema-bundle.json` — a single JSON document containing all method schemas, notification schemas, error registry, and ID rules; consumed by the Rust `build.rs` in Task 5 and the TS dispatcher in Task 3.

- [ ] **Step 1: Write failing schema tests**

Create `packages/coding-agent/test/desktop-v1/schema.test.ts`:

```ts
import { test } from "bun:test";
import { assertEquals, assertExists } from "node:assert";
import { methodSchemas, notificationSchemas, errorRegistry, idFormats } from "../../src/modes/acp/desktop-v1/schema/index.ts";

test("every method in the inventory has a schema", () => {
  const expectedMethods = [
    "sessions.listAll", "sessions.byCwd", "projects.list", "usage.reports",
    "extensions.list", "extensions.toggle", "providers.list", "providers.models",
    "credentials.list", "credentials.beginAuth", "credentials.completeAuth", "credentials.cancelAuth",
    "credentials.replace", "credentials.revoke", "credentials.health", "credentials.migrationStatus",
    "mcp.list", "mcp.discover", "sessionConfig.get", "sessionConfig.set",
    "queue.enqueue", "queue.cancel", "steer.send", "diagnostics.selfCheck",
  ];
  for (const m of expectedMethods) {
    assertExists(methodSchemas[m], `missing schema for ${m}`);
    assertEquals(methodSchemas[m].methodNamespace, "_omp/desktop/v1");
  }
});

test("every notification has a schema", () => {
  const expectedNotifications = ["queue.updated", "turn.status", "journal.commit", "credential.expired"];
  for (const n of expectedNotifications) {
    assertExists(notificationSchemas[n], `missing schema for ${n}`);
  }
});

test("error registry contains all codes from the inventory", () => {
  const expectedCodes = ["runtime_unavailable", "invalid_params", "not_found", "auth_failed",
    "capability_missing", "too_late", "schema_digest_mismatch", "unknown_method", "journal_gap"];
  for (const code of expectedCodes) {
    assertExists(errorRegistry[code], `missing error code ${code}`);
    assertExists(errorRegistry[code].messageKey);
  }
});

test("id formats define regex patterns", () => {
  for (const [name, format] of Object.entries(idFormats)) {
    assertExists(format.pattern, `${name} missing pattern`);
    assertExists(format.example, `${name} missing example`);
  }
});
```

Run `cd runtime/oh-my-pi && bun test packages/coding-agent/test/desktop-v1/schema.test.ts`.

Expected: FAIL with module not found.

- [ ] **Step 2: Implement the schema modules**

Create each schema file with raw JSON Schema objects (JSON Schema 2020-12 dialect). Each method schema exports:

```ts
export interface MethodSchema {
  method: string;              // e.g. "sessions.listAll"
  methodNamespace: "_omp/desktop/v1";
  params: object;              // JSON Schema for params
  result: object;              // JSON Schema for result
  errors: string[];            // error codes this method can return
  capability: string;          // capability category
  deprecated?: boolean;
  deprecatedMessage?: string;
}
```

For `ids.ts`:

```ts
export interface IdFormat {
  prefix: string;
  pattern: string;   // regex source string
  example: string;
  description: string;
}

export const idFormats: Record<string, IdFormat> = {
  session: { prefix: "sess_", pattern: "^sess_[a-z2-7]{26}$", example: "sess_abc123def456ghi789jkl012mno", description: "..." },
  turn: { prefix: "turn_", pattern: "^turn_[a-z2-7]{26}$", example: "turn_xyz789abc012def345ghi678jkl", description: "..." },
  event: { prefix: "evt_", pattern: "^evt_[a-z2-7]{26}$", example: "evt_qrs456tuv789wxy012abc345def", description: "..." },
  permission: { prefix: "perm_", pattern: "^perm_[a-z2-7]{26}$", example: "perm_mno012pqr345stu678vwx901yza", description: "..." },
  queueReceipt: { prefix: "rcpt_", pattern: "^rcpt_[a-z2-7]{26}$", example: "rcpt_bcd123efg456hij789klm012nop", description: "..." },
  credential: { prefix: "cred_", pattern: "^cred_[a-z2-7]{26}$", example: "cred_pqr456stu789vwx012abc345def", description: "..." },
  project: { prefix: "proj_", pattern: "^proj_[a-f0-9]{40}$", example: "proj_a1b2c3d4e5f6789012345678901234567890abcd", description: "..." },
  model: { prefix: "", pattern: "^[a-z0-9-]+/[a-z0-9.-]+$", example: "xai/grok-4.5", description: "..." },
  mcpSource: { prefix: "mcp_", pattern: "^mcp_[a-f0-9]{40}$", example: "mcp_f1e2d3c4b5a697887766554433221100ffeeddcc", description: "..." },
};
```

For `errors.ts`:

```ts
export interface ErrorDefinition {
  code: string;
  category: "runtime" | "validation" | "state" | "auth" | "capability" | "timing" | "compatibility" | "recovery";
  severity: "error" | "warning";
  recoverable: boolean;
  retryable: boolean;
  messageKey: string;
  description: string;
}

export const errorRegistry: Record<string, ErrorDefinition> = {
  runtime_unavailable: { code: "runtime_unavailable", category: "runtime", severity: "error", recoverable: false, retryable: false, messageKey: "runtime.unavailable", description: "Runtime is not connected" },
  invalid_params: { code: "invalid_params", category: "validation", severity: "error", recoverable: true, retryable: false, messageKey: "validation.invalidParams", description: "Request params failed schema validation" },
  not_found: { code: "not_found", category: "state", severity: "error", recoverable: true, retryable: false, messageKey: "state.notFound", description: "Referenced entity does not exist" },
  auth_failed: { code: "auth_failed", category: "auth", severity: "error", recoverable: true, retryable: false, messageKey: "auth.failed", description: "Authentication completed but failed" },
  capability_missing: { code: "capability_missing", category: "capability", severity: "error", recoverable: false, retryable: false, messageKey: "capability.missing", description: "Required capability not negotiated" },
  too_late: { code: "too_late", category: "timing", severity: "warning", recoverable: false, retryable: false, messageKey: "timing.tooLate", description: "Steer arrived after turn boundary" },
  schema_digest_mismatch: { code: "schema_digest_mismatch", category: "compatibility", severity: "error", recoverable: false, retryable: false, messageKey: "compat.schemaDigestMismatch", description: "Schema digest in initialize does not match local digest" },
  unknown_method: { code: "unknown_method", category: "compatibility", severity: "error", recoverable: false, retryable: false, messageKey: "compat.unknownMethod", description: "Method name not in registry" },
  journal_gap: { code: "journal_gap", category: "recovery", severity: "warning", recoverable: true, retryable: true, messageKey: "recovery.journalGap", description: "Event journal has a gap in stable event IDs" },
};
```

For `pagination.ts`:

```ts
export const cursorSchema = {
  type: "object",
  properties: {
    cursor: { type: "string", description: "Opaque pagination cursor" },
    limit: { type: "integer", minimum: 1, maximum: 5000 },
  },
  additionalProperties: false,
};

export const pageResultSchema = (itemSchema: object) => ({
  type: "object",
  properties: {
    items: { type: "array", items: itemSchema },
    total: { type: "integer", minimum: 0 },
    cursor: { type: ["string", "null"] },
  },
  required: ["items", "total"],
  additionalProperties: false,
});
```

For `journal.ts`:

```ts
export const journalCommitPointSchema = {
  type: "object",
  properties: {
    sessionId: { type: "string", pattern: idFormats.session.pattern },
    commitPoint: { type: "string", description: "Opaque token representing a durable commit boundary" },
    stableEventId: { type: "string", pattern: idFormats.event.pattern },
    sequence: { type: "integer", minimum: 0 },
  },
  required: ["sessionId", "commitPoint", "stableEventId", "sequence"],
  additionalProperties: false,
};

export const replayRequestSchema = {
  type: "object",
  properties: {
    sessionId: { type: "string", pattern: idFormats.session.pattern },
    fromCommitPoint: { type: "string" },
    maxEvents: { type: "integer", minimum: 1, maximum: 10000 },
  },
  required: ["sessionId"],
  additionalProperties: false,
};
```

Implement `methods.ts` with a `methodSchemas: Record<string, MethodSchema>` containing all 24 methods listed in the inventory. Each method's `params` and `result` are JSON Schema objects. Use `$ref`-style internal references where shapes repeat (SessionInfo, UsageReport, etc.) by defining them in a shared `types` section of the bundle.

Implement `notifications.ts` with a `notificationSchemas: Record<string, NotificationSchema>` containing all 4 notifications.

Implement `index.ts` re-exporting everything plus a `computeSchemaDigest()` function that returns `sha256(JSON.stringify(bundle))[:16]`.

- [ ] **Step 3: Implement the codegen script**

Create `packages/coding-agent/scripts/gen-desktop-v1-schema.ts`:

```ts
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { methodSchemas, notificationSchemas, errorRegistry, idFormats } from "../src/modes/acp/desktop-v1/schema/index.ts";

const bundle = {
  schemaVersion: 1,
  generatedAt: new Date().toISOString(),
  methods: methodSchemas,
  notifications: notificationSchemas,
  errors: errorRegistry,
  ids: idFormats,
};

const outPath = join(import.meta.dir, "..", "src", "modes", "acp", "desktop-v1", "schema", "generated", "schema-bundle.json");
writeFileSync(outPath, JSON.stringify(bundle, null, 2) + "\n");
console.log(`Schema bundle written to ${outPath}`);
console.log(`Digest: ${computeSchemaDigest()}`);
```

Run:

```bash
cd runtime/oh-my-pi && bun run packages/coding-agent/scripts/gen-desktop-v1-schema.ts
```

Expected: `schema-bundle.json` is written and the digest is printed.

- [ ] **Step 4: Run schema tests and commit**

```bash
cd runtime/oh-my-pi && bun test packages/coding-agent/test/desktop-v1/schema.test.ts
```

Expected: all tests PASS.

```bash
git add runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/schema runtime/oh-my-pi/packages/coding-agent/scripts/gen-desktop-v1-schema.ts runtime/oh-my-pi/packages/coding-agent/test/desktop-v1/schema.test.ts
git commit -m "feat: add OMP Desktop v1 Extension Protocol schema bundle"
```

---

### Task 3: Implement the v1 Dispatcher and Error Class (OMP Submodule)

**Files:**
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/errors.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/types.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/dispatcher.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/capability.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/test/desktop-v1/dispatcher.test.ts`

**Interfaces:**
- Consumes: schema bundle from Task 2.
- Produces: `DesktopV1Dispatcher` that validates params against schema, routes to handlers, validates result, and throws `DesktopV1Error` on failure.

- [ ] **Step 1: Write failing dispatcher tests**

Create `packages/coding-agent/test/desktop-v1/dispatcher.test.ts`:

```ts
import { test, expect } from "bun:test";
import { DesktopV1Dispatcher } from "../../src/modes/acp/desktop-v1/dispatcher.ts";
import { DesktopV1Error } from "../../src/modes/acp/desktop-v1/errors.ts";

test("dispatcher rejects unknown method", async () => {
  const dispatcher = new DesktopV1Dispatcher(new Map());
  await expect(dispatcher.dispatch("_omp/desktop/v1/nonexistent", {})).rejects.toThrow(DesktopV1Error);
  await expect(dispatcher.dispatch("_omp/desktop/v1/nonexistent", {})).rejects.toMatchObject({ code: "unknown_method" });
});

test("dispatcher validates params against schema", async () => {
  const handlers = new Map([["sessions.byCwd", async () => ({ sessions: [] })]]);
  const dispatcher = new DesktopV1Dispatcher(handlers);
  await expect(dispatcher.dispatch("_omp/desktop/v1/sessions.byCwd", {})).rejects.toMatchObject({ code: "invalid_params" });
});

test("dispatcher returns result on valid call", async () => {
  const handlers = new Map([["sessions.byCwd", async (params: { cwd: string }) => ({ sessions: [], cursor: null })]]);
  const dispatcher = new DesktopV1Dispatcher(handlers);
  const result = await dispatcher.dispatch("_omp/desktop/v1/sessions.byCwd", { cwd: "/tmp" });
  expect(result).toEqual({ sessions: [], cursor: null });
});
```

Run `cd runtime/oh-my-pi && bun test packages/coding-agent/test/desktop-v1/dispatcher.test.ts`.

Expected: FAIL with module not found.

- [ ] **Step 2: Implement the error class**

Create `packages/coding-agent/src/modes/acp/desktop-v1/errors.ts`:

```ts
import { errorRegistry } from "./schema/errors.ts";

export interface DesktopV1ErrorData {
  code: string;
  message: string;
  messageKey: string;
  args?: Record<string, unknown>;
  recoverable: boolean;
  retryable: boolean;
  details?: unknown;
}

export class DesktopV1Error extends Error {
  readonly code: string;
  readonly messageKey: string;
  readonly args: Record<string, unknown>;
  readonly recoverable: boolean;
  readonly retryable: boolean;
  readonly details: unknown;

  constructor(code: string, args: Record<string, unknown> = {}, details?: unknown) {
    const def = errorRegistry[code];
    if (!def) throw new Error(`Unknown error code: ${code}`);
    super(def.messageKey);
    this.name = "DesktopV1Error";
    this.code = code;
    this.messageKey = def.messageKey;
    this.args = args;
    this.recoverable = def.recoverable;
    this.retryable = def.retryable;
    this.details = details;
  }

  toJSON(): DesktopV1ErrorData {
    return {
      code: this.code,
      message: this.message,
      messageKey: this.messageKey,
      args: this.args,
      recoverable: this.recoverable,
      retryable: this.retryable,
      details: this.details,
    };
  }
}
```

- [ ] **Step 3: Implement the dispatcher**

Create `packages/coding-agent/src/modes/acp/desktop-v1/dispatcher.ts`:

```ts
import { methodSchemas } from "./schema/methods.ts";
import { DesktopV1Error } from "./errors.ts";

export type Handler = (params: Record<string, unknown>) => Promise<Record<string, unknown>>;

const NAMESPACE = "_omp/desktop/v1/";
const METHOD_PREFIX = "_omp/desktop/v1/";

export class DesktopV1Dispatcher {
  readonly schemaDigest: string;

  constructor(private handlers: Map<string, Handler>) {
    this.schemaDigest = computeSchemaDigest();
  }

  async dispatch(method: string, params: Record<string, unknown>): Promise<Record<string, unknown>> {
    if (!method.startsWith(METHOD_PREFIX)) {
      throw new DesktopV1Error("unknown_method", { method });
    }
    const shortName = method.slice(METHOD_PREFIX.length);
    const schema = methodSchemas[shortName];
    if (!schema) {
      throw new DesktopV1Error("unknown_method", { method });
    }
    const handler = this.handlers.get(shortName);
    if (!handler) {
      throw new DesktopV1Error("capability_missing", { method: shortName });
    }
    // Validate params
    const validation = validateJsonSchema(params, schema.params);
    if (!validation.valid) {
      throw new DesktopV1Error("invalid_params", { method: shortName, errors: validation.errors });
    }
    const result = await handler(params);
    // Validate result
    const resultValidation = validateJsonSchema(result, schema.result);
    if (!resultValidation.valid) {
      throw new Error(`Internal: result schema violation for ${shortName}: ${resultValidation.errors}`);
    }
    return result;
  }
}

// Minimal JSON Schema validator (subset of 2020-12)
function validateJsonSchema(value: unknown, schema: object): { valid: boolean; errors?: string[] } {
  // Implementation: a minimal validator covering type, properties, required, additionalProperties,
  // pattern, minimum, maximum, enum, items, and $ref within the bundle.
  // For Plan 2, use a focused validator; full ajv can be added if needed.
  // ... (implementation details)
  return { valid: true };
}

function computeSchemaDigest(): string {
  // sha256 of the canonical JSON of the method schemas
  return "placeholder_digest_0000";
}
```

Note: the `validateJsonSchema` and `computeSchemaDigest` functions should be fully implemented. For `validateJsonSchema`, implement a focused validator covering `type`, `properties`, `required`, `additionalProperties`, `pattern`, `minimum`, `maximum`, `enum`, `items`, and `oneOf`. For `computeSchemaDigest`, use `node:crypto` to compute `sha256(JSON.stringify(methodSchemas)).slice(0, 16)`.

- [ ] **Step 4: Implement the capability descriptor**

Create `packages/coding-agent/src/modes/acp/desktop-v1/capability.ts`:

```ts
import { methodSchemas } from "./schema/methods.ts";
import { notificationSchemas } from "./schema/notifications.ts";
import { DesktopV1Dispatcher } from "./dispatcher.ts";

export interface DesktopV1Capability {
  schemaVersion: 1;
  schemaDigest: string;
  methods: string[];
  notifications: string[];
  optionalFeatures: string[];  // e.g. ["queue", "steer"] — present but may return runtime_unavailable
}

export function buildDesktopV1Capability(dispatcher: DesktopV1Dispatcher): DesktopV1Capability {
  return {
    schemaVersion: 1,
    schemaDigest: dispatcher.schemaDigest,
    methods: Object.keys(methodSchemas),
    notifications: Object.keys(notificationSchemas),
    optionalFeatures: ["queue", "steer"],
  };
}
```

- [ ] **Step 5: Run tests and commit**

```bash
cd runtime/oh-my-pi && bun test packages/coding-agent/test/desktop-v1/dispatcher.test.ts
git add runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/{errors,types,dispatcher,capability}.ts runtime/oh-my-pi/packages/coding-agent/test/desktop-v1/dispatcher.test.ts
git commit -m "feat: add OMP Desktop v1 dispatcher and error class"
```

---

### Task 4: Implement v1 Handlers (OMP Submodule)

**Files:**
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/sessions.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/usage.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/extensions.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/providers.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/credentials.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/mcp.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/sessionConfig.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/queue.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/steer.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/diagnostics.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/index.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/index.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/test/desktop-v1/contract/*.test.ts` (8 files)

**Interfaces:**
- Consumes: dispatcher from Task 3; existing OMP `SessionManager`, `AuthStorage`, `ModelManager`, extension loader, MCP config.
- Produces: A complete handler registry that the dispatcher can route to.

- [ ] **Step 1: Write failing contract tests for sessions and projects**

Create `packages/coding-agent/test/desktop-v1/contract/sessions.test.ts`:

```ts
import { test, expect, beforeEach } from "bun:test";
import { createDispatcherWithHandlers } from "../helpers.ts";
import { DesktopV1Error } from "../../src/modes/acp/desktop-v1/errors.ts";

// Use a mock SessionManager that returns canned data
const mockSessionManager = {
  listAll: async (limit: number) => ({
    sessions: [{ id: "sess_abc123def456ghi789jkl012mno", cwd: "/tmp", title: "Test", modified: "2026-07-29T00:00:00Z" }],
    total: 1,
  }),
  list: async (cwd: string, limit: number) => ({
    sessions: [{ id: "sess_abc123def456ghi789jkl012mno", cwd, title: "Test", modified: "2026-07-29T00:00:00Z" }],
  }),
  listProjects: async () => ({
    projects: [{ cwd: "/tmp", sessionCount: 1, lastActivityAt: "2026-07-29T00:00:00Z", lastTitle: "Test" }],
    totalSessions: 1,
  }),
};

test("sessions.listAll returns sessions with stable IDs", async () => {
  const dispatcher = createDispatcherWithHandlers(mockSessionManager);
  const result = await dispatcher.dispatch("_omp/desktop/v1/sessions.listAll", { limit: 10 });
  expect(result.sessions).toHaveLength(1);
  expect(result.sessions[0].id).toMatch(/^sess_[a-z2-7]{26}$/);
  expect(result.total).toBe(1);
});

test("sessions.byCwd requires cwd param", async () => {
  const dispatcher = createDispatcherWithHandlers(mockSessionManager);
  await expect(dispatcher.dispatch("_omp/desktop/v1/sessions.byCwd", {})).rejects.toMatchObject({ code: "invalid_params" });
});

test("projects.list returns project buckets", async () => {
  const dispatcher = createDispatcherWithHandlers(mockSessionManager);
  const result = await dispatcher.dispatch("_omp/desktop/v1/projects.list", {});
  expect(result.projects).toHaveLength(1);
  expect(result.totalSessions).toBe(1);
});
```

Run `cd runtime/oh-my-pi && bun test packages/coding-agent/test/desktop-v1/contract/sessions.test.ts`.

Expected: FAIL (handlers not implemented).

- [ ] **Step 2: Implement sessions, projects, usage handlers**

Create `handlers/sessions.ts`:

```ts
import type { SessionManagerLike } from "../types.ts";

export function createSessionsHandlers(sm: SessionManagerLike) {
  return {
    "sessions.listAll": async (params: { limit?: number }) => {
      const limit = Math.min(Math.max(params.limit ?? 1000, 1), 5000);
      const result = await sm.listAll(limit);
      return {
        sessions: result.sessions.map(normalizeSessionInfo),
        total: result.total,
        cursor: result.sessions.length >= limit ? String(result.sessions.length) : null,
      };
    },
    "sessions.byCwd": async (params: { cwd: string; limit?: number }) => {
      const limit = Math.min(Math.max(params.limit ?? 100, 1), 500);
      const result = await sm.list(params.cwd, limit);
      return {
        sessions: result.sessions.map(normalizeSessionInfo),
        cursor: result.sessions.length >= limit ? String(result.sessions.length) : null,
      };
    },
    "projects.list": async () => {
      const result = await sm.listProjects();
      return { projects: result.projects, totalSessions: result.totalSessions };
    },
  };
}

function normalizeSessionInfo(s: any) {
  return {
    id: s.id,
    cwd: s.cwd,
    title: s.title ?? null,
    modified: s.modified,
    parentSession: s.parentSession ?? null,
  };
}
```

Create `handlers/usage.ts`:

```ts
export function createUsageHandlers(getReports: () => Promise<any[]>) {
  return {
    "usage.reports": async () => {
      const reports = await getReports();
      return { reports };
    },
  };
}
```

- [ ] **Step 3: Implement extensions, providers, mcp handlers**

Create `handlers/extensions.ts`:

```ts
export function createExtensionsHandlers(loadAll: (cwd?: string) => Promise<any[]>, toggle: (providerId: string, enabled?: boolean) => Promise<boolean>) {
  return {
    "extensions.list": async (params: { cwd?: string }) => {
      const extensions = await loadAll(params.cwd);
      return { extensions: extensions.map(normalizeExtension) };
    },
    "extensions.toggle": async (params: { providerId: string; enabled?: boolean }) => {
      const enabled = await toggle(params.providerId, params.enabled);
      return { enabled };
    },
  };
}
```

Create `handlers/providers.ts`:

```ts
export function createProvidersHandlers(listProviders: () => Promise<any[]>, listModels: (providerId?: string) => Promise<any[]>) {
  return {
    "providers.list": async () => {
      const providers = await listProviders();
      return { providers: providers.map(normalizeProvider) };
    },
    "providers.models": async (params: { providerId?: string }) => {
      const models = await listModels(params.providerId);
      return { models: models.map(normalizeModel) };
    },
  };
}
```

Create `handlers/mcp.ts`:

```ts
export function createMcpHandlers(listMcp: (cwd?: string) => Promise<any[]>, discoverMcp: (cwd: string) => Promise<any[]>) {
  return {
    "mcp.list": async (params: { cwd?: string }) => {
      const sources = await listMcp(params.cwd);
      return { sources: sources.map(normalizeMcpSource) };
    },
    "mcp.discover": async (params: { cwd: string }) => {
      const sources = await discoverMcp(params.cwd);
      return { sources: sources.map(normalizeMcpSource) };
    },
  };
}
```

- [ ] **Step 4: Implement credentials handlers (metadata only, no secrets)**

Create `handlers/credentials.ts`:

```ts
export function createCredentialsHandlers(authStorage: any) {
  return {
    "credentials.list": async (params: { providerId?: string }) => {
      const creds = await authStorage.listMetadata(params.providerId);
      return { credentials: creds.map((c: any) => ({ ...c, secret: undefined })) };
    },
    "credentials.beginAuth": async (params: { providerId: string; method: string }) => {
      const result = await authStorage.beginAuth(params.providerId, params.method);
      return { authId: result.authId, status: "pending" as const };
    },
    "credentials.completeAuth": async (params: { authId: string; code: string }) => {
      await authStorage.completeAuth(params.authId, params.code);
      return { status: "active" as const };
    },
    "credentials.cancelAuth": async (params: { authId: string }) => {
      await authStorage.cancelAuth(params.authId);
      return { status: "cancelled" as const };
    },
    "credentials.replace": async (params: { credentialId: string }) => {
      await authStorage.replace(params.credentialId);
      return { status: "active" as const };
    },
    "credentials.revoke": async (params: { credentialId: string }) => {
      await authStorage.revoke(params.credentialId);
      return { status: "revoked" as const };
    },
    "credentials.health": async (params: { credentialId?: string }) => {
      const health = await authStorage.health(params.credentialId);
      return health;
    },
    "credentials.migrationStatus": async () => {
      const status = await authStorage.migrationStatus();
      return status;
    },
  };
}
```

- [ ] **Step 5: Implement sessionConfig, queue, steer, diagnostics handlers**

Create `handlers/sessionConfig.ts`:

```ts
export function createSessionConfigHandlers(getConfig: (sessionId?: string) => Promise<any>, setConfig: (sessionId: string | undefined, config: any) => Promise<any>) {
  return {
    "sessionConfig.get": async (params: { sessionId?: string }) => {
      const config = await getConfig(params.sessionId);
      return { config };
    },
    "sessionConfig.set": async (params: { sessionId?: string; config: any }) => {
      const config = await setConfig(params.sessionId, params.config);
      return { config };
    },
  };
}
```

Create `handlers/queue.ts`:

```ts
import { DesktopV1Error } from "../errors.ts";

export function createQueueHandlers() {
  return {
    "queue.enqueue": async (_params: { sessionId: string; prompt: string }) => {
      throw new DesktopV1Error("runtime_unavailable", { reason: "queue requires Supervisor (Plan 3)" });
    },
    "queue.cancel": async (_params: { receiptId: string }) => {
      throw new DesktopV1Error("runtime_unavailable", { reason: "queue requires Supervisor (Plan 3)" });
    },
  };
}
```

Create `handlers/steer.ts`:

```ts
import { DesktopV1Error } from "../errors.ts";

export function createSteerHandlers() {
  return {
    "steer.send": async (_params: { turnId: string; message: string }) => {
      throw new DesktopV1Error("runtime_unavailable", { reason: "steer requires active turn tracking (Plan 3)" });
    },
  };
}
```

Create `handlers/diagnostics.ts`:

```ts
export function createDiagnosticsHandlers(selfCheck: () => Promise<any[]>) {
  return {
    "diagnostics.selfCheck": async () => {
      const checks = await selfCheck();
      return { checks };
    },
  };
}
```

Create `handlers/index.ts`:

```ts
import type { Handler } from "../dispatcher.ts";
import { createSessionsHandlers } from "./sessions.ts";
import { createUsageHandlers } from "./usage.ts";
import { createExtensionsHandlers } from "./extensions.ts";
import { createProvidersHandlers } from "./providers.ts";
import { createCredentialsHandlers } from "./credentials.ts";
import { createMcpHandlers } from "./mcp.ts";
import { createSessionConfigHandlers } from "./sessionConfig.ts";
import { createQueueHandlers } from "./queue.ts";
import { createSteerHandlers } from "./steer.ts";
import { createDiagnosticsHandlers } from "./diagnostics.ts";

export function createAllHandlers(deps: {
  sessionManager: any;
  usageReports: () => Promise<any[]>;
  extensions: { loadAll: (cwd?: string) => Promise<any[]>; toggle: (id: string, enabled?: boolean) => Promise<boolean> };
  providers: { list: () => Promise<any[]>; listModels: (id?: string) => Promise<any[]> };
  authStorage: any;
  mcp: { list: (cwd?: string) => Promise<any[]>; discover: (cwd: string) => Promise<any[]> };
  sessionConfig: { get: (id?: string) => Promise<any>; set: (id: string | undefined, config: any) => Promise<any> };
  diagnostics: { selfCheck: () => Promise<any[]> };
}): Map<string, Handler> {
  const handlers = new Map<string, Handler>();
  for (const [name, handler] of Object.entries(createSessionsHandlers(deps.sessionManager))) handlers.set(name, handler);
  for (const [name, handler] of Object.entries(createUsageHandlers(deps.usageReports))) handlers.set(name, handler);
  for (const [name, handler] of Object.entries(createExtensionsHandlers(deps.extensions.loadAll, deps.extensions.toggle))) handlers.set(name, handler);
  for (const [name, handler] of Object.entries(createProvidersHandlers(deps.providers.list, deps.providers.listModels))) handlers.set(name, handler);
  for (const [name, handler] of Object.entries(createCredentialsHandlers(deps.authStorage))) handlers.set(name, handler);
  for (const [name, handler] of Object.entries(createMcpHandlers(deps.mcp.list, deps.mcp.discover))) handlers.set(name, handler);
  for (const [name, handler] of Object.entries(createSessionConfigHandlers(deps.sessionConfig.get, deps.sessionConfig.set))) handlers.set(name, handler);
  for (const [name, handler] of Object.entries(createQueueHandlers())) handlers.set(name, handler);
  for (const [name, handler] of Object.entries(createSteerHandlers())) handlers.set(name, handler);
  for (const [name, handler] of Object.entries(createDiagnosticsHandlers(deps.diagnostics.selfCheck))) handlers.set(name, handler);
  return handlers;
}
```

- [ ] **Step 6: Implement the index.ts entrypoint**

Create `packages/coding-agent/src/modes/acp/desktop-v1/index.ts`:

```ts
export { DesktopV1Dispatcher } from "./dispatcher.ts";
export type { Handler } from "./dispatcher.ts";
export { DesktopV1Error } from "./errors.ts";
export { buildDesktopV1Capability } from "./capability.ts";
export type { DesktopV1Capability } from "./capability.ts";
export { createAllHandlers } from "./handlers/index.ts";
export { methodSchemas } from "./schema/methods.ts";
export { notificationSchemas } from "./schema/notifications.ts";
export { errorRegistry } from "./schema/errors.ts";
export { idFormats } from "./schema/ids.ts";
```

- [ ] **Step 7: Write and run remaining contract tests**

Write contract tests for usage, extensions, providers, credentials, mcp, queue/steer, diagnostics following the pattern in Step 1. Each test uses mock dependencies and verifies:
- Valid params produce correct results
- Invalid params produce `invalid_params` error
- Runtime-unavailable handlers produce `runtime_unavailable` error

Run:

```bash
cd runtime/oh-my-pi && bun test packages/coding-agent/test/desktop-v1/
```

Expected: all contract tests PASS.

- [ ] **Step 8: Commit the handlers**

```bash
git add runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/index.ts runtime/oh-my-pi/packages/coding-agent/test/desktop-v1/contract
git commit -m "feat: implement OMP Desktop v1 extension handlers"
```

---

### Task 5: Wire the v1 Dispatcher into AcpAgent and Implement the Legacy Compat Shim

**Files:**
- Modify: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/acp-agent.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/compat.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/test/desktop-v1/compat.test.ts`

**Interfaces:**
- Consumes: dispatcher and handlers from Tasks 3–4.
- Produces: `AcpAgent.initialize` advertises `desktopV1` capability; `extMethod` routes `_omp/desktop/v1/*` to the dispatcher; `_omp/*` legacy methods route through `compat.ts`.

- [ ] **Step 1: Write failing compat tests**

Create `packages/coding-agent/test/desktop-v1/compat.test.ts`:

```ts
import { test, expect } from "bun:test";
import { mapLegacyToV1, mapV1ResultToLegacy } from "../../src/modes/acp/desktop-v1/compat.ts";

test("maps _omp/sessions/listAll to _omp/desktop/v1/sessions.listAll", () => {
  const mapped = mapLegacyToV1("_omp/sessions/listAll", { limit: 10 });
  expect(mapped.method).toBe("_omp/desktop/v1/sessions.listAll");
  expect(mapped.params).toEqual({ limit: 10 });
});

test("maps _omp/projects/list to _omp/desktop/v1/projects.list", () => {
  const mapped = mapLegacyToV1("_omp/projects/list", {});
  expect(mapped.method).toBe("_omp/desktop/v1/projects.list");
});

test("maps _omp/chats/byCwd to _omp/desktop/v1/sessions.byCwd", () => {
  const mapped = mapLegacyToV1("_omp/chats/byCwd", { cwd: "/tmp", limit: 5 });
  expect(mapped.method).toBe("_omp/desktop/v1/sessions.byCwd");
  expect(mapped.params).toEqual({ cwd: "/tmp", limit: 5 });
});

test("maps _omp/usage to _omp/desktop/v1/usage.reports", () => {
  const mapped = mapLegacyToV1("_omp/usage", {});
  expect(mapped.method).toBe("_omp/desktop/v1/usage.reports");
});

test("maps _omp/extensions to _omp/desktop/v1/extensions.list", () => {
  const mapped = mapLegacyToV1("_omp/extensions", { cwd: "/tmp" });
  expect(mapped.method).toBe("_omp/desktop/v1/extensions.list");
});

test("maps _omp/extensions/toggle to _omp/desktop/v1/extensions.toggle", () => {
  const mapped = mapLegacyToV1("_omp/extensions/toggle", { providerId: "foo", enabled: true });
  expect(mapped.method).toBe("_omp/desktop/v1/extensions.toggle");
});

test("returns null for unknown legacy methods", () => {
  expect(mapLegacyToV1("_omp/unknown", {})).toBeNull();
});

test("reshapes v1 sessions.listAll result to legacy shape", () => {
  const legacyResult = mapV1ResultToLegacy("_omp/sessions/listAll", {
    sessions: [{ id: "sess_abc123def456ghi789jkl012mno", cwd: "/tmp", title: "Test", modified: "2026-07-29T00:00:00Z", parentSession: null }],
    total: 1,
    cursor: null,
  });
  expect(legacyResult).toEqual({
    sessions: [{ id: "sess_abc123def456ghi789jkl012mno", cwd: "/tmp", title: "Test", modified: "2026-07-29T00:00:00Z" }],
    total: 1,
  });
});
```

Run `cd runtime/oh-my-pi && bun test packages/coding-agent/test/desktop-v1/compat.test.ts`.

Expected: FAIL (compat module missing).

- [ ] **Step 2: Implement the compat shim**

Create `packages/coding-agent/src/modes/acp/desktop-v1/compat.ts`:

```ts
const LEGACY_TO_V1: Record<string, string> = {
  "_omp/sessions/listAll": "_omp/desktop/v1/sessions.listAll",
  "_omp/projects/list": "_omp/desktop/v1/projects.list",
  "_omp/chats/byCwd": "_omp/desktop/v1/sessions.byCwd",
  "_omp/usage": "_omp/desktop/v1/usage.reports",
  "_omp/extensions": "_omp/desktop/v1/extensions.list",
  "_omp/extensions/toggle": "_omp/desktop/v1/extensions.toggle",
};

export function mapLegacyToV1(method: string, params: Record<string, unknown>): { method: string; params: Record<string, unknown> } | null {
  const v1Method = LEGACY_TO_V1[method];
  if (!v1Method) return null;
  return { method: v1Method, params };
}

export function mapV1ResultToLegacy(legacyMethod: string, v1Result: Record<string, unknown>): Record<string, unknown> {
  switch (legacyMethod) {
    case "_omp/sessions/listAll":
      return {
        sessions: (v1Result.sessions as any[]).map((s) => ({ id: s.id, cwd: s.cwd, title: s.title, modified: s.modified })),
        total: v1Result.total,
      };
    case "_omp/projects/list":
      return { projects: v1Result.projects, totalSessions: v1Result.totalSessions };
    case "_omp/chats/byCwd":
      return { sessions: (v1Result.sessions as any[]).map((s) => ({ id: s.id, cwd: s.cwd, title: s.title, modified: s.modified })) };
    case "_omp/usage":
      return { reports: v1Result.reports };
    case "_omp/extensions":
      return { extensions: v1Result.extensions };
    case "_omp/extensions/toggle":
      return { enabled: v1Result.enabled };
    default:
      return v1Result;
  }
}
```

- [ ] **Step 3: Wire the dispatcher into AcpAgent**

Modify `packages/coding-agent/src/modes/acp/acp-agent.ts`:

1. At the top, add imports:

```ts
import { DesktopV1Dispatcher, buildDesktopV1Capability, createAllHandlers, type DesktopV1Capability } from "./desktop-v1/index.ts";
import { mapLegacyToV1, mapV1ResultToLegacy } from "./desktop-v1/compat.ts";
```

2. In the `initialize` method, after the existing `agentCapabilities` object, add the `desktopV1` capability when `OMP_DESKTOP_V1_PROTOCOL=1`:

```ts
const desktopV1Enabled = process.env.OMP_DESKTOP_V1_PROTOCOL === "1";
let desktopV1Capability: DesktopV1Capability | undefined;
if (desktopV1Enabled) {
  const handlers = createAllHandlers({
    sessionManager: this.#session.sessionManager,
    usageReports: () => this.#session.fetchUsageReports(),
    extensions: { loadAll: (cwd) => loadAllExtensions(cwd, this.#settings.disabledExtensions), toggle: (id, enabled) => enabled ? enableProvider(id) : disableProvider(id) },
    providers: { list: () => listProviders(), listModels: (id) => listModels(id) },
    authStorage: this.#authStorage,
    mcp: { list: (cwd) => listMcpSources(cwd), discover: (cwd) => discoverMcp(cwd) },
    sessionConfig: { get: (id) => getSessionConfig(id), set: (id, config) => setSessionConfig(id, config) },
    diagnostics: { selfCheck: () => selfCheck() },
  });
  const dispatcher = new DesktopV1Dispatcher(handlers);
  desktopV1Capability = buildDesktopV1Capability(dispatcher);
  this.#desktopV1Dispatcher = dispatcher;
}
```

3. Add `desktopV1` to the returned `agentCapabilities`:

```ts
agentCapabilities: {
  ...existing,
  desktopV1: desktopV1Capability ?? null,
},
```

4. In `extMethod`, add a branch before the legacy `_omp/*` switch:

```ts
if (method.startsWith("_omp/desktop/v1/") && this.#desktopV1Dispatcher) {
  try {
    return await this.#desktopV1Dispatcher.dispatch(method, params);
  } catch (e) {
    if (e instanceof DesktopV1Error) {
      throw new AcpError(e.code, e.message, e.toJSON());
    }
    throw e;
  }
}
```

5. In `extMethod`, add legacy compat routing before the existing switch:

```ts
const legacyMapped = mapLegacyToV1(method, params);
if (legacyMapped && this.#desktopV1Dispatcher) {
  try {
    const v1Result = await this.#desktopV1Dispatcher.dispatch(legacyMapped.method, legacyMapped.params);
    return mapV1ResultToLegacy(method, v1Result);
  } catch (e) {
    if (e instanceof DesktopV1Error) {
      throw new AcpError(e.code, e.message, e.toJSON());
    }
    throw e;
  }
}
// Fall through to existing legacy switch for backward compatibility when v1 is not enabled
```

- [ ] **Step 4: Run all tests**

```bash
cd runtime/oh-my-pi && OMP_DESKTOP_V1_PROTOCOL=1 bun test packages/coding-agent/test/desktop-v1/
cd runtime/oh-my-pi && bun test packages/coding-agent/test/acp-agent.test.ts
```

Expected: all tests PASS, including existing `_omp/*` tests (which should still pass via the legacy fallback path when v1 is not enabled).

- [ ] **Step 5: Commit**

```bash
git add runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/compat.ts runtime/oh-my-pi/packages/coding-agent/src/modes/acp/acp-agent.ts runtime/oh-my-pi/packages/coding-agent/test/desktop-v1/compat.test.ts
git commit -m "feat: wire v1 dispatcher into AcpAgent with legacy compat"
```

---

### Task 6: Implement the Rust OmpExtension Client

**Files:**
- Create: `src-tauri/src/omp_desktop_v1/mod.rs`
- Create: `src-tauri/src/omp_desktop_v1/capability.rs`
- Create: `src-tauri/src/omp_desktop_v1/errors.rs`
- Create: `src-tauri/src/omp_desktop_v1/ids.rs`
- Create: `src-tauri/src/omp_desktop_v1/generated.rs`
- Create: `src-tauri/src/omp_desktop_v1/build.rs`
- Create: `src-tauri/src/omp_desktop_v1/tests/contract.rs`
- Create: `src-tauri/src/omp_desktop_v1/tests/fixtures/schema-bundle.json`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: schema bundle JSON from Task 2; ACP client transport from Plan 1.
- Produces: `OmpExtension` struct that negotiates capability, dispatches typed requests, and returns typed results or `DesktopV1Error`.

- [ ] **Step 1: Add Cargo dependencies**

In `src-tauri/Cargo.toml`, under `[dependencies]`:

```toml
schemars = "0.8"
```

(`serde` and `serde_json` are already present.)

- [ ] **Step 2: Write the failing Rust contract test**

Create `src-tauri/src/omp_desktop_v1/tests/contract.rs`:

```rust
use super::*;
use crate::mock_acp;

#[tokio::test]
async fn extension_client_returns_unavailable_when_capability_absent() {
    let client = OmpExtension::new();
    let result = client.request("sessions.listAll", serde_json::json!({})).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code, "runtime_unavailable");
}

#[tokio::test]
async fn extension_client_validates_method_name() {
    let client = OmpExtension::new();
    let result = client.request("nonexistent.method", serde_json::json!({})).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code, "unknown_method");
}

#[test]
fn stable_id_patterns_are_compiled() {
    assert!(ID_PATTERNS.session.is_match("sess_abc123def456ghi789jkl012mno"));
    assert!(!ID_PATTERNS.session.is_match("invalid"));
}
```

Run `cargo test --manifest-path src-tauri/Cargo.toml omp_desktop_v1 --locked`.

Expected: FAIL (module not found).

- [ ] **Step 3: Implement the generated types module**

Create `src-tauri/src/omp_desktop_v1/generated.rs`. This file contains `serde`-compatible structs mirroring the TS schema. For Plan 2, hand-write the types to match the schema bundle (the codegen script produces the JSON; a human verifies the Rust types match). The types are:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub cwd: String,
    pub title: Option<String>,
    pub modified: String,
    pub parent_session: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub cwd: String,
    pub session_count: u32,
    pub last_activity_at: String,
    pub last_title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub auth_methods: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub provider_id: String,
    pub display_name: String,
    pub context_window: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialMetadata {
    pub id: String,
    pub provider_id: String,
    pub status: String,
    // Never includes secret
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpSourceInfo {
    pub id: String,
    pub name: String,
    pub source_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionInfo {
    pub id: String,
    pub provider_id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopV1Capability {
    pub schema_version: u32,
    pub schema_digest: String,
    pub methods: Vec<String>,
    pub notifications: Vec<String>,
    pub optional_features: Vec<String>,
}
```

- [ ] **Step 4: Implement the error type**

Create `src-tauri/src/omp_desktop_v1/errors.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopV1Error {
    pub code: String,
    pub message: String,
    pub message_key: String,
    pub args: serde_json::Value,
    pub recoverable: bool,
    pub retryable: bool,
    pub details: Option<serde_json::Value>,
}

impl DesktopV1Error {
    pub fn new(code: &str, args: serde_json::Value) -> Self {
        let (message_key, recoverable, retryable) = match code {
            "runtime_unavailable" => ("runtime.unavailable", false, false),
            "invalid_params" => ("validation.invalidParams", true, false),
            "not_found" => ("state.notFound", true, false),
            "auth_failed" => ("auth.failed", true, false),
            "capability_missing" => ("capability.missing", false, false),
            "too_late" => ("timing.tooLate", false, false),
            "schema_digest_mismatch" => ("compat.schemaDigestMismatch", false, false),
            "unknown_method" => ("compat.unknownMethod", false, false),
            "journal_gap" => ("recovery.journalGap", true, true),
            _ => ("unknown", false, false),
        };
        Self {
            code: code.to_string(),
            message: message_key.to_string(),
            message_key: message_key.to_string(),
            args,
            recoverable,
            retryable,
            details: None,
        }
    }

    pub fn runtime_unavailable() -> Self {
        Self::new("runtime_unavailable", serde_json::json!({}))
    }
}

impl std::fmt::Display for DesktopV1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DesktopV1Error({}): {}", self.code, self.message_key)
    }
}

impl std::error::Error for DesktopV1Error {}
```

- [ ] **Step 5: Implement the stable ID patterns**

Create `src-tauri/src/omp_desktop_v1/ids.rs`:

```rust
use regex::Regex;
use std::sync::OnceLock;

pub struct IdPatterns {
    pub session: Regex,
    pub turn: Regex,
    pub event: Regex,
    pub permission: Regex,
    pub queue_receipt: Regex,
    pub credential: Regex,
    pub project: Regex,
    pub model: Regex,
    pub mcp_source: Regex,
}

static ID_PATTERNS: OnceLock<IdPatterns> = OnceLock::new();

pub fn id_patterns() -> &'static IdPatterns {
    ID_PATTERNS.get_or_init(|| IdPatterns {
        session: Regex::new(r"^sess_[a-z2-7]{26}$").unwrap(),
        turn: Regex::new(r"^turn_[a-z2-7]{26}$").unwrap(),
        event: Regex::new(r"^evt_[a-z2-7]{26}$").unwrap(),
        permission: Regex::new(r"^perm_[a-z2-7]{26}$").unwrap(),
        queue_receipt: Regex::new(r"^rcpt_[a-z2-7]{26}$").unwrap(),
        credential: Regex::new(r"^cred_[a-z2-7]{26}$").unwrap(),
        project: Regex::new(r"^proj_[a-f0-9]{40}$").unwrap(),
        model: Regex::new(r"^[a-z0-9-]+/[a-z0-9.-]+$").unwrap(),
        mcp_source: Regex::new(r"^mcp_[a-f0-9]{40}$").unwrap(),
    })
}
```

Add `regex = "1"` to `Cargo.toml` if not already present.

- [ ] **Step 6: Implement the OmpExtension client**

Create `src-tauri/src/omp_desktop_v1/mod.rs`:

```rust
pub mod capability;
pub mod errors;
pub mod generated;
pub mod ids;

use errors::DesktopV1Error;
use generated::DesktopV1Capability;
use std::sync::Arc;
use tokio::sync::RwLock;

const NAMESPACE: &str = "_omp/desktop/v1/";

pub struct OmpExtension {
    capability: Arc<RwLock<Option<DesktopV1Capability>>>,
    // When the ACP client is connected, this holds a reference to send requests
    // For Plan 2, this is always None (fail-closed); Plan 3 wires the real client.
}

impl OmpExtension {
    pub fn new() -> Self {
        Self {
            capability: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn negotiate_capability(&self, cap: Option<DesktopV1Capability>) {
        *self.capability.write().await = cap;
    }

    pub async fn has_capability(&self) -> bool {
        self.capability.read().await.is_some()
    }

    pub async fn request(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value, DesktopV1Error> {
        let cap = self.capability.read().await;
        if cap.is_none() {
            return Err(DesktopV1Error::runtime_unavailable());
        }
        let cap = cap.as_ref().unwrap();
        let full_method = format!("{NAMESPACE}{method}");
        if !cap.methods.contains(&full_method) {
            return Err(DesktopV1Error::new("unknown_method", serde_json::json!({ "method": full_method })));
        }
        // In Plan 2, we don't have a real ACP transport wired.
        // Plan 3 will inject the AcpClient reference here.
        Err(DesktopV1Error::runtime_unavailable())
    }
}

impl Default for OmpExtension {
    fn default() -> Self {
        Self::new()
    }
}
```

Create `src-tauri/src/omp_desktop_v1/capability.rs`:

```rust
pub use super::generated::DesktopV1Capability;
```

- [ ] **Step 7: Register the module and run tests**

In `src-tauri/src/lib.rs`, add:

```rust
mod omp_desktop_v1;
```

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml omp_desktop_v1 --locked
```

Expected: all tests PASS.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/omp_desktop_v1 src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat: add Rust OmpExtension client for v1 protocol"
```

---

### Task 7: Remove Dead `x.ai/rewind/*` Bindings and Fix Stale Documentation

**Files:**
- Modify: `src-tauri/src/acp_client.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: Plan 1 audit finding (dead `x.ai/rewind/*` bindings at acp_client.rs:1157–1197).
- Produces: clean acp_client.rs with no private extension bindings; accurate module docs.

- [ ] **Step 1: Add a regression test**

In `src-tauri/src/acp_client.rs` test module (or adjacent), add:

```rust
#[test]
fn no_private_xai_bindings_remain() {
    let source = include_str!("acp_client.rs");
    assert!(!source.contains("x.ai/rewind/points"), "x.ai/rewind/points binding must be removed");
    assert!(!source.contains("x.ai/rewind/execute"), "x.ai/rewind/execute binding must be removed");
}
```

Run `cargo test --manifest-path src-tauri/Cargo.toml no_private_xai_bindings_remain --locked`.

Expected: FAIL.

- [ ] **Step 2: Remove the dead bindings**

In `src-tauri/src/acp_client.rs`, delete the `rewind_points` method (lines ~1157–1168) and `rewind_execute` method (lines ~1176–1197). Also remove any associated constants, types, or test fixtures that only reference these methods.

- [ ] **Step 3: Fix the stale header comment and lib.rs doc**

Update `src-tauri/src/acp_client.rs` header comment (lines 1–7) to accurately state:

```rust
//! ACP client — JSON-RPC framing and transport for the OMP Runtime.
//!
//! Plan 1: fail-closed shell. All spawn paths return `runtime_unavailable`.
//! Plan 2: `OmpExtension` client added for versioned `_omp/desktop/v1/*` protocol.
//! No private extension bindings (`_x.ai/*`, `x.ai/rewind/*`) remain.
```

Update `src-tauri/src/lib.rs` line 1 module doc from:

```rust
//! OMP Desktop Host — real ACP default (`grok agent stdio`).
```

to:

```rust
//! OMP Desktop Host — Tauri application entrypoint.
```

- [ ] **Step 4: Run full Rust verification**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

Expected: all tests PASS, including the new regression test.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/acp_client.rs src-tauri/src/lib.rs
git commit -m "refactor: remove dead x.ai/rewind bindings and fix stale docs"
```

---

### Task 8: Implement the Frontend Typed Client

**Files:**
- Create: `src/lib/ompDesktopV1/index.ts`
- Create: `src/lib/ompDesktopV1/capability.ts`
- Create: `src/lib/ompDesktopV1/methods.ts`
- Create: `src/lib/ompDesktopV1/errors.ts`
- Create: `src/lib/ompDesktopV1/contract.test.ts`
- Modify: `src/lib/api.ts`

**Interfaces:**
- Consumes: Rust `OmpExtension` Tauri command from Task 6; existing `invoke` wrapper.
- Produces: Typed TS client that Desktop components use to call v1 methods.

- [ ] **Step 1: Write failing frontend contract tests**

Create `src/lib/ompDesktopV1/contract.test.ts`:

```ts
import { describe, expect, it, vi } from "vitest";
import { OmpDesktopV1Client } from "./index";

describe("OmpDesktopV1Client", () => {
  it("returns runtime_unavailable when capability is not negotiated", async () => {
    const client = new OmpDesktopV1Client();
    const result = await client.call("sessions.listAll", {});
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.code).toBe("runtime_unavailable");
    }
  });

  it("rejects unknown methods", async () => {
    const client = new OmpDesktopV1Client();
    const result = await client.call("nonexistent", {});
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.code).toBe("unknown_method");
    }
  });
});
```

Run `pnpm test -- src/lib/ompDesktopV1/contract.test.ts`.

Expected: FAIL (module not found).

- [ ] **Step 2: Implement the typed method signatures**

Create `src/lib/ompDesktopV1/methods.ts`:

```ts
export interface SessionInfo {
  id: string;
  cwd: string;
  title: string | null;
  modified: string;
  parentSession: string | null;
}

export interface ProjectInfo {
  cwd: string;
  sessionCount: number;
  lastActivityAt: string;
  lastTitle: string | null;
}

export interface ProviderInfo {
  id: string;
  name: string;
  authMethods: string[];
}

export interface ModelInfo {
  id: string;
  providerId: string;
  displayName: string;
  contextWindow: number | null;
}

export interface CredentialMetadata {
  id: string;
  providerId: string;
  status: string;
}

export interface McpSourceInfo {
  id: string;
  name: string;
  sourceType: string;
}

export interface ExtensionInfo {
  id: string;
  providerId: string;
  enabled: boolean;
}

export interface UsageReport {
  providerId: string;
  modelId: string;
  inputTokens: number;
  outputTokens: number;
  timestamp: string;
}

// Method parameter and result types
export interface SessionsListAllParams { limit?: number }
export interface SessionsListAllResult { sessions: SessionInfo[]; total: number; cursor: string | null }
export interface SessionsByCwdParams { cwd: string; limit?: number }
export interface SessionsByCwdResult { sessions: SessionInfo[]; cursor: string | null }
export interface ProjectsListParams {}
export interface ProjectsListResult { projects: ProjectInfo[]; totalSessions: number }
export interface UsageReportsParams {}
export interface UsageReportsResult { reports: UsageReport[] }
export interface ExtensionsListParams { cwd?: string }
export interface ExtensionsListResult { extensions: ExtensionInfo[] }
export interface ExtensionsToggleParams { providerId: string; enabled?: boolean }
export interface ExtensionsToggleResult { enabled: boolean }
export interface ProvidersListParams {}
export interface ProvidersListResult { providers: ProviderInfo[] }
export interface ProvidersModelsParams { providerId?: string }
export interface ProvidersModelsResult { models: ModelInfo[] }
export interface CredentialsListParams { providerId?: string }
export interface CredentialsListResult { credentials: CredentialMetadata[] }
export interface CredentialsBeginAuthParams { providerId: string; method: string }
export interface CredentialsBeginAuthResult { authId: string; status: "pending" }
export interface CredentialsCompleteAuthParams { authId: string; code: string }
export interface CredentialsCompleteAuthResult { status: "active" }
export interface CredentialsCancelAuthParams { authId: string }
export interface CredentialsCancelAuthResult { status: "cancelled" }
export interface CredentialsReplaceParams { credentialId: string }
export interface CredentialsReplaceResult { status: "active" }
export interface CredentialsRevokeParams { credentialId: string }
export interface CredentialsRevokeResult { status: "revoked" }
export interface CredentialsHealthParams { credentialId?: string }
export interface CredentialsHealthResult { healthy: string[]; unhealthy: string[] }
export interface CredentialsMigrationStatusParams {}
export interface CredentialsMigrationStatusResult { migrated: number; pending: number; failed: number; details: unknown[] }
export interface McpListParams { cwd?: string }
export interface McpListResult { sources: McpSourceInfo[] }
export interface McpDiscoverParams { cwd: string }
export interface McpDiscoverResult { sources: McpSourceInfo[] }
export interface SessionConfigGetParams { sessionId?: string }
export interface SessionConfigGetResult { config: unknown }
export interface SessionConfigSetParams { sessionId?: string; config: unknown }
export interface SessionConfigSetResult { config: unknown }
export interface QueueEnqueueParams { sessionId: string; prompt: string }
export interface QueueEnqueueResult { receiptId: string; status: "queued" }
export interface QueueCancelParams { receiptId: string }
export interface QueueCancelResult { status: "cancelled" }
export interface SteerSendParams { turnId: string; message: string }
export interface SteerSendResult { status: "accepted" }
export interface DiagnosticsSelfCheckParams {}
export interface DiagnosticsSelfCheckResult { checks: unknown[] }

export interface MethodMap {
  "sessions.listAll": { params: SessionsListAllParams; result: SessionsListAllResult };
  "sessions.byCwd": { params: SessionsByCwdParams; result: SessionsByCwdResult };
  "projects.list": { params: ProjectsListParams; result: ProjectsListResult };
  "usage.reports": { params: UsageReportsParams; result: UsageReportsResult };
  "extensions.list": { params: ExtensionsListParams; result: ExtensionsListResult };
  "extensions.toggle": { params: ExtensionsToggleParams; result: ExtensionsToggleResult };
  "providers.list": { params: ProvidersListParams; result: ProvidersListResult };
  "providers.models": { params: ProvidersModelsParams; result: ProvidersModelsResult };
  "credentials.list": { params: CredentialsListParams; result: CredentialsListResult };
  "credentials.beginAuth": { params: CredentialsBeginAuthParams; result: CredentialsBeginAuthResult };
  "credentials.completeAuth": { params: CredentialsCompleteAuthParams; result: CredentialsCompleteAuthResult };
  "credentials.cancelAuth": { params: CredentialsCancelAuthParams; result: CredentialsCancelAuthResult };
  "credentials.replace": { params: CredentialsReplaceParams; result: CredentialsReplaceResult };
  "credentials.revoke": { params: CredentialsRevokeParams; result: CredentialsRevokeResult };
  "credentials.health": { params: CredentialsHealthParams; result: CredentialsHealthResult };
  "credentials.migrationStatus": { params: CredentialsMigrationStatusParams; result: CredentialsMigrationStatusResult };
  "mcp.list": { params: McpListParams; result: McpListResult };
  "mcp.discover": { params: McpDiscoverParams; result: McpDiscoverResult };
  "sessionConfig.get": { params: SessionConfigGetParams; result: SessionConfigGetResult };
  "sessionConfig.set": { params: SessionConfigSetParams; result: SessionConfigSetResult };
  "queue.enqueue": { params: QueueEnqueueParams; result: QueueEnqueueResult };
  "queue.cancel": { params: QueueCancelParams; result: QueueCancelResult };
  "steer.send": { params: SteerSendParams; result: SteerSendResult };
  "diagnostics.selfCheck": { params: DiagnosticsSelfCheckParams; result: DiagnosticsSelfCheckResult };
}
```

- [ ] **Step 3: Implement the error type**

Create `src/lib/ompDesktopV1/errors.ts`:

```ts
export interface DesktopV1Error {
  code: string;
  message: string;
  messageKey: string;
  args: Record<string, unknown>;
  recoverable: boolean;
  retryable: boolean;
  details?: unknown;
}

export function isDesktopV1Error(value: unknown): value is DesktopV1Error {
  return typeof value === "object" && value !== null && "code" in value && "messageKey" in value;
}

export const RUNTIME_UNAVAILABLE: DesktopV1Error = {
  code: "runtime_unavailable",
  message: "runtime.unavailable",
  messageKey: "runtime.unavailable",
  args: {},
  recoverable: false,
  retryable: false,
};
```

- [ ] **Step 4: Implement the capability type**

Create `src/lib/ompDesktopV1/capability.ts`:

```ts
export interface DesktopV1Capability {
  schemaVersion: number;
  schemaDigest: string;
  methods: string[];
  notifications: string[];
  optionalFeatures: string[];
}
```

- [ ] **Step 5: Implement the client**

Create `src/lib/ompDesktopV1/index.ts`:

```ts
import type { MethodMap } from "./methods";
import type { DesktopV1Capability } from "./capability";
import { type DesktopV1Error, RUNTIME_UNAVAILABLE } from "./errors";

export type { MethodMap } from "./methods";
export type * from "./methods";
export type { DesktopV1Capability } from "./capability";
export type { DesktopV1Error } from "./errors";
export { isDesktopV1Error, RUNTIME_UNAVAILABLE } from "./errors";

export type CallResult<T> = { ok: true; value: T } | { ok: false; error: DesktopV1Error };

const NAMESPACE = "_omp/desktop/v1/";

export class OmpDesktopV1Client {
  private capability: DesktopV1Capability | null = null;

  setCapability(cap: DesktopV1Capability | null): void {
    this.capability = cap;
  }

  get hasCapability(): boolean {
    return this.capability !== null;
  }

  async call<K extends keyof MethodMap>(
    method: K,
    params: MethodMap[K]["params"],
  ): Promise<CallResult<MethodMap[K]["result"]>> {
    if (!this.capability) {
      return { ok: false, error: RUNTIME_UNAVAILABLE };
    }
    const fullMethod = `${NAMESPACE}${method}`;
    if (!this.capability.methods.includes(fullMethod)) {
      return { ok: false, error: { ...RUNTIME_UNAVAILABLE, code: "unknown_method", messageKey: "compat.unknownMethod" } };
    }
    // In Plan 2, we don't have a real transport. Plan 3 will wire this to the ACP client.
    return { ok: false, error: RUNTIME_UNAVAILABLE };
  }
}
```

- [ ] **Step 6: Run tests and commit**

```bash
pnpm test -- src/lib/ompDesktopV1/contract.test.ts
git add src/lib/ompDesktopV1 src/lib/api.ts
git commit -m "feat: add frontend OmpDesktopV1 typed client"
```

---

### Task 9: Route Desktop Commands Through OmpExtension When Capability Present

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src/lib/api.ts`

**Interfaces:**
- Consumes: `OmpExtension` from Task 6; existing command stubs from Plan 1.
- Produces: Commands that prefer v1 protocol when capability is negotiated, falling back to `runtime_unavailable` when absent.

- [ ] **Step 1: Add Tauri command for capability negotiation**

In `src-tauri/src/commands.rs`, add:

```rust
#[tauri::command]
pub async fn omp_desktop_v1_capability(state: tauri::State<'_, AppState>) -> Result<Option<DesktopV1Capability>, String> {
    Ok(state.extension.capability().await)
}
```

Register it in `lib.rs` `generate_handler!`.

- [ ] **Step 2: Update skills_list, inspect_mcp, project_inspect, agents_list, plugins_list**

For each of these commands, replace the direct `run_grok_inspect` / `run_grok_cli_args` stub with:

```rust
#[tauri::command]
pub async fn skills_list(/* existing params */ state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    if state.extension.has_capability().await {
        let result = state.extension.request("extensions.list", serde_json::json!({})).await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => Err(e.to_string()),
        }
    } else {
        Err("runtime_unavailable: OMP Runtime is not connected".to_string())
    }
}
```

Apply the same pattern to `inspect_mcp` (→ `mcp.list`), `project_inspect` (→ `diagnostics.selfCheck`), `agents_list` (→ `extensions.list`), `plugins_list` (→ `extensions.list`).

- [ ] **Step 3: Add frontend wrappers**

In `src/lib/api.ts`, add typed wrappers:

```ts
export async function ompDesktopV1Capability(): Promise<DesktopV1Capability | null> {
  return invoke("omp_desktop_v1_capability");
}

export async function extensionsListV1(cwd?: string): Promise<ExtensionInfo[]> {
  const client = getOmpDesktopV1Client();
  const result = await client.call("extensions.list", { cwd });
  if (!result.ok) throw new Error(result.error.messageKey);
  return result.value.extensions;
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --locked
pnpm test
pnpm typecheck
```

Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs src/lib/api.ts
git commit -m "feat: route Desktop commands through OmpExtension"
```

---

### Task 10: Record Provenance Patch and Advance Submodule Pointer

**Files:**
- Modify: `provenance/omp-patches.json`
- Modify: `.gitmodules` (if submodule URL changes)
- Modify: `runtime/oh-my-pi` (gitlink update)

**Interfaces:**
- Consumes: all OMP submodule changes from Tasks 2–5.
- Produces: `omp-patches.json` records the patch; superproject submodule pointer advances to the patched commit.

- [ ] **Step 1: Commit the OMP submodule changes on a Fork branch**

```bash
cd runtime/oh-my-pi
git checkout -b desktop-v1-protocol
git add packages/coding-agent/src/modes/acp/desktop-v1 packages/coding-agent/test/desktop-v1 packages/coding-agent/scripts/gen-desktop-v1-schema.ts packages/coding-agent/src/modes/acp/acp-agent.ts
git commit -m "feat: implement OMP Desktop v1 Extension Protocol

Adds versioned _omp/desktop/v1/* namespace with:
- JSON Schema bundle for all 24 methods and 4 notifications
- Dispatcher with schema validation
- Handlers for sessions, projects, usage, extensions, providers, credentials, mcp, sessionConfig, queue, steer, diagnostics
- Legacy _omp/* compat shim
- Capability descriptor in initialize

Plan 2 of OMP Desktop 1.0 roadmap."
git log --oneline -1
```

Record the commit SHA.

- [ ] **Step 2: Update the superproject submodule pointer**

```bash
cd /Users/po1nt9/Github/grok-app-main
git add runtime/oh-my-pi
```

- [ ] **Step 3: Update omp-patches.json**

Update `provenance/omp-patches.json`:

```json
{
  "schemaVersion": 1,
  "baseCommit": "667111575ebba136dadfd6989379e7f67e0d40d9",
  "patches": [
    {
      "id": "desktop-v1-protocol",
      "branch": "desktop-v1-protocol",
      "description": "OMP Desktop v1 Extension Protocol: versioned _omp/desktop/v1/* namespace, schema bundle, dispatcher, handlers, legacy compat",
      "plan": "2026-07-29-omp-desktop-extension-protocol",
      "commit": "<patched-commit-sha>"
    }
  ]
}
```

- [ ] **Step 4: Verify provenance still passes**

```bash
pnpm check:provenance
```

Expected: PASS (the checker should accept the new patch entry).

If the checker rejects the patch entry, update `scripts/check-provenance.mjs` to validate patch entries against the submodule log (the patch commit must be an ancestor of the submodule HEAD).

- [ ] **Step 5: Commit**

```bash
git add provenance/omp-patches.json runtime/oh-my-pi
git commit -m "chore: record OMP Desktop v1 protocol patch and advance submodule"
```

---

### Task 11: Run Full Verification and Write Plan 2 Verification Record

**Files:**
- Create: `docs/superpowers/verification/2026-07-29-plan-2-extension-protocol.md`
- Modify: only if a gate exposes a Plan 2 regression.

**Interfaces:**
- Consumes: all prior Plan 2 deliverables.
- Produces: a reproducible verification record and a clean working tree.

- [ ] **Step 1: Reinitialize dependencies**

```bash
pnpm install --frozen-lockfile
git submodule update --init --recursive
```

Expected: install succeeds; submodule HEAD is the patched commit.

- [ ] **Step 2: Run custom policy gates**

```bash
pnpm check:provenance
pnpm check:brand
pnpm check:legal
node --test scripts/check-provenance.test.mjs scripts/check-brand-policy.test.mjs scripts/check-legal-baseline.test.mjs
```

Expected: all PASS.

- [ ] **Step 3: Run OMP submodule tests**

```bash
cd runtime/oh-my-pi && OMP_DESKTOP_V1_PROTOCOL=1 bun test packages/coding-agent/test/desktop-v1/
cd runtime/oh-my-pi && bun test packages/coding-agent/test/acp-agent.test.ts
```

Expected: all v1 tests PASS; existing ACP tests PASS.

- [ ] **Step 4: Run frontend verification**

```bash
pnpm typecheck
pnpm test
pnpm build:ui
```

Expected: all PASS.

- [ ] **Step 5: Run Rust verification**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --locked
cargo metadata --manifest-path src-tauri/Cargo.toml --locked --format-version 1 >/dev/null
```

Expected: all tests PASS; metadata exits 0.

- [ ] **Step 6: Verify no private bindings remain**

```bash
! git grep -nE '_x\.ai/|x\.ai/rewind' -- src src-tauri ':!testdata/brand-policy/denied/**' ':!docs/upstream-history/**'
```

Expected: grep exits 0 (no matches).

- [ ] **Step 7: Write the verification record**

Create `docs/superpowers/verification/2026-07-29-plan-2-extension-protocol.md`:

```md
# Plan 2 OMP Desktop Extension Protocol Verification

- OMP submodule base: 667111575ebba136dadfd6989379e7f67e0d40d9
- OMP submodule patched: <patched-commit-sha>
- Patch branch: desktop-v1-protocol
- Namespace: _omp/desktop/v1/*
- Methods defined: 24
- Notifications defined: 4
- Error codes: 9
- Legacy compat: 6 methods mapped
- Schema digest: <digest>
- Brand policy: zero violations
- Provenance policy: passed (patch recorded)
- Legal/SBOM input policy: passed
- OMP submodule tests: passed (desktop-v1 + acp-agent)
- Frontend typecheck/tests/build: passed
- Rust tests/metadata: passed
- Dead x.ai/rewind bindings: removed
- Stale lib.rs doc: fixed
- Capability negotiation: implemented (gated on OMP_DESKTOP_V1_PROTOCOL=1)
- Queue/steer handlers: return runtime_unavailable (Plan 3 dependency)
```

- [ ] **Step 8: Commit**

```bash
git add docs/superpowers/verification/2026-07-29-plan-2-extension-protocol.md
git commit -m "test: verify OMP Desktop v1 Extension Protocol"
git status --short --branch
```

Expected: commit succeeds; working tree clean.

---

## Plan 2 Completion Boundary

Plan 2 is complete only when all Task 11 gates pass. The resulting codebase has:

1. A versioned `_omp/desktop/v1/*` namespace with 24 methods and 4 notifications, each with a JSON Schema.
2. A capability descriptor negotiated during ACP `initialize`.
3. A dispatcher that validates params and results against schemas.
4. Legacy `_omp/*` compatibility through a shim that maps to v1 handlers.
5. A Rust `OmpExtension` client and frontend typed client.
6. Dead `x.ai/rewind/*` bindings removed.
7. An OMP Fork patch recorded in `provenance/omp-patches.json`.
8. All policy gates, frontend tests, Rust tests, and submodule tests passing.

Plan 2 does not connect a real runtime. The `OmpExtension` client returns `runtime_unavailable` until Plan 3 wires the Supervisor and real ACP transport. Plan 3 may then rely on the v1 protocol surface defined here without inventing new protocol methods.
