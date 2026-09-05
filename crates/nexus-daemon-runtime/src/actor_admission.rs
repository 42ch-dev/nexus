//! Stored-data Actor admission for agent-host session and prompt paths (v1.184 P2 Task 1).
//!
//! Callers never treat request-body ownership claims as trusted context.
//! Any deny returns before MCA, HostFacade session work, registry insertion,
//! or provider calls.

use crate::actor_knowledge_view::{
    ActorKnowledgePage, ActorKnowledgeViewQuery, ActorKnowledgeViewService, AdmittedActor,
};
use crate::api::errors::NexusApiError;
use sqlx::SqlitePool;

/// Pair classification for optional Actor/Viewpoint fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorPairMode {
    /// Both fields absent — exact legacy path.
    Legacy,
    /// Both fields present — Actor mode.
    Actor,
}

/// Optional viewpoint identifiers after pair classification.
#[derive(Debug, Clone)]
pub struct ActorViewpoint {
    pub world_id: String,
    pub binding_id: Option<String>,
    pub branch_id: Option<String>,
    pub event_id: Option<String>,
}

/// Admitted Actor + viewpoint plus the P1 bounded KnowledgeView.
#[derive(Debug, Clone)]
pub struct AdmittedActorContext {
    pub actor: AdmittedActor,
    pub world_id: String,
    pub binding_id: Option<String>,
    pub branch_id: Option<String>,
    pub event_id: Option<String>,
    pub view: ActorKnowledgePage,
}

/// One reusable stored-data admission service.
pub struct ActorAdmissionService {
    views: ActorKnowledgeViewService,
    pool: SqlitePool,
}

impl ActorAdmissionService {
    /// Bind admission to a workspace pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            views: ActorKnowledgeViewService::new(pool.clone()),
            pool,
        }
    }

    /// Classify the optional pair. Partial pairs are a stable 400.
    ///
    /// # Errors
    ///
    /// Returns `invalid_input` when exactly one of the pair is present.
    pub fn classify_pair(
        actor_present: bool,
        viewpoint_present: bool,
    ) -> Result<ActorPairMode, NexusApiError> {
        match (actor_present, viewpoint_present) {
            (false, false) => Ok(ActorPairMode::Legacy),
            (true, true) => Ok(ActorPairMode::Actor),
            (true, false) | (false, true) => Err(NexusApiError::BadRequest {
                code: "invalid_input".into(),
                message: "actor_ref and viewpoint must both be present or both absent".into(),
            }),
        }
    }

    /// Admit stored Creator/Character/World/binding ownership and load the P1 view.
    ///
    /// # Errors
    ///
    /// Auth, ownership, status, or view-composition failures. No host/MCA side effects.
    pub async fn admit(
        &self,
        caller_creator_id: &str,
        actor: AdmittedActor,
        viewpoint: ActorViewpoint,
    ) -> Result<AdmittedActorContext, NexusApiError> {
        match &actor {
            AdmittedActor::Creator { creator_id } => {
                if creator_id != caller_creator_id {
                    return Err(not_found("actor_ref", creator_id));
                }
                if viewpoint.binding_id.is_some() {
                    return Err(NexusApiError::BadRequest {
                        code: "invalid_input".into(),
                        message: "binding_id must be omitted for Creator actor_ref".into(),
                    });
                }
                self.require_active_owned_world(caller_creator_id, &viewpoint.world_id)
                    .await?;
            }
            AdmittedActor::Character { character_id } => {
                let Some(binding) = viewpoint.binding_id.as_deref() else {
                    return Err(NexusApiError::BadRequest {
                        code: "invalid_input".into(),
                        message: "binding_id is required for Character actor_ref".into(),
                    });
                };
                self.require_active_owned_character(caller_creator_id, character_id)
                    .await?;
                self.require_active_owned_world(caller_creator_id, &viewpoint.world_id)
                    .await?;
                self.views
                    .require_active_binding(character_id, binding, &viewpoint.world_id)
                    .await?;
            }
        }

        let view = self
            .complete_view(
                caller_creator_id,
                &actor,
                &viewpoint.world_id,
                viewpoint.binding_id.clone(),
            )
            .await?;
        Ok(AdmittedActorContext {
            actor,
            world_id: viewpoint.world_id,
            binding_id: viewpoint.binding_id,
            branch_id: viewpoint.branch_id,
            event_id: viewpoint.event_id,
            view,
        })
    }

    async fn complete_view(
        &self,
        caller_creator_id: &str,
        actor: &AdmittedActor,
        world_id: &str,
        binding_id: Option<String>,
    ) -> Result<ActorKnowledgePage, NexusApiError> {
        let limit = ActorKnowledgeViewService::resolve_limit(Some(100))?;
        self.views
            .view(
                caller_creator_id,
                actor,
                ActorKnowledgeViewQuery {
                    world_id: world_id.to_string(),
                    binding_id,
                    limit,
                    cursor: None,
                },
            )
            .await
    }

    async fn require_active_owned_world(
        &self,
        creator_id: &str,
        world_id: &str,
    ) -> Result<(), NexusApiError> {
        let row = sqlx::query!(
            r#"SELECT owner_creator_id as "owner_creator_id!", status as "status!"
               FROM narrative_worlds WHERE world_id = ?"#,
            world_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(NexusApiError::from)?;
        match row {
            Some(stored) if stored.owner_creator_id == creator_id && stored.status == "active" => {
                Ok(())
            }
            Some(stored) if stored.owner_creator_id == creator_id => {
                Err(NexusApiError::ConflictCoded {
                    code: "world_inactive".into(),
                    message: format!("world {world_id} is {}", stored.status),
                })
            }
            _ => Err(not_found("world", world_id)),
        }
    }

    async fn require_active_owned_character(
        &self,
        creator_id: &str,
        character_id: &str,
    ) -> Result<(), NexusApiError> {
        let row = nexus_local_db::get_character(&self.pool, creator_id, character_id).await?;
        match row {
            Some(stored) if stored.status == "active" => Ok(()),
            Some(stored) => Err(NexusApiError::ConflictCoded {
                code: "character_inactive".into(),
                message: format!("character {character_id} is {}", stored.status),
            }),
            None => Err(not_found("character", character_id)),
        }
    }
}

