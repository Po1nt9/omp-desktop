/**
 * `DesktopV1Error` — the stable error envelope for `_omp/desktop/v1/*` requests.
 *
 * Mirrors `src-tauri/src/omp_desktop_v1/errors.rs` and the OMP submodule's
 * `packages/coding-agent/src/modes/acp/desktop-v1/errors.ts`. The code →
 * metadata table is the authoritative TS source for error codes defined in
 * the Plan 2 inventory.
 */

export interface DesktopV1Error {
  code: string;
  message: string;
  messageKey: string;
  args: Record<string, unknown>;
  recoverable: boolean;
  retryable: boolean;
  details?: unknown;
}

/**
 * Type guard for values that look like a {@link DesktopV1Error}.
 *
 * Used when unwrapping rejected `invoke` results or parsing error payloads
 * forwarded from the Rust `OmpExtension` client.
 */
export function isDesktopV1Error(value: unknown): value is DesktopV1Error {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "messageKey" in value
  );
}

/**
 * Pre-built `runtime_unavailable` sentinel — the Plan 2 fail-closed value
 * returned whenever no capability has been negotiated or the Plan 3 transport
 * is not yet wired.
 */
export const RUNTIME_UNAVAILABLE: DesktopV1Error = {
  code: "runtime_unavailable",
  message: "runtime.unavailable",
  messageKey: "runtime.unavailable",
  args: {},
  recoverable: false,
  retryable: false,
};

/**
 * Pre-built `unknown_method` sentinel — returned when a method name is not in
 * the negotiated capability's method list.
 */
export const UNKNOWN_METHOD: DesktopV1Error = {
  code: "unknown_method",
  message: "compat.unknownMethod",
  messageKey: "compat.unknownMethod",
  args: {},
  recoverable: false,
  retryable: false,
};
