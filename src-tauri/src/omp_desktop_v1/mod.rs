//! `OmpExtension` — Desktop-side client for the versioned `_omp/desktop/v1/*`
//! Extension Protocol.
//!
//! Plan 2 scope: capability cache + method allow-list + fail-closed request
//! surface. The struct always returns `runtime_unavailable` for the actual
//! request dispatch because the real ACP transport is wired in Plan 3.
//!
//! Plan 3 will inject an `AcpClient` reference and replace the final
//! `runtime_unavailable` return with a real JSON-RPC call. The capability
//! negotiation, method allow-list, and error mapping implemented here will
//! remain unchanged.

pub mod capability;
pub mod errors;
pub mod generated;
pub mod ids;

#[cfg(test)]
mod tests;

use errors::DesktopV1Error;
use generated::DesktopV1Capability;
use std::sync::Arc;
use tokio::sync::RwLock;

const NAMESPACE: &str = "_omp/desktop/v1/";

/// Desktop-side client for the OMP Desktop v1 Extension Protocol.
///
/// Holds the negotiated capability (if any) and enforces the fail-closed
/// contract from Plan 1: when no capability has been negotiated, every
/// request returns [`DesktopV1Error::runtime_unavailable`] without touching
/// the wire.
pub struct OmpExtension {
    capability: Arc<RwLock<Option<DesktopV1Capability>>>,
    // When Plan 3 wires the real transport, this will hold a reference to the
    // `AcpClient` so `request` can issue the actual JSON-RPC call. For Plan 2
    // the field is intentionally absent and every advertised method also
    // returns `runtime_unavailable`.
}

impl OmpExtension {
    /// Construct a fail-closed client with no capability negotiated.
    pub fn new() -> Self {
        Self {
            capability: Arc::new(RwLock::new(None)),
        }
    }

    /// Store (or clear with `None`) the capability descriptor advertised by
    /// the OMP Runtime during ACP `initialize`.
    pub async fn negotiate_capability(&self, cap: Option<DesktopV1Capability>) {
        *self.capability.write().await = cap;
    }

    /// Returns `true` when a capability descriptor has been negotiated.
    pub async fn has_capability(&self) -> bool {
        self.capability.read().await.is_some()
    }

    /// Returns a clone of the negotiated capability descriptor, if any.
    pub async fn capability(&self) -> Option<DesktopV1Capability> {
        self.capability.read().await.clone()
    }

    /// Dispatch a `_omp/desktop/v1/<method>` request.
    ///
    /// Plan 2 behavior:
    /// 1. No capability → `runtime_unavailable`.
    /// 2. Method not in the capability's method list → `unknown_method`.
    /// 3. Otherwise → `runtime_unavailable` (real transport lands in Plan 3).
    pub async fn request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, DesktopV1Error> {
        let cap_guard = self.capability.read().await;
        let cap = match cap_guard.as_ref() {
            None => return Err(DesktopV1Error::runtime_unavailable()),
            Some(c) => c,
        };
        let full_method = format!("{NAMESPACE}{method}");
        if !cap.methods.contains(&full_method) {
            return Err(DesktopV1Error::new(
                "unknown_method",
                serde_json::json!({ "method": full_method }),
            ));
        }
        // Plan 2 fail-closed: the real transport is not wired yet.
        // Plan 3 will inject the AcpClient and dispatch the request here.
        let _ = params; // params will be forwarded to the transport in Plan 3.
        Err(DesktopV1Error::runtime_unavailable())
    }
}

impl Default for OmpExtension {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract the `_omp/desktop/v1` capability descriptor from an ACP
/// `initialize` result, if the runtime advertised it.
///
/// The ACP `initialize` response may carry an `extensions` array whose
/// entries each describe a negotiated extension namespace. This helper
/// scans for the entry whose `namespace` is `_omp/desktop/v1` and
/// deserializes its fields into [`DesktopV1Capability`]. Returns `None`
/// when the runtime did not advertise the v1 extension or the entry
/// failed to deserialize.
pub fn extract_capability_from_initialize(
    initialize_result: &serde_json::Value,
) -> Option<DesktopV1Capability> {
    let extensions = initialize_result.get("extensions")?.as_array()?;
    for ext in extensions {
        let namespace = ext.get("namespace")?.as_str()?;
        if namespace == "_omp/desktop/v1" {
            if let Ok(cap) = serde_json::from_value::<DesktopV1Capability>(ext.clone()) {
                return Some(cap);
            }
        }
    }
    None
}
