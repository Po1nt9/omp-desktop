/**
 * Capability descriptor advertised by the OMP Runtime during ACP `initialize`.
 *
 * When `null`, the `OmpDesktopV1Client` is fail-closed and every request
 * returns `runtime_unavailable`. Mirrors
 * `src-tauri/src/omp_desktop_v1/generated.rs::DesktopV1Capability`.
 */

export interface DesktopV1Capability {
  schemaVersion: number;
  schemaDigest: string;
  methods: string[];
  notifications: string[];
  optionalFeatures: string[];
}
