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
| Session | `sess_<26char base32>` | `sess_abcdefghijklmnopqrstuvwx23` |
| Turn | `turn_<26char base32>` | `turn_xyzastruvwxypqrsdefghijklm` |
| Event | `evt_<26char base32>` | `evt_qrstuvwxyzabcdefgijklmno23` |
| Permission request | `perm_<26char base32>` | `perm_mnopqrstuvwxyzabcdefgijk23` |
| Queue receipt | `rcpt_<26char base32>` | `rcpt_bcdefghijklmnopqrstuvwxy23` |
| Credential | `cred_<26char base32>` | `cred_pqrstuvwxyzabcdefghijklmno` |
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
