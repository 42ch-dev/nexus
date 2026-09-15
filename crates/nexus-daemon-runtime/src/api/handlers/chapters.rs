//! Thin chapter-content HTTP adapters over the guarded core authoring service.
//!
//! Endpoints under `/v1/daemon/works/{work_id}/chapters/*` translate the wire
//! envelope to `nexus_core` chapter-content operations and back; protection
//! rules, per-Work locks, path guards and storage semantics live in the core.

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, Query, State};
use axum::Json;
use nexus_contracts::{
    ChapterBody, ChapterContentQuery, ChapterDetail, ChapterOutline, ListChaptersQuery,
    ListChaptersResponse, PatchChapterRequest,
};

/// Map a core content/outline fault onto the legacy HTTP classification.
///
/// `BadRequest` codes and `NotFound` resources round-trip verbatim; the
/// legacy internal codes (DATABASE_ERROR, CONTRACT_ERROR, FILE_READ_ERROR,
/// DIRECTORY_CREATE_ERROR, OUTLINE_WRITE_ERROR, OUTLINE_SERIALIZE_ERROR,
/// OUTLINE_REVISION_NEGATIVE, WORK_REF_MISSING, PATH_GUARD_PANIC) are carried
/// verbatim as `<CODE>: <message>` and re-emitted with the original code;
/// every other internal category keeps the shared `CORE_ERROR` shape.
pub(crate) fn content_error(error: nexus_core::CoreError) -> NexusApiError {
    match error {
        nexus_core::CoreError::InvalidInput { field, reason } => {
            NexusApiError::BadRequest { code: field, message: reason }
        }
        nexus_core::CoreError::NotFound { resource } => NexusApiError::NotFound(resource),
        nexus_core::CoreError::Forbidden { resource } if resource.starts_with("work_locked:") => {
            NexusApiError::Locked { resource: "work".into(), reason: resource[12..].to_owned() }
        }
        nexus_core::CoreError::OutlineConflict(details) => NexusApiError::OutlineConflict {
            current_revision: details.current_revision,
            node_id: details.node_id,
            conflicting_path: details.conflicting_path,
            recovery_hint: details.recovery_hint,
        },
        nexus_core::CoreError::OutlineValidation(summary) => {
            NexusApiError::outline_validation_failed(&summary.errors, &summary.warnings)
        }
        nexus_core::CoreError::Internal { category } => match category.split_once(": ") {
            Some((code, message))
                if matches!(
                    code,
                    "DATABASE_ERROR"
                        | "CONTRACT_ERROR"
                        | "FILE_READ_ERROR"
                        | "DIRECTORY_CREATE_ERROR"
                        | "OUTLINE_WRITE_ERROR"
                        | "OUTLINE_SERIALIZE_ERROR"
                        | "OUTLINE_REVISION_NEGATIVE"
                        | "WORK_REF_MISSING"
                        | "PATH_GUARD_PANIC"
                ) =>
            {
                NexusApiError::Internal { code: code.to_owned(), message: message.to_owned() }
            }
            _ => nexus_core::CoreError::Internal { category }.into(),
        },
        other => other.into(),
    }
}

/// `GET /v1/daemon/works/{work_id}/chapters` — cursor-paginated chapter summaries.
pub async fn list_chapters(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
    Query(query): Query<ListChaptersQuery>,
) -> Result<Json<ListChaptersResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let response = core.list_chapters(&principal, work_id, query).await.map_err(content_error)?;
    Ok(Json(response))
}

/// `GET /v1/daemon/works/{work_id}/chapters/{n}` — chapter detail.
pub async fn get_chapter(
    State(state): State<WorkspaceState>,
    Path((work_id, n)): Path<(String, String)>,
    Query(query): Query<ChapterContentQuery>,
) -> Result<Json<ChapterDetail>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let detail = core
        .chapter_detail(&principal, work_id, n, nexus_core::CoreChapterContentQuery::from(query))
        .await
        .map_err(content_error)?;
    Ok(Json(detail))
}

/// `GET /v1/daemon/works/{work_id}/chapters/{n}/outline` — read outline markdown.
pub async fn get_chapter_outline(
    State(state): State<WorkspaceState>,
    Path((work_id, n)): Path<(String, String)>,
    Query(query): Query<ChapterContentQuery>,
) -> Result<Json<ChapterOutline>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let outline = core
        .chapter_outline(&principal, work_id, n, nexus_core::CoreChapterContentQuery::from(query))
        .await
        .map_err(content_error)?;
    Ok(Json(outline))
}

