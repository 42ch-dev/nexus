//! Reading depth (scroll progress and per-chapter annotations) over the
//! guarded core workspace. HTTP envelopes stay in adapters; these projections
//! are owned domain values. Extracted from the legacy daemon `reading.rs`
//! handlers (V1.89 surface, P1-T3): scope/isolation, pagination-free chapter
//! listing and the explicit nullable note-update semantics are preserved.

use std::num::NonZeroU64;

use nexus_contracts::daemon_api::reading::{
    ReadingAnnotation, ReadingAnnotationCreateRequest, ReadingAnnotationListQuery,
    ReadingAnnotationListResponse, ReadingAnnotationPatchRequest, ReadingProgressQuery,
    ReadingProgressRequest, ReadingProgressResponse,
};
use nexus_local_db::reading::{self, AnnotationRow};
use uuid::Uuid;

use crate::content::wire_cast;
use crate::error::local_db_err;
use crate::{CoreError, CoreResult, CoreService, Principal};

const ANNOTATION_ID_PREFIX: &str = "ann_";
const VALID_COLORS: [&str; 4] = ["yellow", "blue", "green", "pink"];

/// Reading-family fault carrying the legacy classification until the adapter
/// boundary (same carrier scheme as the content family).
#[derive(Debug, thiserror::Error)]
enum ReadingFault {
    #[error("{message}")]
    BadRequest { message: String },
    #[error("{0}")]
    NotFound(String),
    /// Foreign-creator annotation access; the resource carries the
    /// `annotation_owner:` prefix so the adapter can re-emit the legacy
    /// `Forbidden { resource, reason }` pair verbatim.
    #[error("forbidden: {0}")]
    ForeignAnnotation(String),
    #[error(transparent)]
    Core(#[from] CoreError),
}

impl From<ReadingFault> for CoreError {
    fn from(error: ReadingFault) -> Self {
        match error {
            ReadingFault::BadRequest { message } => Self::InvalidInput {
                field: "invalid_input".into(),
                reason: message,
            },
            ReadingFault::NotFound(resource) => Self::NotFound { resource },
            ReadingFault::ForeignAnnotation(resource) => Self::Forbidden { resource },
            ReadingFault::Core(error) => error,
        }
    }
}

/// Map a reading DAO error onto the family fault. Validation rejections ride
/// the legacy `invalid_input` 422; everything else keeps the shared storage
/// carrier (`database_error: …`, re-classified to `DATABASE_ERROR` by the
/// daemon adapter).
fn reading_db_err(error: nexus_local_db::LocalDbError) -> CoreError {
    match error {
        nexus_local_db::LocalDbError::ValidationError(message) => CoreError::InvalidInput {
            field: "invalid_input".into(),
            reason: message,
        },
        other => local_db_err(other),
    }
}

/// Convert a `NonZeroU64` chapter number to i64 for local-db calls.
fn chapter_i64(ch: NonZeroU64) -> i64 {
    i64::try_from(u64::from(ch)).unwrap_or(1)
}

/// Convert a `u64` offset/progress value to `i64`, returning a 422 if it
/// does not fit (defense-in-depth; schema bounds keep values well within range).
fn u64_to_i64(value: u64, field: &str) -> Result<i64, ReadingFault> {
    i64::try_from(value).map_err(|_| ReadingFault::BadRequest {
        message: format!("{field} exceeds maximum supported value"),
    })
}

/// Validate a highlight color against the V1.89 enum.
fn validate_color(color: &str) -> Result<(), ReadingFault> {
    if VALID_COLORS.contains(&color) {
        Ok(())
    } else {
        Err(ReadingFault::BadRequest {
            message: format!(
                "color must be one of {}, got '{color}'",
                VALID_COLORS.join(", ")
            ),
        })
    }
}

/// Map a database annotation row to the contract DTO.
fn to_annotation_dto(row: &AnnotationRow) -> Result<ReadingAnnotation, CoreError> {
    Ok(ReadingAnnotation {
        annotation_id: row.annotation_id.clone(),
        work_id: row.work_id.clone(),
        chapter: NonZeroU64::new(u64::try_from(row.chapter).unwrap_or(1))
            .unwrap_or(NonZeroU64::MIN),
        start_offset: u64::try_from(row.start_offset).unwrap_or_default(),
        end_offset: u64::try_from(row.end_offset).unwrap_or_default(),
        selected_text: wire_cast(row.selected_text.clone())?,
        color: wire_cast(row.color.clone())?,
        note: row.note.clone(),
        created_at: row.created_at.to_rfc3339(),
        updated_at: row.updated_at.to_rfc3339(),
    })
}

/// Fetch an annotation and enforce creator ownership.
async fn load_annotation_for_creator(
    service: &CoreService,
    principal: &Principal,
    annotation_id: &str,
) -> Result<AnnotationRow, ReadingFault> {
    let row = reading::get_annotation(&service.inner.pool, annotation_id)
        .await
        .map_err(reading_db_err)?
        .ok_or_else(|| ReadingFault::NotFound(format!("annotation {annotation_id}")))?;

    if row.creator_id != principal.creator_id() {
        return Err(ReadingFault::ForeignAnnotation(format!(
            "annotation_owner:annotation {annotation_id}"
        )));
    }

    Ok(row)
}

impl CoreService {
    /// Persisted scroll progress for a (work, chapter); zero with a fresh
    /// timestamp when nothing was ever saved.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification, [`CoreError::NotFound`] for an unknown or foreign Work,
    /// and the storage carrier otherwise.
    pub async fn get_reading_progress(
        &self,
        principal: &Principal,
        query: ReadingProgressQuery,
    ) -> CoreResult<ReadingProgressResponse> {
        self.verify_principal(principal)?;
        self.resolve_owned_work(principal, &query.work_id).await?;
        let (scroll_progress, updated_at) = reading::get_reading_progress(
            &self.inner.pool,
            principal.creator_id(),
            &query.work_id,
            chapter_i64(query.chapter),
        )
        .await
        .map_err(reading_db_err)?;
        self.verify_principal(principal)?;
        Ok(ReadingProgressResponse {
            work_id: query.work_id,
            chapter: query.chapter,
            scroll_progress,
            updated_at: updated_at.unwrap_or_else(chrono::Utc::now).to_rfc3339(),
        })
    }

