/** Generic identity display helpers with no account/billing coupling. */

/**
 * Neutral account/profile shapes — the full account/quota/billing surface
 * was removed in Task 6. These inert placeholders keep existing UI
 * conditionals compiling; the state is always null (no login path remains).
 */
export interface NeutralAccountProfile {
  signedIn?: boolean;
  email?: string | null;
  displayName?: string | null;
  expired?: boolean;
}

/** Inert billing shape — fields kept so legacy UI conditionals compile. */
export interface NeutralBilling {
  resetsAt?: string;
  remainingPercent?: number;
  available?: number;
  products?: unknown[];
  message?: string;
}

/** Inert call-log row — keeps AccountPanel table compiling (state is always empty). */
export interface NeutralCallLog {
  id: string;
  startedAt: string;
  projectPath?: string;
  title?: string;
  model?: string;
  turns?: number;
  contextTokens?: number;
  durationSecs?: number;
}

export interface NeutralAccountStatus {
  profile?: NeutralAccountProfile;
  billing?: NeutralBilling;
  channel?: string;
  heatmap?: never[];
  callLogs?: NeutralCallLog[];
}

export interface NeutralSavedAccount {
  id: string;
  label: string;
  email?: string | null;
  displayName?: string | null;
}

/**
 * Derive up to two initials from a free-form label.
 *
 * Replaces the old `accountInitials(profile)` helper that depended on
 * `AccountProfile`. Accepts a plain string so callers can pass a display
 * name, email, or any other label without pulling in account DTOs.
 */
export function identityInitials(label: string): string {
  return label
    .trim()
    .split(/\s+/u)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0] ?? "")
    .join("")
    .toUpperCase();
}

/**
 * Neutral display name for a profile — falls back to email, then localLabel.
 * Replaces the old `accountDisplayName(profile, localLabel)` helper.
 */
export function profileDisplayName(
  profile: NeutralAccountProfile | null | undefined,
  localLabel: string,
): string {
  if (!profile) return localLabel;
  return (
    profile.displayName?.trim() ||
    profile.email?.trim() ||
    localLabel
  );
}

/**
 * Neutral initials for a profile — falls back to "G" when no identity data.
 * Replaces the old `accountInitials(profile)` helper.
 */
export function profileInitials(
  profile: NeutralAccountProfile | null | undefined,
): string {
  const label = profileDisplayName(profile, "");
  return label ? identityInitials(label) : "G";
}

