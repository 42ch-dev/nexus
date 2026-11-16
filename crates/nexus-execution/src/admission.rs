//! The applied-identity admission port.
//!
//! Run admission must resolve the strategy's current apply before it builds a
//! graph: the apply decides the intake mode and the frozen document the run is
//! built from. That resolution reads the Creator store, the preset loader and
//! the home config — none of which this crate owns — so the engine asks this
//! port, exactly as it asks the run-event port for its rings. The owner half
//! implements it once and shares that implementation between admission and
//! recovery, so both re-enter through the same resolution.
//!
//! The port is MANDATORY on the coordinator, never `Option`: an unwired port
//! could otherwise skip the `ir-only` no-apply refusal, and re-deriving the
//! user-bundle classification inside the engine would be a second classifier
//! beside the one the apply gate refuses on.

use async_trait::async_trait;
use nexus_home_layout::strategy_intake::StrategyIntakeMode;
use nexus_orchestration::capability::AppliedDocument;
use nexus_preset::LoadedPreset;

/// What one admission resolved: the intake mode it runs under and the applied
/// document it builds the graph from.
///
/// `intake_mode: None` means the preset is not a user bundle, so no intake key
/// governs it; `applied: None` means the strategy has no current apply.
pub struct AppliedAdmission {
    /// The `[strategy_intake]` mode this admission runs under, when one does.
    pub intake_mode: Option<StrategyIntakeMode>,
    /// The applied document the run's graph is built from.
    pub applied: Option<AppliedDocument>,
}

/// Why one admission resolution refused.
#[derive(Debug)]
pub enum AppliedAdmissionError {
    /// The store could not be read. Each site maps it to its own store variant.
    Store(String),
    /// `[strategy_intake]` is unreadable or names a mode outside the known arms.
    InvalidIntakeConfig(String),
    /// The applied identity no longer replays: a coded refusal, not a fault.
    AppliedIdentity {
        /// Stable lowercase wire code (`applied_pin_*`, `applied_document_*`).
        code: String,
        /// The sentence naming the strategy and the apply.
        message: String,
    },
}

/// Resolve the applied identity one run admission runs under.
#[async_trait]
pub trait AppliedAdmissionPort: Send + Sync {
    /// Resolve the intake mode and the applied document for `preset_id` in
    /// `nexus_home`.
    ///
    /// `nexus_home` is the home the caller resolved the preset from, passed in
    /// rather than held by the port: the intake key and the pin re-comparison
    /// must read the same workspace as the bundle the run is admitted with.
    ///
    /// # Errors
    ///
    /// Returns [`AppliedAdmissionError`] when the store read fails, the intake
    /// config is invalid, or the current apply no longer replays.
    async fn admit(
        &self,
        nexus_home: &std::path::Path,
        preset_id: &str,
        loaded: &LoadedPreset,
    ) -> Result<AppliedAdmission, AppliedAdmissionError>;
}
