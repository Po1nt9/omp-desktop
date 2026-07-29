/**
 * Typed method signatures for every `_omp/desktop/v1/*` method.
 *
 * The shapes mirror the JSON Schemas in
 * `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/schema/methods.ts`
 * and the Rust structs in
 * `src-tauri/src/omp_desktop_v1/generated.rs`. The `MethodMap` interface maps
 * each short method name (e.g. `"sessions.listAll"`) to its `{ params, result }`
 * pair so the {@link OmpDesktopV1Client} can provide end-to-end typing.
 */

// ── Shared result shapes ───────────────────────────────────────────────────

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

/** Credential metadata only — never includes the secret. */
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

// ── Per-method params/result types (32 methods) ────────────────────────────

export interface SessionsListAllParams {
  limit?: number;
}
export interface SessionsListAllResult {
  sessions: SessionInfo[];
  total: number;
  cursor: string | null;
}

export interface SessionsByCwdParams {
  cwd: string;
  limit?: number;
}
export interface SessionsByCwdResult {
  sessions: SessionInfo[];
  cursor: string | null;
}

export interface ProjectsListParams {}
export interface ProjectsListResult {
  projects: ProjectInfo[];
  totalSessions: number;
}

export interface UsageReportsParams {}
export interface UsageReportsResult {
  reports: UsageReport[];
}

export interface ExtensionsListParams {
  cwd?: string;
}
export interface ExtensionsListResult {
  extensions: ExtensionInfo[];
}

export interface ExtensionsToggleParams {
  providerId: string;
  enabled?: boolean;
}
export interface ExtensionsToggleResult {
  enabled: boolean;
}

export interface ProvidersListParams {}
export interface ProvidersListResult {
  providers: ProviderInfo[];
}

export interface ProvidersModelsParams {
  providerId?: string;
}
export interface ProvidersModelsResult {
  models: ModelInfo[];
}

export interface CredentialsListParams {
  providerId?: string;
}
export interface CredentialsListResult {
  credentials: CredentialMetadata[];
}

export interface CredentialsBeginAuthParams {
  providerId: string;
  method: string;
}
export interface CredentialsBeginAuthResult {
  authId: string;
  status: "pending";
}

export interface CredentialsCompleteAuthParams {
  authId: string;
  code: string;
}
export interface CredentialsCompleteAuthResult {
  status: "active";
}

export interface CredentialsCancelAuthParams {
  authId: string;
}
export interface CredentialsCancelAuthResult {
  status: "cancelled";
}

export interface CredentialsReplaceParams {
  credentialId: string;
}
export interface CredentialsReplaceResult {
  status: "active";
}

export interface CredentialsRevokeParams {
  credentialId: string;
}
export interface CredentialsRevokeResult {
  status: "revoked";
}

export interface CredentialsHealthParams {
  credentialId?: string;
}
export interface CredentialsHealthResult {
  healthy: string[];
  unhealthy: string[];
}

export interface CredentialsMigrationStatusParams {}
export interface CredentialsMigrationStatusResult {
  migrated: number;
  pending: number;
  failed: number;
  details: unknown[];
}

export interface McpListParams {
  cwd?: string;
}
export interface McpListResult {
  sources: McpSourceInfo[];
}

export interface McpDiscoverParams {
  cwd: string;
}
export interface McpDiscoverResult {
  sources: McpSourceInfo[];
}

export interface SessionConfigGetParams {
  sessionId?: string;
}
export interface SessionConfigGetResult {
  config: unknown;
}

export interface SessionConfigSetParams {
  sessionId?: string;
  config: unknown;
}
export interface SessionConfigSetResult {
  config: unknown;
}

export interface QueueEnqueueParams {
  sessionId: string;
  prompt: string;
}
export interface QueueEnqueueResult {
  receiptId: string;
  status: "queued";
}

export interface QueueCancelParams {
  receiptId: string;
}
export interface QueueCancelResult {
  status: "cancelled";
}

export interface SteerSendParams {
  turnId: string;
  message: string;
}
export interface SteerSendResult {
  status: "accepted";
}

export interface DiagnosticsSelfCheckParams {}
export interface DiagnosticsSelfCheckResult {
  checks: unknown[];
}

// ── skills.list ────────────────────────────────────────────────────────────
export interface SkillsListParams {
  cwd?: string;
}
export interface SkillInfo {
  id: string;
  name: string;
  description: string | null;
  level: "user" | "project";
  hidden: boolean;
}
export interface SkillsListResult {
  skills: SkillInfo[];
}

// ── todo.list ────────────────────────────────────────────────────────────────
export interface TodoListParams {
  sessionId?: string;
}
export interface TodoTask {
  content: string;
  status: "pending" | "in_progress" | "completed" | "abandoned" | "blocked";
}
export interface TodoPhase {
  name: string;
  tasks: TodoTask[];
}
export interface TodoListResult {
  phases: TodoPhase[];
}

// ── subagents.status ─────────────────────────────────────────────────────────
export interface SubagentsStatusParams {}
export interface SubagentsStatusResult {
  enabled: boolean;
  activeCount: number;
}