    /// Upsert scroll progress for a (work, chapter).
    ///
    /// `work_id` is the authoritative Work reference; it is repeated inside
    /// `request` only for wire compatibility with the legacy body shape.
    ///
    /// # Errors
    /// As [`CoreService::get_reading_progress`]; additionally
    /// [`CoreError::Forbidden`] under read-only core access and
    /// [`CoreError::InvalidInput`] when the progress value is outside 0–10000.
    pub async fn put_reading_progress(
        &self,
        principal: &Principal,
        work_id: String,
        request: ReadingProgressRequest,
    ) -> CoreResult<ReadingProgressResponse> {
        self.verify_principal(principal)?;
        self.resolve_owned_work(principal, &work_id).await?;
        self.require_work_write()?;
        let updated_at = reading::upsert_reading_progress(
            &self.inner.pool,
            principal.creator_id(),
            &work_id,
            chapter_i64(request.chapter),
            request.scroll_progress,
        )
        .await
        .map_err(reading_db_err)?;
        self.verify_principal(principal)?;
        Ok(ReadingProgressResponse {
            work_id,
            chapter: request.chapter,
            scroll_progress: request.scroll_progress,
            updated_at: updated_at.to_rfc3339(),
        })
    }

    /// Delete persisted scroll progress for a (work, chapter).
    ///
    /// # Errors
    /// As [`CoreService::put_reading_progress`] minus the progress validation.
    pub async fn delete_reading_progress(
        &self,
        principal: &Principal,
        query: ReadingProgressQuery,
    ) -> CoreResult<()> {
        self.verify_principal(principal)?;
        self.resolve_owned_work(principal, &query.work_id).await?;
        self.require_work_write()?;
        reading::delete_reading_progress(
            &self.inner.pool,
            principal.creator_id(),
            &query.work_id,
            chapter_i64(query.chapter),
        )
        .await
        .map_err(reading_db_err)?;
        self.verify_principal(principal)
    }

