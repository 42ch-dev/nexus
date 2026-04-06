//! nexus42d — Nexus Daemon Library
//!
//! This library exposes the daemon's internal modules for testing
//! and future embedding. The binary entry point is in `main.rs`.

#![deny(clippy::unwrap_used)]

pub mod api;
pub mod auth;
pub mod db;
pub mod workspace;
