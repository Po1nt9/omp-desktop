//! `V1Transport` — abstraction over the wire used to dispatch
//! `_omp/desktop/v1/*` JSON-RPC calls to the OMP Runtime.
//!
//! Abstracted as a trait (object-safe via `async-trait`) so `OmpExtension`
//! can be unit-tested with a mock transport and stays decoupled from
//! `AcpClient` internals. In production the implementor is `AcpClient`
//! (see `acp_client.rs`); in tests it is a mock.

use async_trait::async_trait;
use serde_json::Value;

/// Dispatches a fully-qualified `_omp/desktop/v1/<method>` JSON-RPC call.
#[async_trait]
pub trait V1Transport: Send + Sync {
    /// Send the request and return the raw JSON-RPC result value.
    ///
    /// `full_method` already carries the `_omp/desktop/v1/` prefix. On any
    /// transport-layer failure returns `Err` with a short diagnostic string;
    /// `OmpExtension::request` maps that to a `runtime_unavailable` error.
    async fn dispatch_v1(&self, full_method: &str, params: Value) -> Result<Value, String>;
}
