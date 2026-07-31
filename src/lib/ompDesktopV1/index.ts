/**
 * `OmpDesktopV1Client` — typed frontend client for the versioned
 * `_omp/desktop/v1/*` Extension Protocol.
 *
 * Plan 2 scope: capability cache + method allow-list + fail-closed request
 * surface. The client always returns `runtime_unavailable` for the actual
 * request dispatch because the real ACP transport is wired in Plan 3.
 *
 * Plan 3 will inject the Tauri `invoke` transport (or a direct ACP client)
 * and replace the final `runtime_unavailable` return with a real JSON-RPC
 * call. The capability negotiation, method allow-list, and error mapping
 * implemented here will remain unchanged.
 */

import type { MethodMap, MethodName } from "./methods";
import type { DesktopV1Capability } from "./capability";
import { type DesktopV1Error, isDesktopV1Error, RUNTIME_UNAVAILABLE, UNKNOWN_METHOD } from "./errors";

export type { MethodMap, MethodName } from "./methods";
export type * from "./methods";
export type { DesktopV1Capability } from "./capability";
export type { DesktopV1Error } from "./errors";
export { isDesktopV1Error, RUNTIME_UNAVAILABLE, UNKNOWN_METHOD } from "./errors";

/**
 * Result of a v1 method call — a discriminated union of success and error.
 *
 * Callers should narrow with `result.ok` before accessing `result.value` or
 * `result.error`:
 *
 * ```ts
 * const result = await client.call("sessions.listAll", { limit: 10 });
 * if (!result.ok) {
 *   console.error(result.error.code);
 *   return;
 * }
 * console.log(result.value.sessions);
 * ```
 */
export type CallResult<T> = { ok: true; value: T } | { ok: false; error: DesktopV1Error };

/**
 * Injected transport for `_omp/desktop/v1/*` requests (Plan 3 seam, AC-1.5).
 * Receives the fully-namespaced method and the raw params; resolves with the
 * JSON-RPC result payload. Rejections map to `runtime_unavailable`.
 */
export type DesktopV1Transport = (
  method: string,
  params: unknown,
) => Promise<unknown>;

/** Wire namespace prefix for every v1 method. */
const NAMESPACE = "_omp/desktop/v1/";

/**
 * Desktop-side typed client for the OMP Desktop v1 Extension Protocol.
 *
 * Holds the negotiated capability (if any) and enforces the fail-closed
 * contract from Plan 1: when no capability has been negotiated, every
 * request returns {@link RUNTIME_UNAVAILABLE} without touching the wire.
 */
export class OmpDesktopV1Client {
  private capability: DesktopV1Capability | null = null;
  private transport: DesktopV1Transport | null = null;

  /**
   * Inject (or clear with `null`) the request transport. Until a transport
   * is set the client stays fail-closed even with a negotiated capability.
   */
  setTransport(t: DesktopV1Transport | null): void {
    this.transport = t;
  }

  /**
   * Store (or clear with `null`) the capability descriptor advertised by
   * the OMP Runtime during ACP `initialize`.
   *
   * Typically called once at app startup after probing the Rust
   * `omp_desktop_v1_capability` Tauri command.
   */
  setCapability(cap: DesktopV1Capability | null): void {
    this.capability = cap;
  }

  /** Returns `true` when a capability descriptor has been negotiated. */
  get hasCapability(): boolean {
    return this.capability !== null;
  }

  /**
   * Dispatch a `_omp/desktop/v1/<method>` request.
   *
   * Plan 2 behavior:
   * 1. No capability → `runtime_unavailable`.
   * 2. Method not in the capability's method list → `unknown_method`.
   * 3. Otherwise → `runtime_unavailable` (real transport lands in Plan 3).
   *
   * The method name `K` is a literal string from {@link MethodName}, so the
   * `params` argument and the success `value` are fully typed at the call
   * site without runtime casts.
   */
  async call<K extends MethodName>(
    method: K,
    params: MethodMap[K]["params"],
  ): Promise<CallResult<MethodMap[K]["result"]>> {
    if (!this.capability) {
      return { ok: false, error: RUNTIME_UNAVAILABLE };
    }
    const fullMethod = `${NAMESPACE}${method}`;
    if (!this.capability.methods.includes(fullMethod)) {
      return { ok: false, error: UNKNOWN_METHOD };
    }
    if (!this.transport) {
      // Plan 2 fail-closed: no transport injected yet.
      return { ok: false, error: RUNTIME_UNAVAILABLE };
    }
    try {
      const value = (await this.transport(fullMethod, params)) as MethodMap[K]["result"];
      return { ok: true, value };
    } catch (e) {
      if (isDesktopV1Error(e)) {
        return { ok: false, error: e };
      }
      return { ok: false, error: RUNTIME_UNAVAILABLE };
    }
  }
}
