//! Compute sandbox policy: the default limits and how module/manifest
//! overrides resolve against them (compass Q6).
//!
//! This crate is **private policy only** — plain `std`, no runtime. The
//! enforcement lives in its consumers: `nexus-wasm-host` owns fuel, traps, the
//! `StoreLimits` memory cap and the epoch watchdog, while
//! `nexus-orchestration` projects the same maxima onto capability descriptors.
//!
//! The three limits are independent; each resolves on its own:
//!
//! 1. **Fuel** — instruction-level budget. Every executed wasm instruction
//!    consumes fuel; the instance traps with `Trap::OutOfFuel` once depleted.
//! 2. **Memory cap** — total linear-memory ceiling in bytes.
//! 3. **Wall-time** — hard real-time cap for a single invocation.

use std::time::Duration;

/// Default fuel budget: 10 million instructions.
///
/// Sufficient for character-scale combat compute (parsing a small JSON envelope
/// and emitting a 4-part output) while bounding an infinite loop to well under
/// the wall-time limit on commodity hardware. Compass Q6.
pub const DEFAULT_FUEL: u64 = 10_000_000;

/// Default memory cap: 64 MiB. Compass Q6.
pub const DEFAULT_MEMORY_MIB: u32 = 64;

/// Default wall-time limit: 30 seconds. Compass Q6.
pub const DEFAULT_WALL_TIME: Duration = Duration::from_secs(30);

/// Sandbox limits applied to a single compute invocation.
///
/// All limits are enforced independently; a breach of any one traps the
/// instance. Instances are never reused (per-invocation sandbox), so limits
/// always start from a clean slate.
#[derive(Debug, Clone, Copy)]
pub struct SandboxConfig {
    /// Instruction-level fuel budget.
    pub fuel: u64,
    /// Maximum total linear-memory size in bytes.
    pub max_memory_bytes: usize,
    /// Maximum wall-clock time a single invocation may run.
    pub wall_time: Duration,
}

impl SandboxConfig {
    /// Build a `SandboxConfig` from a memory size expressed in MiB.
    #[must_use]
    pub fn with_memory_mib(memory_mib: u32) -> Self {
        Self {
            fuel: DEFAULT_FUEL,
            max_memory_bytes: usize::try_from(memory_mib)
                .unwrap_or(0)
                .saturating_mul(1024 * 1024),
            wall_time: DEFAULT_WALL_TIME,
        }
    }

    /// Resolve optional overrides against this configuration as the base.
    ///
    /// An override may only **tighten** a limit, never raise it: the effective
    /// value is `min(override, base)`. Absent overrides keep the base value.
    /// A MiB override that cannot be represented saturates rather than wrapping;
    /// the result is still capped by this configuration's memory cap.
    #[must_use]
    pub fn resolve(
        self,
        fuel: Option<u64>,
        memory_mib: Option<u32>,
        wall_time_ms: Option<u64>,
    ) -> Self {
        Self {
            fuel: fuel.map_or(self.fuel, |f| f.min(self.fuel)),
            max_memory_bytes: memory_mib.map_or(self.max_memory_bytes, |mib| {
                usize::try_from(mib)
                    .unwrap_or(0)
                    .saturating_mul(1024 * 1024)
                    .min(self.max_memory_bytes)
            }),
            wall_time: wall_time_ms.map_or(self.wall_time, |ms| {
                Duration::from_millis(ms).min(self.wall_time)
            }),
        }
    }

    /// Returns the configured memory cap in MiB (rounded down).
    #[must_use]
    pub fn memory_mib(&self) -> u32 {
        u32::try_from(self.max_memory_bytes / (1024 * 1024)).unwrap_or(u32::MAX)
    }
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            fuel: DEFAULT_FUEL,
            max_memory_bytes: (DEFAULT_MEMORY_MIB as usize) * 1024 * 1024,
            wall_time: DEFAULT_WALL_TIME,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_compass_q6() {
        // Compare against the compass Q6 product literals directly — `Default`
        // and the constants are both derived from these, so asserting against
        // the constants alone would let the two drift together (QC1-03).
        assert_eq!(DEFAULT_FUEL, 10_000_000);
        assert_eq!(DEFAULT_MEMORY_MIB, 64);
        assert_eq!(DEFAULT_WALL_TIME, Duration::from_secs(30));
        let cfg = SandboxConfig::default();
        assert_eq!(cfg.fuel, 10_000_000);
        // QC1-03 round 2 (D): the exact BYTE default, not the floored MiB
        // projection — `memory_mib()` alone would pass a drifted byte value.
        assert_eq!(cfg.max_memory_bytes, 67_108_864);
        assert_eq!(cfg.memory_mib(), 64);
        assert_eq!(cfg.wall_time, Duration::from_secs(30));
    }

    #[test]
    fn with_memory_mib_converts_bytes() {
        let cfg = SandboxConfig::with_memory_mib(8);
        assert_eq!(cfg.max_memory_bytes, 8 * 1024 * 1024);
        assert_eq!(cfg.memory_mib(), 8);
    }

    #[test]
    fn resolve_preserves_non_mib_host_byte_ceiling() {
        // Host ceiling that is NOT MiB-aligned: 5 MiB + 12,345 bytes.
        let base = SandboxConfig {
            fuel: DEFAULT_FUEL,
            max_memory_bytes: 5 * 1024 * 1024 + 12_345,
            wall_time: DEFAULT_WALL_TIME,
        };

        // A manifest request higher than the base must keep the EXACT
        // original byte ceiling: the min() runs on bytes, so no round-up or
        // round-down may pass through the MiB conversion.
        let clamped = base.resolve(None, Some(64), None);
        assert_eq!(clamped.max_memory_bytes, 5 * 1024 * 1024 + 12_345);
        assert_eq!(clamped.fuel, DEFAULT_FUEL);
        assert_eq!(clamped.wall_time, DEFAULT_WALL_TIME);

        // A request strictly lower than the base still clamps (controls the
        // control above), in exact bytes, not rounded MiB.
        let lowered = base.resolve(None, Some(2), None);
        assert_eq!(lowered.max_memory_bytes, 2 * 1024 * 1024);

        // Absent overrides keep every base limit untouched, including the
        // unaligned byte ceiling.
        let untouched = base.resolve(None, None, None);
        assert_eq!(untouched.max_memory_bytes, 5 * 1024 * 1024 + 12_345);
        assert_eq!(untouched.fuel, DEFAULT_FUEL);
        assert_eq!(untouched.wall_time, DEFAULT_WALL_TIME);
    }
}
