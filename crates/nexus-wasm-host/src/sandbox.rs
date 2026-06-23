//! Sandbox configuration for a single compute invocation (compass Q6).
//!
//! Every `compute()` call runs in a fresh, fully isolated instance with three
//! independent guards:
//!
//! 1. **Fuel** — instruction-level budget. Every executed wasm instruction
//!    consumes fuel; the instance traps with `Trap::OutOfFuel` once depleted.
//!    This is the primary compute-bound guard and bounds CPU work.
//! 2. **Memory cap** — total linear-memory ceiling, enforced via wasmtime's
//!    `StoreLimits` resource limiter. Prevents runaway allocation / OOM.
//! 3. **Wall-time** — enforced via epoch interruption: a watchdog thread calls
//!    `Engine::increment_epoch()` after the deadline, trapping the instance
//!    (`Trap::Interrupt`). Hard cap on real time.

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
/// instance and is reported as a [`ComputeError`](crate::ComputeError) variant.
/// Instances are never reused (per-invocation sandbox), so limits always start
/// from a clean slate.
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

impl SandboxConfig {
    /// Returns the configured memory cap in MiB (rounded down).
    #[must_use]
    pub fn memory_mib(&self) -> u32 {
        u32::try_from(self.max_memory_bytes / (1024 * 1024)).unwrap_or(u32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_compass_q6() {
        let cfg = SandboxConfig::default();
        assert_eq!(cfg.fuel, DEFAULT_FUEL);
        assert_eq!(cfg.memory_mib(), DEFAULT_MEMORY_MIB);
        assert_eq!(cfg.wall_time, DEFAULT_WALL_TIME);
    }

    #[test]
    fn with_memory_mib_converts_bytes() {
        let cfg = SandboxConfig::with_memory_mib(8);
        assert_eq!(cfg.max_memory_bytes, 8 * 1024 * 1024);
        assert_eq!(cfg.memory_mib(), 8);
    }
}
