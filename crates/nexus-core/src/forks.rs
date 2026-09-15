//! Local timeline forks: immutable parent history, canon lineage markers.
//! The storage write is the shared narrative repository operation; no branch
//! registry, history copy or community/social fork is introduced here.

use nexus_contracts::daemon_api::{CreateForkRequest, CreateForkResponse};
use nexus_local_db::narrative_write;
use serde_json::json;

use crate::world_kb::{db_err, guards};
use crate::{CoreAccess, CoreError, CoreResult, CoreService, Principal};

impl CoreService {
    /// Create a local branch at an existing event in an owned World.
    ///
    /// # Errors
    /// Rejects invalid principals, read-only access, missing/foreign Worlds,
    /// invalid fork points and storage failures, before writing any marker.
    pub async fn create_fork(
        &self,
        principal: &Principal,
        world_id: String,
        request: CreateForkRequest,
    ) -> CoreResult<CreateForkResponse> {
        self.verify_principal(principal)?;
        if self.inner.access == CoreAccess::ReadOnly {
            return Err(CoreError::Forbidden {
                resource: "fork_create: read-only core access".to_string(),
            });
        }
        let pool = &self.inner.pool;
        guards::require_world_owner(pool, &world_id, principal.creator_id()).await?;

        let event: Option<String> = sqlx::query_scalar(
            "SELECT timeline_event_id FROM narrative_timeline_events \
             WHERE timeline_event_id = ? AND world_id = ? AND branch_id = ?",
        )
        .bind(&request.forked_from_event_id)
        .bind(&world_id)
        .bind(&request.parent_branch_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| db_err(&e))?;
        if event.is_none() {
            return Err(CoreError::InvalidInput {
                field: "fork_point".to_string(),
                reason: "fork point not found on parent branch".to_string(),
            });
        }

        let branch_id = format!("fbk_{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
        let label = request.label.map(String::from).unwrap_or_else(|| "fork".to_string());
        let summary = format!(
            "forked from {}/{} ({label})",
            request.parent_branch_id, request.forked_from_event_id
        );
        let lineage = json!({
            "fork_lineage": {
                "parent_branch_id": request.parent_branch_id,
                "forked_from_event_id": request.forked_from_event_id,
                "label": label,
            }
        }).to_string();
        let marker = narrative_write::append_event_canon_with_extensions(
            pool, &world_id, &branch_id, "fork_created", Some(&label), Some(&summary), &lineage,
        )
        .await
        .map_err(|e| CoreError::Internal { category: format!("fork marker append: {e}") })?;

        // The generated response owns the timestamp format validation.
        serde_json::from_value(json!({
            "branch_id": branch_id,
            "parent_branch_id": request.parent_branch_id,
            "forked_from_event_id": request.forked_from_event_id,
            "created_at": marker.created_at,
        }))
        .map_err(|e| CoreError::Internal { category: format!("fork response: {e}") })
    }
}
