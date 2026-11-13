//! Read-only pattern package registry: the daemon's listing of `<nexus_home>/patterns`.
//!
//! The reader owns discovery, manifest validation and the fragment pin; this module only points it
//! at the home layout and hands the observed packages to the wire. Nothing here writes, creates the
//! directory or loads a capability: a missing directory is an empty listing.

use crate::{CoreError, CoreResult, CoreService, Principal};
use nexus_contracts::generated::daemon_api::patterns::pattern_summary::PatternSummary;

impl CoreService {
    /// Every package directory under `<nexus_home>/patterns`, unavailable ones included.
    ///
    /// # Errors
    ///
    /// Returns `CoreError::Internal` when the root directory cannot be enumerated, and the
    /// principal/auth errors of [`CoreService::verify_principal`] otherwise.
    #[allow(clippy::unused_async)] // async is the await-symmetric public signature; the scan is sync
    #[allow(clippy::unused_async_trait_impl)] // same signature shape as the sibling listing methods
    pub async fn list_patterns(&self, principal: &Principal) -> CoreResult<Vec<PatternSummary>> {
        self.verify_principal(principal)?;
        let patterns_dir = self.inner.nexus_home.join("patterns");
        let packages =
            nexus_pattern::scan_pattern_packages(&patterns_dir, nexus_workflow_ir::IR_VERSION)
                .map_err(|error| CoreError::Internal {
                    category: format!("pattern registry {}: {error}", patterns_dir.display()),
                })?;
        Ok(packages
            .iter()
            .map(PatternSummary::from)
            .collect::<Vec<_>>())
    }
}