/// `PATCH /v1/daemon/works/{work_id}/chapters/{n}` — partial structure update.
pub async fn patch_chapter(
    State(state): State<WorkspaceState>,
    Path((work_id, n)): Path<(String, String)>,
    Query(query): Query<ChapterContentQuery>,
    Json(req): Json<PatchChapterRequest>,
) -> Result<Json<ChapterDetail>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let detail = core
        .patch_chapter(
            &principal,
            work_id,
            n,
            nexus_core::CoreChapterContentQuery::from(query),
            req,
        )
        .await
        .map_err(content_error)?;
    Ok(Json(detail))
}

/// `GET /v1/daemon/works/{work_id}/chapters/{n}/body` — read body markdown (read-only).
pub async fn get_chapter_body(
    State(state): State<WorkspaceState>,
    Path((work_id, n)): Path<(String, String)>,
    Query(query): Query<ChapterContentQuery>,
) -> Result<Json<ChapterBody>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let body = core
        .chapter_body(&principal, work_id, n, nexus_core::CoreChapterContentQuery::from(query))
        .await
        .map_err(content_error)?;
    Ok(Json(body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    /// Adapter parity: the legacy error classification (400 codes, 404
    /// resources, 409 outline conflict, 422 validation, 423 locked and the
    /// verbatim legacy internal codes) survives the core carrier unchanged.
    #[test]
    fn content_error_retains_status_and_body() {
        let cases = [
            (
                nexus_core::CoreError::InvalidInput {
                    field: "chapter_title_unsupported".into(),
                    reason: "title is display-only in V1.65; use outline frontmatter or slug instead".into(),
                },
                NexusApiError::BadRequest {
                    code: "chapter_title_unsupported".into(),
                    message: "title is display-only in V1.65; use outline frontmatter or slug instead".into(),
                },
                StatusCode::BAD_REQUEST,
            ),
            (
                nexus_core::CoreError::NotFound { resource: "chapter 1 volume 1".into() },
                NexusApiError::NotFound("chapter 1 volume 1".into()),
                StatusCode::NOT_FOUND,
            ),
            (
                nexus_core::CoreError::Forbidden {
                    resource: "work_locked:work wrk_x is locked by 'driver'; wait for release or check 'creator works status'".into(),
                },
                NexusApiError::Locked {
                    resource: "work".into(),
                    reason: "work wrk_x is locked by 'driver'; wait for release or check 'creator works status'".into(),
                },
                StatusCode::LOCKED,
            ),
            (
                nexus_core::CoreError::outline_conflict(5, "3", "outline_revision", "refetch the work outline and reapply"),
                NexusApiError::outline_conflict(5, "3", "outline_revision", "refetch the work outline and reapply"),
                StatusCode::CONFLICT,
            ),
        ];
        for (core_error, old_error, status) in cases {
            let migrated = content_error(core_error);
            assert_eq!(migrated.status_code(), status);
            assert_eq!(
                serde_json::to_string(&migrated.to_response_body()).unwrap(),
                serde_json::to_string(&old_error.to_response_body()).unwrap(),
                "legacy error body must round-trip verbatim"
            );
        }

        let validation =
            content_error(nexus_core::CoreError::outline_validation_failed(
                &["slug 'X' must be kebab-case".into()],
                &[],
            ));
        assert_eq!(validation.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(validation.error_code(), "outline_validation_failed");

        for category in [
            "DATABASE_ERROR: locked",
            "CONTRACT_ERROR: shape",
            "FILE_READ_ERROR: gone",
            "DIRECTORY_CREATE_ERROR: no dir",
            "OUTLINE_WRITE_ERROR: disk",
            "OUTLINE_SERIALIZE_ERROR: yaml",
            "OUTLINE_REVISION_NEGATIVE: bug",
            "WORK_REF_MISSING: no ref",
            "PATH_GUARD_PANIC: panic",
        ] {
            let migrated =
                content_error(nexus_core::CoreError::Internal { category: category.into() });
            let NexusApiError::Internal { code, message } = &migrated else {
                panic!("internal category must stay Internal, got {migrated:?}");
            };
            let (expected_code, expected_message) = category.split_once(": ").unwrap();
            assert_eq!(code, expected_code);
            assert_eq!(message, expected_message);
        }

        // Non-legacy internal categories keep the shared CORE_ERROR fallback.
        let fallback =
            content_error(nexus_core::CoreError::Internal { category: "workspace metadata: boom".into() });
        let NexusApiError::Internal { code, .. } = &fallback else {
            panic!("fallback must stay Internal");
        };
        assert_eq!(code, "CORE_ERROR");
    }
}
