//! The applied-identity admission port.
//!
// why: run admission must resolve the strategy's current apply before it builds a
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
use nexus_orchestration::applied_document::AppliedDocument;
use nexus_preset::LoadedPreset;

/// note: what one admission resolved: the intake mode it runs under and the applied
/// document it builds the graph from.
///
/// `intake_mode: None` means the preset is not a user bundle, so no intake key
/// governs it; `applied: None` means the strategy has no current apply (never an
/// embedded bundle, whose document always resolves).
pub struct AppliedAdmission {
    /// note: the `[strategy_intake]` mode this admission runs under, when one does.
    pub intake_mode: Option<StrategyIntakeMode>,
    /// note: the applied document the run's graph is built from.
    pub applied: Option<AppliedDocument>,
}

/// note: why one admission resolution refused.
#[derive(Debug)]
pub enum AppliedAdmissionError {
    /// note: the store could not be read. Each site maps it to its own store variant.
    Store(String),
    /// note: `[strategy_intake]` is unreadable or names a mode outside the known arms.
    InvalidIntakeConfig(String),
    /// note: the applied identity no longer replays: a coded refusal, not a fault.
    AppliedIdentity {
        /// note: stable lowercase wire code (`applied_pin_*`, `applied_document_*`).
        code: String,
        /// note: the sentence naming the strategy and the apply.
        message: String,
    },
}

/// note: resolve the applied identity one run admission runs under.
#[async_trait]
pub trait AppliedAdmissionPort: Send + Sync {
    /// note: resolve the intake mode and the applied document for `preset_id`.
    ///
    /// The port takes no home: the workspace whose `[strategy_intake]` key and
    /// pattern pins this resolution reads is the one the implementation holds,
    /// which is also the home recovery re-compares those pins against. A
    /// caller-supplied home would be a second value that only convention keeps
    /// equal, and recovery has no caller to take it from.
    ///
    /// Admission and recovery both read the owner's pins. That is not the same
    /// as requiring the pin home to equal the preset lookup home — a caller's
    /// home still chooses which bundle is resolved, and the applied arm reads
    /// the owner's document rather than the caller's bundle.
    ///
    /// # Errors
    ///
    /// Returns [`AppliedAdmissionError`] when the store read fails, the intake
    /// config is invalid, or the current apply no longer replays.
    async fn admit(
        &self,
        preset_id: &str,
        loaded: &LoadedPreset,
    ) -> Result<AppliedAdmission, AppliedAdmissionError>;
}
