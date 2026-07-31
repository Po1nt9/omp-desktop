//! `OmpExtension` — Desktop-side client for the versioned `_omp/desktop/v1/*`
//! Extension Protocol.
//!
//! Capability cache + method allow-list + fail-closed request surface, with a
//! pluggable [`V1Transport`] injected once a session negotiates the
//! `_omp/desktop/v1` capability. Without a transport the surface stays
//! fail-closed (`runtime_unavailable`) even for advertised methods.

pub mod capability;
pub mod errors;
pub mod generated;
pub mod ids;
pub mod transport;

#[cfg(test)]
mod tests;

use errors::DesktopV1Error;
use generated::DesktopV1Capability;
use std::sync::Arc;
use tokio::sync::RwLock;
use transport::V1Transport;

const NAMESPACE: &str = "_omp/desktop/v1/";

/// Desktop-side client for the OMP Desktop v1 Extension Protocol.
///
/// Holds the negotiated capability (if any) and enforces the fail-closed
/// contract from Plan 1: when no capability has been negotiated, every
/// request returns [`DesktopV1Error::runtime_unavailable`] without touching
/// the wire. Once a session negotiates the capability, the session manager
/// injects the session's `AcpClient` as the [`V1Transport`] so advertised
/// methods dispatch as real JSON-RPC calls.
pub struct OmpExtension {
    capability: Arc<RwLock<Option<DesktopV1Capability>>>,
    transport: Arc<RwLock<Option<Arc<dyn V1Transport>>>>,
}

impl OmpExtension {
    /// Construct a fail-closed client with no capability and no transport.
    pub fn new() -> Self {
        Self {
            capability: Arc::new(RwLock::new(None)),
            transport: Arc::new(RwLock::new(None)),
        }
    }

    /// Store (or clear with `None`) the capability descriptor advertised by
    /// the OMP Runtime during ACP `initialize`.
    pub async fn negotiate_capability(&self, cap: Option<DesktopV1Capability>) {
        *self.capability.write().await = cap;
    }

    /// Install (or clear with `None`) the transport used to dispatch
    /// advertised v1 methods. Called by the session manager alongside
    /// capability negotiation; clearing keeps the client fail-closed.
    pub async fn set_transport(&self, transport: Option<Arc<dyn V1Transport>>) {
        *self.transport.write().await = transport;
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
    /// Fail-closed contract:
    /// 1. No capability → `runtime_unavailable`.
    /// 2. Method not in the capability's method list → `unknown_method`.
    /// 3. No transport installed → `runtime_unavailable`.
    /// 4. Otherwise forward to the transport; a transport failure maps to
    ///    `runtime_unavailable` with the underlying detail in `args.detail`.
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
        let transport = self.transport.read().await.clone();
        let Some(transport) = transport else {
            return Err(DesktopV1Error::runtime_unavailable());
        };
        transport
            .dispatch_v1(&full_method, params)
            .await
            .map_err(|e| {
                DesktopV1Error::new(
                    "runtime_unavailable",
                    serde_json::json!({ "detail": e }),
                )
            })
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
