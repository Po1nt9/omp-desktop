//! Re-export of the capability descriptor type.
//!
//! `DesktopV1Capability` is generated from the schema bundle in
//! [`super::generated`]. The re-export keeps `capability.rs` as a stable path
//! for callers that import `omp_desktop_v1::capability::DesktopV1Capability`.

pub use super::generated::DesktopV1Capability;