// ── subagents.setEnabled ──────────────────────────────────────────────────────
export interface SubagentsSetEnabledParams {
  enabled: boolean;
}
export interface SubagentsSetEnabledResult {
  enabled: boolean;
}

// ── sessions.fork ─────────────────────────────────────────────────────────────
export interface SessionsForkParams {
  sourceId: string;
  throughUserPromptIndex?: number;
  title?: string;
}
export interface SessionsForkResult {
  id: string;
  title: string | null;
  parentSession: string;
}

// ── sessions.rewindPoints ─────────────────────────────────────────────────────
export interface SessionsRewindPointsParams {
  sessionId?: string;
}
export interface RewindPoint {
  promptIndex: number;
  messageId: string | null;
  preview: string;
}
export interface SessionsRewindPointsResult {
  points: RewindPoint[];
}

// ── sessions.rewind ───────────────────────────────────────────────────────────
export interface SessionsRewindParams {
  targetPromptIndex: number;
  sessionId?: string;
}
export interface SessionsRewindResult {
  keptCount: number;
  localOk: boolean;
}

// ── sessions.resolveMedia ─────────────────────────────────────────────────────
export interface SessionsResolveMediaParams {
  sessionId: string;
  relatives: string[];
}
export interface MediaAttachment {
  path: string;
  name: string;
  isDir: boolean;
}
export interface SessionsResolveMediaResult {
  attachments: MediaAttachment[];
}

// ── diagnostics.exportBundle ─────────────────────────────────────────────────
export interface DiagnosticsExportBundleParams {
  sessionId: string;
}
export interface DiagnosticsExportBundleResult {
  path: string;
}

// ── config.discover ─────────────────────────────────────────────────────────
export interface ConfigDiscoverParams {
  cwd?: string;
}
export interface ConfigSourceInfo {
  kind:
    | "settings"
    | "mcp"
    | "models"
    | "credentials"
    | "skills"
    | "sessions"
    | "project";
  path: string;
  level: "user" | "project";
  writable: boolean;
}
export interface ConfigDiscoverResult {
  agentDir: string;
  profile: string | null;
  projectCwd: string | null;
  sources: ConfigSourceInfo[];
}

// ── Method map ─────────────────────────────────────────────────────────────

/**
 * Maps each short v1 method name to its `{ params, result }` pair.
 *
 * Used by {@link OmpDesktopV1Client.call} to type-check both the request
 * params and the success result at the call site.
 */
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
  "credentials.beginAuth": {
    params: CredentialsBeginAuthParams;
    result: CredentialsBeginAuthResult;
  };
  "credentials.completeAuth": {
    params: CredentialsCompleteAuthParams;
    result: CredentialsCompleteAuthResult;
  };
  "credentials.cancelAuth": {
    params: CredentialsCancelAuthParams;
    result: CredentialsCancelAuthResult;
  };
  "credentials.replace": {
    params: CredentialsReplaceParams;
    result: CredentialsReplaceResult;
  };
  "credentials.revoke": {
    params: CredentialsRevokeParams;
    result: CredentialsRevokeResult;
  };
  "credentials.health": {
    params: CredentialsHealthParams;
    result: CredentialsHealthResult;
  };
  "credentials.migrationStatus": {
    params: CredentialsMigrationStatusParams;
    result: CredentialsMigrationStatusResult;
  };
  "mcp.list": { params: McpListParams; result: McpListResult };
  "mcp.discover": { params: McpDiscoverParams; result: McpDiscoverResult };
  "sessionConfig.get": {
    params: SessionConfigGetParams;
    result: SessionConfigGetResult;
  };
  "sessionConfig.set": {
    params: SessionConfigSetParams;
    result: SessionConfigSetResult;
  };
  "queue.enqueue": { params: QueueEnqueueParams; result: QueueEnqueueResult };
  "queue.cancel": { params: QueueCancelParams; result: QueueCancelResult };
  "steer.send": { params: SteerSendParams; result: SteerSendResult };
  "diagnostics.selfCheck": {
    params: DiagnosticsSelfCheckParams;
    result: DiagnosticsSelfCheckResult;
  };
  "skills.list": { params: SkillsListParams; result: SkillsListResult };
  "todo.list": { params: TodoListParams; result: TodoListResult };
  "subagents.status": {
    params: SubagentsStatusParams;
    result: SubagentsStatusResult;
  };
  "subagents.setEnabled": {
    params: SubagentsSetEnabledParams;
    result: SubagentsSetEnabledResult;
  };
  "sessions.fork": { params: SessionsForkParams; result: SessionsForkResult };
  "sessions.rewindPoints": {
    params: SessionsRewindPointsParams;
    result: SessionsRewindPointsResult;
  };
  "sessions.rewind": {
    params: SessionsRewindParams;
    result: SessionsRewindResult;
  };
  "sessions.resolveMedia": {
    params: SessionsResolveMediaParams;
    result: SessionsResolveMediaResult;
  };
  "diagnostics.exportBundle": {
    params: DiagnosticsExportBundleParams;
    result: DiagnosticsExportBundleResult;
  };
  "config.discover": {
    params: ConfigDiscoverParams;
    result: ConfigDiscoverResult;
  };
}

/** Short method name (without the `_omp/desktop/v1/` prefix). */
export type MethodName = keyof MethodMap;