    /// List annotations for a chapter, oldest first per the DAO ordering.
    ///
    /// # Errors
    /// As [`CoreService::get_reading_progress`].
    pub async fn list_annotations(
        &self,
        principal: &Principal,
        query: ReadingAnnotationListQuery,
    ) -> CoreResult<ReadingAnnotationListResponse> {
        self.verify_principal(principal)?;
        self.resolve_owned_work(principal, &query.work_id).await?;
        let rows = reading::list_annotations(
            &self.inner.pool,
            principal.creator_id(),
            &query.work_id,
            chapter_i64(query.chapter),
        )
        .await
        .map_err(reading_db_err)?;
        let items = rows
            .iter()
            .map(to_annotation_dto)
            .collect::<Result<Vec<_>, CoreError>>()?;
        let items = wire_cast(items)?;
        self.verify_principal(principal)?;
        Ok(ReadingAnnotationListResponse { items })
    }

    /// Create an annotation (highlight) on a chapter.
    ///
    /// # Errors
    /// As [`CoreService::get_reading_progress`]; additionally
    /// [`CoreError::Forbidden`] under read-only core access,
    /// [`CoreError::InvalidInput`] for an off-enum color or oversized offset,
    /// and [`CoreError::InvalidInput`] with the storage validation message
    /// when `end_offset <= start_offset`.
    pub async fn create_annotation(
        &self,
        principal: &Principal,
        request: ReadingAnnotationCreateRequest,
    ) -> CoreResult<ReadingAnnotation> {
        self.verify_principal(principal)?;
        self.resolve_owned_work(principal, &request.work_id).await?;
        self.require_work_write()?;

        validate_color(&request.color.to_string())?;
        let start_offset = u64_to_i64(request.start_offset, "start_offset")?;
        let end_offset = u64_to_i64(request.end_offset, "end_offset")?;
        let annotation_id = format!("{ANNOTATION_ID_PREFIX}{}", Uuid::new_v4().simple());

        let row = reading::create_annotation(
            &self.inner.pool,
            principal.creator_id(),
            &request.work_id,
            chapter_i64(request.chapter),
            &annotation_id,
            start_offset,
            end_offset,
            &request.selected_text.to_string(),
            &request.color.to_string(),
            request.note.as_deref(),
        )
        .await
        .map_err(reading_db_err)?;
        self.verify_principal(principal)?;
        to_annotation_dto(&row)
    }
    /// Patch an annotation's color and/or note.
    ///
    /// The note update is explicitly tri-state: a missing field keeps the
    /// stored note, an empty (or whitespace-only) string clears it, and any
    /// other value replaces it trimmed.
    ///
    /// # Errors
    /// As [`CoreService::create_annotation`]; additionally
    /// [`CoreError::NotFound`] for an unknown annotation and
    /// [`CoreError::Forbidden`] when the annotation belongs to a different
    /// creator.
    pub async fn patch_annotation(
        &self,
        principal: &Principal,
        annotation_id: String,
        request: ReadingAnnotationPatchRequest,
    ) -> CoreResult<ReadingAnnotation> {
        self.verify_principal(principal)?;
        let _ = load_annotation_for_creator(self, principal, &annotation_id).await?;
        self.require_work_write()?;

        if let Some(color) = &request.color {
            validate_color(&color.to_string())?;
        }

        // Translate empty-string note to None (clear note); missing field stays None
        // on the request, which we interpret as "do not change" via `note: None` below.
        let note_change = request.note.as_ref().map(|n| {
            let trimmed = n.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });

        let row = reading::update_annotation(
            &self.inner.pool,
            &annotation_id,
            request
                .color
                .as_ref()
                .map(std::string::ToString::to_string)
                .as_deref(),
            note_change,
        )
        .await
        .map_err(reading_db_err)?
        .ok_or_else(|| ReadingFault::NotFound(format!("annotation {annotation_id}")))?;
        self.verify_principal(principal)?;
        to_annotation_dto(&row)
    }

    /// Delete an annotation.
    ///
    /// # Errors
    /// As [`CoreService::patch_annotation`] minus the validation.
    pub async fn delete_annotation(
        &self,
        principal: &Principal,
        annotation_id: String,
    ) -> CoreResult<()> {
        self.verify_principal(principal)?;
        let _ = load_annotation_for_creator(self, principal, &annotation_id).await?;
        self.require_work_write()?;
        reading::delete_annotation(&self.inner.pool, &annotation_id)
            .await
            .map_err(reading_db_err)?;
        self.verify_principal(principal)
    }
}
