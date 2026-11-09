//! Reflection admission (§6.5, §7.25–§7.27): the `OpTarget` family for
//! `memory.reflections.{state,list,generate}`, its freeze classification and
//! the honest cognition refusal.
//!
//! All three reflection ops require the `reflect` capability covering the
//! request's selection (`reflect` never unions across grants, MEM-R006). The
//! §6.5 facets follow the selection exactly as in recall: `subject_all` needs
//! an unrestricted grant (C-075), `selected_contexts` needs coverage of every
//! listed context, and a shared read needs the shared-access facet; `admin`
//! implies `reflect` inside the published evaluator. The provenance filter is
//! a read-time intersection, never a placement statement (§6.13), so it
//! narrows rows without changing the grant requirement.
//!
//! This slice serves no cognition family: a present `cognition_filter` is
//! refused `INVALID_INPUT` on all three ops — an honest refusal, never a
//! silent widening of the served surface. `state`/`list` are read-only and
//! stay admitted in a frozen space (§8.2 allowlist); `generate` is not
//! allowlisted and refuses `SPACE_FROZEN`. Refusal constructors live in this
//! slice because the frozen slices keep theirs module-private; codes and
//! `details` shapes are the published protocol ones.

use chrono::{DateTime, Utc};
use spoke_operations::spoke_schemas::memory_reflections_generate_request::ReadSelection as GenerateSelection;
use spoke_operations::spoke_schemas::memory_reflections_list_request::ReadSelection as ListSelection;
use spoke_operations::spoke_schemas::memory_reflections_state_request::ReadSelection as StateSelection;
use spoke_operations::spoke_schemas::CognitionFilter;
use spoke_operations::{is_freeze_allowlisted, GrantRequirement, SpokeReject, SpokeRejectCode};
use sqlx::{Row, SqliteConnection};

use crate::provider::FixtureMemoryProvider;
use crate::spaces::{internal_failure, is_authorized, load_grants};

/// The grant facets one reflection request's selection needs (§6.5).
///
/// `state`/`generate` selections are required; `list` may omit its selection
/// to list the whole subject, which is the same whole-subject requirement as
/// `subject_all`.
#[derive(Debug, Clone)]
pub(crate) struct ReflectionTarget {
    subject_all: bool,
    context_ids: Vec<String>,
    reads_shared: bool,
}

impl ReflectionTarget {
    fn new(subject_all: bool, context_ids: &[String], reads_shared: bool) -> Self {
        Self {
            subject_all,
            context_ids: context_ids.to_vec(),
            reads_shared,
        }
    }

    pub(crate) fn from_state(selection: &StateSelection) -> Self {
        match selection {
            StateSelection::SubjectAll => Self::new(true, &[], true),
            StateSelection::SharedOnly => Self::new(false, &[], true),
            StateSelection::SelectedContexts {
                context_ids,
                include_shared,
            } => Self::new(false, context_ids, include_shared.unwrap_or(false)),
        }
    }

    pub(crate) fn from_list(selection: Option<&ListSelection>) -> Self {
        match selection {
            None | Some(ListSelection::SubjectAll) => Self::new(true, &[], true),
            Some(ListSelection::SharedOnly) => Self::new(false, &[], true),
            Some(ListSelection::SelectedContexts {
                context_ids,
                include_shared,
            }) => Self::new(false, context_ids, include_shared.unwrap_or(false)),
        }
    }

    pub(crate) fn from_generate(selection: &GenerateSelection) -> Self {
        match selection {
            GenerateSelection::SubjectAll => Self::new(true, &[], true),
            GenerateSelection::SharedOnly => Self::new(false, &[], true),
            GenerateSelection::SelectedContexts {
                context_ids,
                include_shared,
            } => Self::new(false, context_ids, include_shared.unwrap_or(false)),
        }
    }

    /// The §6.5 requirement one grant must cover: `reflect` over this
    /// selection's facets.
    fn requirement<'a>(&'a self, subject_id: &'a str) -> GrantRequirement<'a> {
        GrantRequirement {
            capability: "reflect",
            subject_id: Some(subject_id),
            context_ids: self.context_ids.clone(),
            reads_shared: self.reads_shared,
            subject_all: self.subject_all,
        }
    }
}

