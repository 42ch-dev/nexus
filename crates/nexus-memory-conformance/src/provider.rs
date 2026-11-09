use std::path::Path;

use chrono::{SecondsFormat, Utc};
use sqlx::Row;
use uuid::Uuid;

use crate::faults::FixtureFaults;
use crate::storage::FixtureStore;
use crate::FixtureError;

const SPACE_DISPLAY_NAME: &str = "Fixture memory";

/// Durable identity of the fixture space.
///
/// The pair `(authority_host_id, authority_generation)` is what a remote client
/// pins after admission, so both values must come from the stored row rather
/// than from process state: a restart serves the same authority as the session
/// that provisioned it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureIdentity {
    /// The single space this provider serves.
    pub space_id: String,
    /// Opaque authority host label of this provider instance.
    pub authority_host_id: String,
    /// Monotonic authority generation, `1` on first provisioning.
    pub authority_generation: u64,
}

/// The independent durable provider the conformance suite exercises.
pub struct FixtureMemoryProvider {
    store: FixtureStore,
    /// Crate-private: the operation slices pin every request to the durable
    /// space/authority pair this provider serves.
    pub(crate) identity: FixtureIdentity,
    /// Operator/test-only control (C7 seam): bounded fault points only. It is
    /// in-memory state attached to this provider instance, never persisted and
    /// never consulted as provider authority.
    faults: FixtureFaults,
}

impl FixtureMemoryProvider {
    /// Open the scenario database at `database_path`, applying the fixture
    /// migrations and provisioning the single space on first use.
    ///
    /// Reopening the same file reads the persisted space and authority instead
    /// of provisioning again.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`] when the file cannot be opened, a migration
    /// fails, or the database already holds a bootstrap state this provider
    /// cannot serve (for example more than one space).
    pub async fn open(database_path: &Path) -> Result<Self, FixtureError> {
        let store = FixtureStore::open(database_path).await?;
        let identity = provision_space(&store).await?;
        Ok(Self {
            store,
            identity,
            faults: FixtureFaults::new(),
        })
    }

    /// The durable space/authority identity provisioned for this database.
    #[must_use]
    pub const fn identity(&self) -> &FixtureIdentity {
        &self.identity
    }

    /// Operator provisioning of the space's initial administrator grant
    /// (§7.0.6).
    ///
    /// This is the non-protocol bootstrap path: it runs before any admin exists
    /// and stays outside `grants.grant`'s non-amplification rule, so the space
    /// starts from an operator decision rather than an uncallable grant cycle.
    /// Repeating it for the same principal returns the same grant.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`] when the bootstrap transaction fails or the
    /// stored grant rows cannot be read.
    pub async fn provision_operator_admin(
        &self,
        principal_id: &str,
    ) -> Result<String, FixtureError> {
        self.bootstrap_operator_admin(principal_id).await
    }

    /// The operator/test-only fault and clock control for this provider
    /// instance (C7 seam). Arming a point never changes durable state; the
    /// provider's mutation path observes it once.
    #[must_use]
    pub const fn faults(&self) -> &FixtureFaults {
        &self.faults
    }

    /// Storage handle for the provider's operation slices.
    pub(crate) const fn store(&self) -> &FixtureStore {
        &self.store
    }

    /// Close the pool so the caller can reopen the same file as a fresh
    /// session.
    pub async fn close(self) {
        self.store().close().await;
    }
}

/// Read the stored space, provisioning it when the database is empty.
async fn provision_space(store: &FixtureStore) -> Result<FixtureIdentity, FixtureError> {
    let mut transaction = store.pool().begin().await?;
    let rows = sqlx::query(
        "SELECT space_id, authority_host_id, authority_generation \
         FROM spaces ORDER BY space_id",
    )
    .fetch_all(&mut *transaction)
    .await?;

    let identity = match rows.as_slice() {
        [] => {
            let identity = FixtureIdentity {
                space_id: format!("spc_{}", Uuid::new_v4().simple()),
                authority_host_id: format!("fixture-{}", Uuid::new_v4().simple()),
                authority_generation: 1,
            };
            sqlx::query(
                "INSERT INTO spaces (space_id, display_name, authority_host_id, \
                 authority_generation, status, revision, created_at) \
                 VALUES (?, ?, ?, ?, 'active', 1, ?)",
            )
            .bind(&identity.space_id)
            .bind(SPACE_DISPLAY_NAME)
            .bind(&identity.authority_host_id)
            .bind(1_i64)
            .bind(Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true))
            .execute(&mut *transaction)
            .await?;
            // The private snapshot marker exists for every provisioned space,
            // so the mutators can advance it without a lazy-create branch.
            sqlx::query("INSERT INTO provider_snapshot (space_id, marker) VALUES (?, 1)")
                .bind(&identity.space_id)
                .execute(&mut *transaction)
                .await?;
            identity
        }
        [row] => FixtureIdentity {
            space_id: row.try_get("space_id")?,
            authority_host_id: row.try_get("authority_host_id")?,
            authority_generation: u64::try_from(row.try_get::<i64, _>("authority_generation")?)
                .map_err(|_| {
                    FixtureError::Bootstrap("authority_generation is not positive".to_owned())
                })?,
        },
        _ => {
            return Err(FixtureError::Bootstrap(format!(
                "expected one provisioned space, found {}",
                rows.len()
            )))
        }
    };

    transaction.commit().await?;
    Ok(identity)
}
