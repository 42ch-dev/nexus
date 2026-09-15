//! Reading, findings and knowledge services on guarded storage (AC-P1-T3).
//!
//! Ports the named legacy daemon behaviors (`reading_api.rs` annotation round
//! trip and the null-note boundary) onto the core service and protects the
//! scope/isolation, cursor-pagination and explicit nullable-update invariants
//! through the extraction.
use nexus_contracts::daemon_api::kb::ListKbEntriesQuery;
use nexus_contracts::daemon_api::reading::{
    ReadingAnnotationCreateRequest, ReadingAnnotationListQuery, ReadingAnnotationPatchRequest,
    ReadingProgressQuery, ReadingProgressRequest,
};
use nexus_contracts::{
    BatchUpdateFindingsRequest, CreateWorkRequest, CreateWorldRequest,
};
use nexus_core::{
    CoreAccess, CoreError, CoreOpenOptions, CoreService,
    Principal, UpdateFindingRequest,
};

fn select_creator(home: &std::path::Path, creator: &str, workspace: &str) {
    std::fs::write(home.join(".nexus42/config.toml"), format!("active_creator_id = \"{creator}\"\n[active_workspace_slug_by_creator]\n\"{creator}\" = \"{workspace}\"\n")).unwrap();
}

async fn service(home: &std::path::Path) -> CoreService {
    CoreService::open(CoreOpenOptions { user_home: home.into(), access: CoreAccess::DirectWriter })
        .await
        .unwrap()
}

async fn core_with_work(home: &std::path::Path) -> (CoreService, Principal, String) {
    std::fs::create_dir_all(home.join(".nexus42")).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(home, "author", "default")).unwrap();
    select_creator(home, "author", "default");
    {
        let db = nexus_home_layout::workspace_state_db_path(home, "author", "default");
        let seed = nexus_local_db::writer_protocol::init_guarded_pool(&db, "author").await.unwrap();
        let pool = seed.clone_pool();
        nexus_local_db::creators::ensure_creator_row(&pool, "author", "Author").await.unwrap();
        pool.close().await;
    }
    let core = service(home).await;
    let principal = core.active_principal().await.unwrap();
    let world = core
        .create_world(&principal, serde_json::from_value::<CreateWorldRequest>(serde_json::json!({"title": "World"})).unwrap())
        .await
        .unwrap()
        .world_id;
    let work_id = core
        .create_work(&principal, serde_json::from_value::<CreateWorkRequest>(serde_json::json!({
            "title": "Test Novel", "long_term_goal": "Write", "initial_idea": "Idea", "world_id": world
        })).unwrap())
        .await
        .unwrap()
        .work_id;
    (core, principal, work_id)
}

fn annotation_request(work_id: &str) -> ReadingAnnotationCreateRequest {
    serde_json::from_value(serde_json::json!({
        "work_id": work_id,
        "chapter": 1,
        "start_offset": 10,
        "end_offset": 20,
        "selected_text": "highlighted",
        "color": "yellow",
        "note": "note"
    }))
    .unwrap()
}

/// Port of the legacy daemon `reading_api.rs` `annotation_create_list_patch_delete_round_trip`
/// (create at :195, null-note boundary exercised separately below): create,
/// list, patch and delete one annotation through the core service.
#[tokio::test]
async fn annotation_create_list_patch_delete_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    let (core, principal, work_id) = core_with_work(temp.path()).await;

    let created = core.create_annotation(&principal, annotation_request(&work_id)).await.unwrap();
    assert_eq!(created.start_offset, 10);
    assert_eq!(created.end_offset, 20);
    assert_eq!(created.color.to_string(), "yellow");
    assert_eq!(created.note, Some("note".to_string()));

    let list = core
        .list_annotations(&principal, ReadingAnnotationListQuery { work_id: work_id.clone(), chapter: 1.try_into().unwrap() })
        .await
        .unwrap();
    assert_eq!(list.items.len(), 1);
    assert_eq!(list.items[0].annotation_id, created.annotation_id);

    let patch: ReadingAnnotationPatchRequest =
        serde_json::from_value(serde_json::json!({"color": "blue"})).unwrap();
    let updated = core.patch_annotation(&principal, created.annotation_id.clone(), patch).await.unwrap();
    assert_eq!(updated.color.to_string(), "blue");
    assert_eq!(updated.note, Some("note".to_string()));

    core.delete_annotation(&principal, created.annotation_id.clone()).await.unwrap();

    let list = core
        .list_annotations(&principal, ReadingAnnotationListQuery { work_id: work_id.clone(), chapter: 1.try_into().unwrap() })
        .await
        .unwrap();
    assert!(list.items.is_empty());

    // Unknown annotation keeps the legacy not-found resource string.
    assert!(matches!(
        core.delete_annotation(&principal, "ann_unknown".into()).await,
        Err(CoreError::NotFound { resource }) if resource == "annotation ann_unknown"
    ));
    core.close().await.unwrap();
}

