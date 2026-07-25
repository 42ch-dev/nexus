//! # nexus-spoke-adapter
//!
//! The single boundary that crosses between nexus domain concerns and SPOKE
//! standard objects. It does two things and nothing else:
//!
//! 1. **Typed accessors** over the `extensions.nexus` namespace on a spoke
//!    [`KnowledgeEntry`] — see the [`extensions`] module.
//! 2. **Thin delegation** of standard lifecycle invariants to
//!    [`spoke_operations`] — see the [`ops`] module.
//!
//! This crate is a **delegation facade** (tracked spec
//! `spoke-adapter-architecture.md` §1.2 / §7): where `spoke-operations`
//! already exports a function, this adapter re-exports or thin-wraps it. It
//! does NOT reimplement any lifecycle invariant, and it introduces no
//! parallel nexus types where spoke already provides them.
//!
//! ## Call-boundary invariant (HARD)
//!
//! Every public function in this crate accepts and returns only spoke
//! standard objects (`KnowledgeEntry`, `Finding`, `Scope`, `PromoteRequest`,
//! `AssemblePacket`, `ExtensionMap`). There are no nexus wrapper types here
//! — the adapter IS the boundary.

pub mod extensions;
pub mod ops;

// ── Spoke type re-exports (consumer convenience) ────────────────────────
//
// Consumers depend on `nexus-spoke-adapter` for both the accessors and the
// spoke types that appear in the public API surface, so they do not need a
// direct `spoke-schemas` / `spoke-operations` dependency just to spell the
// operand types. These mirror `spoke_operations`' own `pub use spoke_schemas`.

pub use spoke_operations::{ExtensionMap, SpokeReject, SpokeRejectCode, SpokeResult};
pub use spoke_schemas::{AssemblePacket, Finding, KnowledgeEntry, PromoteRequest};
