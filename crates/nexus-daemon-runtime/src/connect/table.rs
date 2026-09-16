//! P4-T3 retirement shim: the module body moved to `nexus_core::connect::table`
//! (transport-neutral core). This re-export keeps the daemon call sites and
//! public names stable; there is no daemon-owned business body left here.
pub use nexus_core::connect::table::*;