/// Port of the legacy `reading_api.rs:261` null-note boundary: an empty note
/// string clears the stored note, an absent field keeps it, and the scope
/// isolation (unknown Work → NotFound, foreign-creator row → Forbidden)
/// survives the extraction.
#[tokio::test]
async fn annotation_note_clear_and_scope_isolation() {
    let temp = tempfile::tempdir().unwrap();
    let (core, principal, work_id) = core_with_work(temp.path()).await;

    // Progress upsert/get/delete over the same Work proves the progress half.
    core.put_reading_progress(&principal, work_id.clone(), ReadingProgressRequest {
        chapter: 1.try_into().unwrap(), scroll_progress: 7500, work_id: work_id.clone(),
    }).await.unwrap();
    let progress = core.get_reading_progress(&principal, ReadingProgressQuery {
        work_id: work_id.clone(), chapter: 1.try_into().unwrap(),
    }).await.unwrap();
    assert_eq!(progress.scroll_progress, 7500);

    let created = core.create_annotation(&principal, annotation_request(&work_id)).await.unwrap();
    let clear: ReadingAnnotationPatchRequest =
        serde_json::from_value(serde_json::json!({"note": ""})).unwrap();
    let cleared = core.patch_annotation(&principal, created.annotation_id.clone(), clear).await.unwrap();
    assert_eq!(cleared.note, None);

    // Unknown Work stays NotFound with the legacy resource string.
    assert!(matches!(
        core.get_reading_progress(&principal, ReadingProgressQuery {
            work_id: "wrk_unknown".into(), chapter: 1.try_into().unwrap()
        }).await,
        Err(CoreError::NotFound { resource }) if resource == "work wrk_unknown"
    ));

    // A foreign-creator annotation row is Forbidden, not silently editable.
    sqlx::query("UPDATE reading_annotations SET creator_id = 'other_creator' WHERE annotation_id = ?1")
        .bind(&created.annotation_id)
        .execute(core.pool())
        .await
        .unwrap();
    let patch: ReadingAnnotationPatchRequest =
        serde_json::from_value(serde_json::json!({"color": "blue"})).unwrap();
    assert!(matches!(
        core.patch_annotation(&principal, created.annotation_id.clone(), patch).await,
        Err(CoreError::Forbidden { resource })
            if resource.starts_with("annotation_owner:annotation ")
    ));
    core.close().await.unwrap();
}

/// Findings cursor pagination (`v1:` opaque offset grammar, `limit + 1`
/// has_more detection) and the legacy invalid-enum filter classification
/// survive the extraction.
#[tokio::test]
async fn findings_list_pagination_and_filter_parity() {
    let temp = tempfile::tempdir().unwrap();
    let (core, principal, work_id) = core_with_work(temp.path()).await;

    for title in ["one", "two", "three"] {
        core.create_finding(&principal, work_id.clone(), nexus_core::CreateFindingRequest {
            chapter: None,
            severity: "minor".into(),
            title: title.into(),
            description: String::new(),
            target_executor: "none".into(),
            kind: "craft".into(),
            rule_suggestion: None,
        }).await.unwrap();
    }

    let page = core.list_findings(&principal, work_id.clone(), nexus_core::ListFindingsQuery {
        chapter: None, status: None, severity: None, limit: Some(2), cursor: None,
    }).await.unwrap();
    assert_eq!(page.items.len(), 2);
    assert!(page.pagination.has_more);
    let cursor = page.pagination.next_cursor.expect("second page cursor");

    let second = core.list_findings(&principal, work_id.clone(), nexus_core::ListFindingsQuery {
        chapter: None, status: None, severity: None, limit: Some(2), cursor: Some(cursor.clone()),
    }).await.unwrap();
    assert_eq!(second.items.len(), 1);
    assert!(!second.pagination.has_more);

    // Status-filter parity (R-V149P0-01): a lone unknown status matches no
    // rows (legacy quirk — the SQL `status = ?` filter simply never hits),
    // while the comma-separated set form validates each token and rejects
    // unknown ones with the verbatim legacy invalid_input message.
    let lone = core.list_findings(&principal, work_id.clone(), nexus_core::ListFindingsQuery {
        chapter: None, status: Some("bogus".into()), severity: None, limit: None, cursor: None,
    }).await.unwrap();
    assert!(lone.items.is_empty());
    let Err(CoreError::InvalidInput { field, reason }) = core.list_findings(
        &principal, work_id.clone(),
        nexus_core::ListFindingsQuery {
            chapter: None, status: Some("open,bogus".into()), severity: None, limit: None, cursor: None,
        },
    ).await else {
        panic!("unknown set token must be rejected");
    };
    assert_eq!((field.as_str(), reason.as_str()), (
        "invalid_input",
        "invalid status value 'bogus'; allowed: open, resolved, wont_fix, triaged, in_review, duplicate"
    ));

    // The `v1:` cursor grammar rejects malformed tokens verbatim.
    let Err(CoreError::InvalidInput { reason, .. }) = core.list_findings(
        &principal, work_id,
        nexus_core::ListFindingsQuery {
            chapter: None, status: None, severity: None, limit: None, cursor: Some("nope".into()),
        },
    ).await else {
        panic!("malformed cursor must be rejected");
    };
    assert!(reason.starts_with("invalid pagination cursor"));
    core.close().await.unwrap();
}

