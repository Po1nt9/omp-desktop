/** Generic identity display helpers with no account/billing coupling. */

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
