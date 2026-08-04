//! Device ID generation and persistence.
//!
//! V1.148 P3 N-C0: the implementation moved to `nexus-home-layout` (the path
//! layout SSOT already owns `device_id_path`; the value read/write belongs
//! next to it). This module re-exports the moved items so the public call
//! path `nexus_cloud_sync::device_id::get_or_create_device_id` (used by
//! `apps/nexus42` main.rs) is unchanged.
pub use nexus_home_layout::device_id::{get_or_create_device_id, DeviceIdError};