// ---------------------------------------------------------------------------
// Protocol errors
// ---------------------------------------------------------------------------

/// Same `INVALID_INPUT` envelope the published validators emit: the offending
/// request field in `details.field`.
fn invalid_input(field: &str, message: impl Into<String>) -> SpokeReject {
    let mut details = serde_json::Map::new();
    details.insert(
        "field".to_owned(),
        serde_json::Value::String(field.to_owned()),
    );
    SpokeReject {
        code: SpokeRejectCode::InvalidInput,
        message: message.into(),
        details: Some(details),
    }
}

fn not_authorized(message: impl Into<String>) -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::NotAuthorized,
        message: message.into(),
        details: None,
    }
}

fn space_frozen(space_id: &str, op: &str) -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::SpaceFrozen,
        message: format!("space {space_id} is frozen; {op} is outside the §8.2 allowlist"),
        details: None,
    }
}

fn not_found(message: impl Into<String>) -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::NotFound,
        message: message.into(),
        details: None,
    }
}

/// This fixture serves no cognition family, so a present `cognition_filter`
/// is refused on every reflection op before any storage is touched.
fn cognition_filter_unsupported() -> SpokeReject {
    invalid_input(
        "cognition_filter",
        "this provider does not serve cognition-filtered reflections",
    )
}