/// Batch triage semantics: empty/duplicate/cap guards, the absent-patch
/// `updated: 0` contract, partial-success buckets (not_found / conflict) and
/// the lifecycle transition rules on the single PATCH path.
#[tokio::test]
async fn findings_update_and_batch_triage_semantics() {
    let temp = tempfile::tempdir().unwrap();
    let (core, principal, work_id) = core_with_work(temp.path()).await;

    let make = |title: &str| nexus_core::CreateFindingRequest {
        chapter: None,
        severity: "minor".into(),
        title: title.into(),
        description: String::new(),
        target_executor: "none".into(),
        kind: "craft".into(),
        rule_suggestion: None,
    };
    let first = core.create_finding(&principal, work_id.clone(), make("one")).await.unwrap();
    let second = core.create_finding(&principal, work_id.clone(), make("two")).await.unwrap();

    // Empty ID list.
    let Err(CoreError::InvalidInput { field, reason }) = core.batch_update_findings(
        &principal, BatchUpdateFindingsRequest { finding_ids: vec![], patch: Default::default() },
    ).await else {
        panic!("empty batch must be rejected");
    };
    assert_eq!((field.as_str(), reason.as_str()), ("invalid_input", "finding_ids must not be empty"));

    // Duplicates.
    let Err(CoreError::InvalidInput { reason, .. }) = core.batch_update_findings(
        &principal,
        BatchUpdateFindingsRequest {
            finding_ids: vec![first.finding_id.clone(), first.finding_id.clone()],
            patch: Default::default(),
        },
    ).await else {
        panic!("duplicate batch ids must be rejected");
    };
    assert_eq!(reason, "finding_ids must not contain duplicates");

    // Cap.
    let ids = (0..101).map(|i| format!("fnd_{i}")).collect();
    let Err(CoreError::InvalidInput { field, reason }) = core.batch_update_findings(
        &principal, BatchUpdateFindingsRequest { finding_ids: ids, patch: Default::default() },
    ).await else {
        panic!("oversized batch must be rejected");
    };
    assert_eq!((field.as_str(), reason.as_str()), ("too_many_findings", "batch update is capped at 100 findings; received 101"));

    // Absent patch reports updated: 0 per the contract.
    let no_patch = core.batch_update_findings(
        &principal,
        BatchUpdateFindingsRequest { finding_ids: vec![first.finding_id.clone()], patch: Default::default() },
    ).await.unwrap();
    assert_eq!(no_patch.updated, 0);

    // Partial success: one updated, one unknown; a terminal-state finding is
    // reported in the conflict bucket instead of failing the batch.
    core.update_finding(&principal, second.finding_id.clone(), UpdateFindingRequest {
        status: Some("resolved".into()), ..Default::default()
    }).await.unwrap();
    core.update_finding(&principal, second.finding_id.clone(), UpdateFindingRequest {
        status: Some("triaged".into()), ..Default::default()
    }).await.unwrap_err(); // resolved is terminal — rejected as invalid_transition
    let report = core.batch_update_findings(&principal, BatchUpdateFindingsRequest {
        finding_ids: vec![
            first.finding_id.clone(),
            second.finding_id.clone(),
            "fnd_unknown".into(),
        ],
        patch: nexus_contracts::NexusFindingBatchPatch {
            status: Some("triaged".into()),
            target_executor: None,
        },
    }).await.unwrap();
    assert_eq!(report.updated, 1);
    assert_eq!(report.not_found, vec!["fnd_unknown".to_string()]);
    assert_eq!(report.conflict, vec![second.finding_id.clone()]);

    // Tri-state rule_suggestion: null clears, absent keeps, value sets (R-V147P0-03).
    let set = core.update_finding(&principal, first.finding_id.clone(), UpdateFindingRequest {
        rule_suggestion: Some(Some("prefer active voice".into())), ..Default::default()
    }).await.unwrap();
    assert_eq!(set.rule_suggestion.as_deref(), Some("prefer active voice"));
    let clear = core.update_finding(&principal, first.finding_id.clone(), UpdateFindingRequest {
        rule_suggestion: Some(None), ..Default::default()
    }).await.unwrap();
    assert_eq!(clear.rule_suggestion, None);

    core.delete_finding(&principal, first.finding_id.clone()).await.unwrap();
    assert!(matches!(
        core.delete_finding(&principal, first.finding_id.clone()).await,
        Err(CoreError::NotFound { .. })
    ));
    core.close().await.unwrap();
}