fn not_found(resource: &str, id: &str) -> NexusApiError {
    NexusApiError::NotFound(format!("{resource} {id}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_test_workspace;
    use nexus_local_db::{ensure_creator_row, CreateCharacterParams};

    const OWNER: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OTHER: &str = "ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const WORLD: &str = "wld_worldA";

    async fn seed() -> (ActorAdmissionService, String, String) {
        let (_tmp, home, db_path) = create_test_workspace().await;
        let state = crate::workspace::WorkspaceState::new_for_testing(home, db_path, None).await;
        let pool = state.pool().unwrap().clone();
        ensure_creator_row(&pool, OWNER, "Owner").await.unwrap();
        ensure_creator_row(&pool, OTHER, "Other").await.unwrap();
        sqlx::query(
            "INSERT INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
              time_policy, metadata_json, created_at) \
             VALUES (?, 'ws', ?, ?, ?, 'active', 'private', 'manual', '{}', datetime('now'))",
        )
        .bind(WORLD)
        .bind(OWNER)
        .bind(WORLD)
        .bind(WORLD)
        .execute(&pool)
        .await
        .unwrap();
        let created = nexus_local_db::create_character_with_initial_binding(
            &pool,
            CreateCharacterParams {
                owner_creator_id: OWNER,
                display_name: "Ada",
                image_uri: None,
                persona_json: "{}",
                world_id: WORLD,
                world_sheet_entry_id: None,
            },
        )
        .await
        .unwrap();
        (
            ActorAdmissionService::new(pool),
            created.character.character_id,
            created.binding.binding_id,
        )
    }

    #[test]
    fn classify_pair_both_absent_is_legacy() {
        assert_eq!(
            ActorAdmissionService::classify_pair(false, false).unwrap(),
            ActorPairMode::Legacy
        );
    }

    #[tokio::test]
    async fn admits_character_with_active_binding() {
        let (svc, character_id, binding_id) = seed().await;
        let ctx = svc
            .admit(
                OWNER,
                AdmittedActor::Character {
                    character_id: character_id.clone(),
                },
                ActorViewpoint {
                    world_id: WORLD.into(),
                    binding_id: Some(binding_id.clone()),
                    branch_id: None,
                    event_id: None,
                },
            )
            .await
            .expect("admit");
        assert_eq!(ctx.world_id, WORLD);
        assert_eq!(ctx.binding_id.as_deref(), Some(binding_id.as_str()));
        match ctx.actor {
            AdmittedActor::Character { character_id: id } => assert_eq!(id, character_id),
            _ => panic!("expected character"),
        }
    }

    #[tokio::test]
    async fn creator_with_binding_is_invalid_input() {
        let (svc, _, binding_id) = seed().await;
        let err = svc
            .admit(
                OWNER,
                AdmittedActor::Creator {
                    creator_id: OWNER.into(),
                },
                ActorViewpoint {
                    world_id: WORLD.into(),
                    binding_id: Some(binding_id),
                    branch_id: None,
                    event_id: None,
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err.error_code(), "invalid_input");
    }

    #[tokio::test]
    async fn foreign_creator_is_not_found() {
        let (svc, _, _) = seed().await;
        let err = svc
            .admit(
                OWNER,
                AdmittedActor::Creator {
                    creator_id: OTHER.into(),
                },
                ActorViewpoint {
                    world_id: WORLD.into(),
                    binding_id: None,
                    branch_id: None,
                    event_id: None,
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err.error_code(), "not_found");
    }

    async fn assert_deny(
        svc: &ActorAdmissionService,
        actor: AdmittedActor,
        viewpoint: ActorViewpoint,
        code: &str,
        status: axum::http::StatusCode,
    ) {
        let err = svc.admit(OWNER, actor, viewpoint).await.unwrap_err();
        assert_eq!(err.error_code(), code);
        assert_eq!(err.status_code(), status);
    }

    #[tokio::test]
    async fn deny_matrix_world_character_binding_mismatches() {
        let (svc, character_id, binding_id) = seed().await;
        let pool = &svc.pool;

        sqlx::query(
            "INSERT INTO narrative_worlds              (world_id, workspace_id, owner_creator_id, title, slug, status, visibility,               time_policy, metadata_json, created_at)              VALUES ('wld_foreign', 'ws', ?, 'f', 'f', 'active', 'private', 'manual', '{}', datetime('now'))",
        )
        .bind(OTHER)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO narrative_worlds              (world_id, workspace_id, owner_creator_id, title, slug, status, visibility,               time_policy, metadata_json, created_at)              VALUES ('wld_inactive', 'ws', ?, 'i', 'i', 'archived', 'private', 'manual', '{}', datetime('now'))",
        )
        .bind(OWNER)
        .execute(pool)
        .await
        .unwrap();
        let other_char = nexus_local_db::create_character_with_initial_binding(
            pool,
            CreateCharacterParams {
                owner_creator_id: OWNER,
                display_name: "Other",
                image_uri: None,
                persona_json: "{}",
                world_id: WORLD,
                world_sheet_entry_id: None,
            },
        )
        .await
        .unwrap();

        assert_deny(
            &svc,
            AdmittedActor::Creator {
                creator_id: OWNER.into(),
            },
            ActorViewpoint {
                world_id: "wld_missing".into(),
                binding_id: None,
                branch_id: None,
                event_id: None,
            },
            "not_found",
            axum::http::StatusCode::NOT_FOUND,
        )
        .await;
        assert_deny(
            &svc,
            AdmittedActor::Creator {
                creator_id: OWNER.into(),
            },
            ActorViewpoint {
                world_id: "wld_foreign".into(),
                binding_id: None,
                branch_id: None,
                event_id: None,
            },
            "not_found",
            axum::http::StatusCode::NOT_FOUND,
        )
        .await;
        assert_deny(
            &svc,
            AdmittedActor::Creator {
                creator_id: OWNER.into(),
            },
            ActorViewpoint {
                world_id: "wld_inactive".into(),
                binding_id: None,
                branch_id: None,
                event_id: None,
            },
            "world_inactive",
            axum::http::StatusCode::CONFLICT,
        )
        .await;

        sqlx::query("UPDATE characters SET status = 'archived' WHERE character_id = ?")
            .bind(&character_id)
            .execute(pool)
            .await
            .unwrap();
        assert_deny(
            &svc,
            AdmittedActor::Character {
                character_id: character_id.clone(),
            },
            ActorViewpoint {
                world_id: WORLD.into(),
                binding_id: Some(binding_id.clone()),
                branch_id: None,
                event_id: None,
            },
            "character_inactive",
            axum::http::StatusCode::CONFLICT,
        )
        .await;
        sqlx::query("UPDATE characters SET status = 'active' WHERE character_id = ?")
            .bind(&character_id)
            .execute(pool)
            .await
            .unwrap();

        sqlx::query("UPDATE actor_world_bindings SET status = 'inactive' WHERE binding_id = ?")
            .bind(&binding_id)
            .execute(pool)
            .await
            .unwrap();
        assert_deny(
            &svc,
            AdmittedActor::Character {
                character_id: character_id.clone(),
            },
            ActorViewpoint {
                world_id: WORLD.into(),
                binding_id: Some(binding_id.clone()),
                branch_id: None,
                event_id: None,
            },
            "not_found",
            axum::http::StatusCode::NOT_FOUND,
        )
        .await;
        sqlx::query("UPDATE actor_world_bindings SET status = 'active' WHERE binding_id = ?")
            .bind(&binding_id)
            .execute(pool)
            .await
            .unwrap();

        assert_deny(
            &svc,
            AdmittedActor::Character {
                character_id: character_id.clone(),
            },
            ActorViewpoint {
                world_id: WORLD.into(),
                binding_id: Some(other_char.binding.binding_id),
                branch_id: None,
                event_id: None,
            },
            "not_found",
            axum::http::StatusCode::NOT_FOUND,
        )
        .await;
        assert_deny(
            &svc,
            AdmittedActor::Character {
                character_id,
            },
            ActorViewpoint {
                world_id: "wld_foreign".into(),
                binding_id: Some(binding_id),
                branch_id: None,
                event_id: None,
            },
            "not_found",
            axum::http::StatusCode::NOT_FOUND,
        )
        .await;
    }
}