async fn space_status(conn: &mut SqliteConnection, space_id: &str) -> Result<String, SpokeReject> {
    let row = sqlx::query("SELECT status FROM spaces WHERE space_id = ?")
        .bind(space_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(internal_failure)?
        .ok_or_else(|| not_found(format!("space {space_id} is not served by this fixture")))?;
    row.try_get("status").map_err(internal_failure)
}

// ---------------------------------------------------------------------------
// Admission
// ---------------------------------------------------------------------------

impl FixtureMemoryProvider {
    /// One admission decision for a reflection op: cognition refusal, served
    /// space, §8.2 freeze classification, then §6.5 grant coverage — the
    /// whole decision on the caller's single snapshot. `state`/`list` pass
    /// `is_freeze_allowlisted`; `generate` refuses `SPACE_FROZEN` when frozen.
    /// The admission decision's inputs arrive per-op from the serving slices;
    /// the count is the protocol surface, not a design smell.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn require_reflection_admitted(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        op: &str,
        space_id: &str,
        subject_id: &str,
        target: &ReflectionTarget,
        cognition_filter: Option<&CognitionFilter>,
        now: DateTime<Utc>,
    ) -> Result<(), SpokeReject> {
        if cognition_filter.is_some() {
            return Err(cognition_filter_unsupported());
        }
        if space_id != self.identity.space_id {
            return Err(not_found(format!(
                "space {space_id} is not served by this fixture"
            )));
        }
        let status = space_status(conn, space_id).await?;
        if status == "frozen" && !is_freeze_allowlisted(op) {
            return Err(space_frozen(space_id, op));
        }
        // §6.5/MEM-R006: one current grant must cover the whole target.
        let grants = load_grants(conn, space_id).await?;
        if !is_authorized(
            &grants,
            principal_id,
            space_id,
            now,
            &target.requirement(subject_id),
        ) {
            return Err(not_authorized(
                "the authenticated principal holds no grant covering this reflection selection",
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Admission-rule tests (the §6.5 facet matrix and the honest refusals; the
// serving selectors that drive these rules over loopback land in Task 6)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use spoke_operations::spoke_schemas::MemoryGrant;
    use spoke_operations::SpokeRejectCode;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    fn grant_from(value: serde_json::Value) -> MemoryGrant {
        serde_json::from_value(value).expect("test grant deserializes")
    }

    fn restricted(capabilities: &[&str], context_ids: &[&str], shared_access: bool) -> MemoryGrant {
        grant_from(json!({
            "grant_id": "g-1",
            "principal_id": "client",
            "space_id": "space-one",
            "subject_id": null,
            "capabilities": capabilities,
            "context_ids": context_ids,
            "shared_access": shared_access,
            "created_at": "2026-01-01T00:00:00Z",
            "revision": 1,
            "schema_version": 1,
            "extensions": {},
        }))
    }

    fn unrestricted(capabilities: &[&str]) -> MemoryGrant {
        grant_from(json!({
            "grant_id": "g-2",
            "principal_id": "client",
            "space_id": "space-one",
            "subject_id": null,
            "capabilities": capabilities,
            "created_at": "2026-01-01T00:00:00Z",
            "revision": 1,
            "schema_version": 1,
            "extensions": {},
        }))
    }

    fn admits(grants: &[MemoryGrant], target: &ReflectionTarget) -> bool {
        is_authorized(
            grants,
            "client",
            "space-one",
            now(),
            &target.requirement("subject-one"),
        )
    }

    fn selection_all() -> ReflectionTarget {
        ReflectionTarget::from_state(&StateSelection::SubjectAll)
    }

    fn selection_contexts(contexts: &[&str], include_shared: bool) -> ReflectionTarget {
        ReflectionTarget::from_state(&StateSelection::SelectedContexts {
            context_ids: contexts.iter().map(|c| (*c).to_owned()).collect(),
            include_shared: Some(include_shared),
        })
    }

    fn selection_shared_only() -> ReflectionTarget {
        ReflectionTarget::from_state(&StateSelection::SharedOnly)
    }

    /// C-075: `subject_all` needs an unrestricted grant; a context-limited or
    /// shared-only grant never covers it.
    #[test]
    fn c_075_subject_all_requires_unrestricted_grant() {
        assert!(admits(&[unrestricted(&["reflect"])], &selection_all()));
        // `admin` implies `reflect` (§6.5).
        assert!(admits(&[unrestricted(&["admin"])], &selection_all()));
        // An empty `context_ids` restriction is no restriction (the published
        // evaluator reads it as full coverage), so the negative cases here are
        // context-limited grants.
        for narrow in [
            restricted(&["reflect"], &["ctx-a"], false),
            restricted(&["reflect"], &["ctx-a"], true),
        ] {
            assert!(
                !admits(&[narrow], &selection_all()),
                "a restricted grant must not cover subject_all"
            );
        }
    }

    /// `selected_contexts` needs coverage of every listed context; a shared
    /// read needs the shared-access facet.
    #[test]
    fn selected_contexts_need_full_coverage_and_shared_reads_need_shared_access() {
        let both = selection_contexts(&["ctx-a", "ctx-b"], false);
        assert!(admits(
            &[restricted(&["reflect"], &["ctx-a", "ctx-b"], false)],
            &both
        ));
        assert!(!admits(
            &[restricted(&["reflect"], &["ctx-a"], false)],
            &both
        ));

        let with_shared = selection_contexts(&["ctx-a"], true);
        assert!(admits(
            &[restricted(&["reflect"], &["ctx-a"], true)],
            &with_shared
        ));
        assert!(!admits(
            &[restricted(&["reflect"], &["ctx-a"], false)],
            &with_shared
        ));

        assert!(admits(
            &[restricted(&["reflect"], &[], true)],
            &selection_shared_only()
        ));
        assert!(!admits(
            &[restricted(&["reflect"], &["ctx-a"], false)],
            &selection_shared_only()
        ));
    }

    /// The list slice's absent selection is the whole-subject requirement.
    #[test]
    fn absent_list_selection_is_subject_all() {
        assert!(admits(
            &[unrestricted(&["reflect"])],
            &ReflectionTarget::from_list(None)
        ));
        assert!(!admits(
            &[restricted(&["reflect"], &["ctx-a"], true)],
            &ReflectionTarget::from_list(None)
        ));
    }

    /// A present `cognition_filter` is an honest `INVALID_INPUT` refusal.
    #[test]
    fn cognition_filter_is_refused_invalid_input() {
        let refusal = cognition_filter_unsupported();
        assert_eq!(refusal.code, SpokeRejectCode::InvalidInput);
        assert_eq!(
            refusal
                .details
                .as_ref()
                .and_then(|d| d.get("field"))
                .and_then(serde_json::Value::as_str),
            Some("cognition_filter")
        );
    }

    /// §8.2 classification: `state`/`list` are allowlisted reads; `generate`
    /// is not, so only generate refuses `SPACE_FROZEN`.
    #[test]
    fn freeze_classification_admits_reads_only() {
        assert!(is_freeze_allowlisted("memory.reflections.state"));
        assert!(is_freeze_allowlisted("memory.reflections.list"));
        assert!(!is_freeze_allowlisted("memory.reflections.generate"));
    }
}