/// Work-scope KB honesty (entity-scope-model §5.3) and creator isolation:
/// non-work scope and foreign creators are rejected, and the add → get →
/// list → delete round trip works against the temp nexus root.
#[tokio::test]
async fn kb_scope_isolation_and_entry_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    let (core, principal, _work_id) = core_with_work(temp.path()).await;

    // Non-work scope is rejected citing entity-scope-model §5.3.
    let Err(CoreError::InvalidInput { field, reason }) = core.list_kb_entries(&principal, ListKbEntriesQuery {
        creator_id: Some("author".into()), workspace_slug: None, scope: Some("world".into()),
        q: None, limit: None, cursor: None,
    }).await else {
        panic!("non-work scope must be rejected");
    };
    assert_eq!(field, "scope");
    assert!(reason.contains("entity-scope-model"), "scope error must cite entity-scope-model, got: {reason}");
    assert!(reason.contains("work-scope file index only"), "scope error must explain work-scope only, got: {reason}");

    // Missing creator_id and non-segment slugs keep the legacy 400 mapping.
    assert!(matches!(
        core.list_kb_entries(&principal, ListKbEntriesQuery {
            creator_id: None, workspace_slug: None, scope: None, q: None, limit: None, cursor: None,
        }).await,
        Err(CoreError::InvalidInput { ref field, ref reason })
            if field == "creator_id" && reason == "creator_id is required"
    ));
    for slug in ["../escape", "a\\b", "..", "/abs", ".", ""] {
        let Err(CoreError::InvalidInput { field, .. }) = core.list_kb_entries(&principal, ListKbEntriesQuery {
            creator_id: Some("author".into()), workspace_slug: Some(slug.into()), scope: None,
            q: None, limit: None, cursor: None,
        }).await else {
            panic!("slug {slug:?} must be rejected");
        };
        assert_eq!(field, "workspace_slug");
    }

    // A format-valid foreign creator is Forbidden under the core authority.
    let request = nexus_contracts::daemon_api::kb::AddKbEntryRequest {
        content: Some("hello".into()), creator_id: "someone_else".into(), file_path: None,
        scope: None, title: None, workspace_slug: None,
    };
    assert!(matches!(
        core.add_kb_entry(&principal, request).await,
        Err(CoreError::Forbidden { .. })
    ));

    // Round trip: add (inline content) → get → list (q filter) → delete.
    let added = core.add_kb_entry(&principal, nexus_contracts::daemon_api::kb::AddKbEntryRequest {
        content: Some("# note body".into()), creator_id: "author".into(), file_path: None,
        scope: None, title: Some("Note".into()), workspace_slug: None,
    }).await.unwrap();
    assert_eq!(added.title, "Note");

    let got = core.get_kb_entry(&principal, added.entry_id.clone()).await.unwrap();
    assert_eq!(got.content, "# note body");
    assert_eq!(got.title, "Note");

    let list = core.list_kb_entries(&principal, ListKbEntriesQuery {
        creator_id: Some("author".into()), workspace_slug: None, scope: None,
        q: Some("note".into()), limit: None, cursor: None,
    }).await.unwrap();
    assert_eq!(list.items.len(), 1);

    let deleted = core.delete_kb_entry(&principal, added.entry_id.clone()).await.unwrap();
    assert!(deleted.deleted);
    assert!(matches!(
        core.get_kb_entry(&principal, added.entry_id.clone()).await,
        Err(CoreError::NotFound { resource }) if resource.contains(&added.entry_id)
    ));

    // Traversal entry IDs are rejected before any filesystem operation.
    let Err(CoreError::InvalidInput { field, .. }) =
        core.get_kb_entry(&principal, "kb_../../etc/passwd".into()).await
    else {
        panic!("traversal entry id must be rejected");
    };
    assert_eq!(field, "entry_id");
    core.close().await.unwrap();
}
