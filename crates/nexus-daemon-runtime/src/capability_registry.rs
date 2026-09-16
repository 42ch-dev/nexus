//! Capability registry — daemon-side re-export of the core spine.
//!
//! v1.190 P3-T3: the registry BODY (the 30 builtin rows, the dispatch spine,
//! the admission pipeline and the peer/user-capability arms) moved to
//! [`nexus_core::execution::capabilities`] and
//! [`nexus_core::execution::peer_tools`]. The daemon keeps only this
//! re-export so its routes, catalog builder and Connect lane keep compiling
//! against the same names while there is exactly ONE registry per process.
//!
//! Nothing here decides anything: a second table would let the builtin set and
//! the peer set drift apart, which is the failure this cutover removes.

pub use nexus_core::execution::capabilities::{
    AdmissionGate, Access, CatalogDescriptor, CapabilityRegistry, CapabilityRow, FailureMode,
    NAMED_PLACEHOLDER_INPUT, TestVector, UserCapCatalogRefusal, build_registry,
    host_tool_registry, json_schema_has_object_root, user_cap_catalog_admission,
};
