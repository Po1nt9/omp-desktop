/**
 * Neutral model / effort option types for the composer.
 *
 * The static catalog is intentionally empty until a runtime integration
 * supplies live models. Consumers must handle an empty `availableModels`.
 * Update docs/llm-wiki/catalog.md when defaults change.
 */

export interface EffortOption {
  /** Effort id passed to `--reasoning-effort` (e.g. low / medium / high). */
  id: string;
  /** CLI value when distinct from id; usually equals id. */
  value?: string;
  /** Display label from catalog when present. */
  label?: string;
  description?: string;
  isDefault?: boolean;
}

export interface ModelOption {
  id: string;
  /** Display name (language-neutral product name) */
  label: string;
  /** True if CLI lists as default */
  isDefault?: boolean;
  /** Catalog source; composer only shows official model IDs (not providers). */
  source?: string;
  /** Per-model reasoning efforts from CLI cache; empty/undefined → no fallback. */
  reasoningEfforts?: EffortOption[];
}

export interface SessionModeOption {
  id: "agent" | "plan" | "ask";
}

/**
 * Permission policies (composer + settings).
 * | Build mode           | App id            |
 * | default              | ask               |
 * | acceptEdits          | accept_edits      |
 * | (session grant UX)   | allow_for_session |
 * | dontAsk              | dont_ask          |
 * | bypassPermissions    | always_approve    |
 */
export type PermissionPolicyId =
  | "ask"
  | "accept_edits"
  | "allow_for_session"
  | "dont_ask"
  | "always_approve";

/** Where composer model / permission choices are remembered. */
export type ComposerPrefsScope = "global" | "project" | "session";

export const COMPOSER_PREFS_SCOPES: ComposerPrefsScope[] = [
  "global",
  "project",
  "session",
];

/**
 * Empty until a runtime integration supplies live models.
 * Do NOT invent a fallback model here.
 */
export const availableModels: readonly ModelOption[] = [];

/** No default model until the runtime catalog is populated. */
export const defaultModelId: string | null = null;

/** Default reasoning depth when a model lists no explicit efforts. */
export const DEFAULT_EFFORT = "medium";

/** Product session modes (desktop shell). */
export const SESSION_MODES: SessionModeOption[] = [
  { id: "agent" },
  { id: "plan" },
  { id: "ask" },
];

/**
 * Permission policies (composer + settings).
 * `always_approve` = YOLO / unrestricted (CLI `--always-approve`, config yolo).
 */
export const PERMISSION_POLICIES: {
  id: PermissionPolicyId;
  dangerous?: boolean;
}[] = [
  { id: "ask" },
  { id: "accept_edits" },
  { id: "allow_for_session" },
  { id: "dont_ask" },
  { id: "always_approve", dangerous: true },
];

export function isValidModelId(
  id: string,
  catalog: readonly ModelOption[] = availableModels,
): boolean {
  return catalog.some((m) => m.id === id);
}

/**
 * Efforts list for a model: live catalog when non-empty, else empty.
 */
export function effortsForModel(
  model?: ModelOption | null,
  catalogEfforts?: EffortOption[] | null,
): EffortOption[] {
  const fromArg =
    catalogEfforts && catalogEfforts.length > 0 ? catalogEfforts : null;
  const fromModel =
    model?.reasoningEfforts && model.reasoningEfforts.length > 0
      ? model.reasoningEfforts
      : null;
  return fromArg ?? fromModel ?? [];
}

/**
 * Validate an effort id against the selected model's efforts when known;
 * otherwise against an empty set (no static fallback).
 */
export function isValidEffort(
  id: string,
  modelOrEfforts?: ModelOption | EffortOption[] | null,
): boolean {
  if (!id) return false;
  if (Array.isArray(modelOrEfforts)) {
    return effortsForModel(null, modelOrEfforts).some((e) => e.id === id);
  }
  return effortsForModel(modelOrEfforts).some((e) => e.id === id);
}

/** Default effort for a model (catalog default flag, else first, else medium). */
export function pickDefaultEffort(
  model?: ModelOption | null,
  catalogEfforts?: EffortOption[] | null,
): string {
  const list = effortsForModel(model, catalogEfforts);
  return (
    list.find((e) => e.isDefault)?.id ?? list[0]?.id ?? DEFAULT_EFFORT
  );
}

/**
 * Strip a shared CLI suffix so "High Effort" / "Medium Effort" collapse to
 * "High" / "Medium" (identical trailing " Effort" is noise in compact UI).
 */
export function stripCommonEffortSuffix(label: string): string {
  const trimmed = label.trim();
  if (!trimmed) return trimmed;
  const stripped = trimmed.replace(/\s+Effort$/i, "").trim();
  return stripped || trimmed;
}

/**
 * Display label for an effort.
 * - Standard ids (`high` / `medium` / `low`): prefer i18n so locale controls
 *   高/中/低 vs High/Medium/Low (catalog labels are English-only).
 * - Other catalog labels: strip a shared " Effort" suffix, then raw id.
 */
export function effortDisplayLabel(
  effort: EffortOption | string,
  i18nLabels?: {
    high?: string;
    medium?: string;
    low?: string;
  },
): string {
  const id = typeof effort === "string" ? effort : effort.id;
  if (id === "high") return i18nLabels?.high ?? "High";
  if (id === "medium") return i18nLabels?.medium ?? "Medium";
  if (id === "low") return i18nLabels?.low ?? "Low";

  if (typeof effort !== "string") {
    const raw = effort.label?.trim();
    if (raw) return stripCommonEffortSuffix(raw);
    return effortDisplayLabel(effort.id, i18nLabels);
  }
  return effort;
}

export function isValidPolicy(id: string): id is PermissionPolicyId {
  return PERMISSION_POLICIES.some((p) => p.id === id);
}

export function isValidPrefsScope(id: string): id is ComposerPrefsScope {
  return COMPOSER_PREFS_SCOPES.includes(id as ComposerPrefsScope);
}

/** Pick the default model id from a catalog (null when empty). */
export function pickDefaultModelId(catalog: readonly ModelOption[]): string | null {
  return (
    catalog.find((m) => m.isDefault)?.id ??
    catalog[0]?.id ??
    defaultModelId
  );
}

/** Find a model in catalog by id. */
export function findModel(
  id: string,
  catalog: readonly ModelOption[] = availableModels,
): ModelOption | undefined {
  return catalog.find((m) => m.id === id);
}
