//! Reference source authority over guarded storage (item 5,
//! R-V1190-REF-REGISTER-GAP).
//!
//! Pins the core `register_reference` producer and the schema-owned
//! `ReferenceGetResponse` fields the `reference show` CLI leaf renders
//! (`workspace_id`/`updated_at`/`tags`/`content_hash`/`refresh_policy`) — the
//! fields whose absence kept the CLI surfaces on the local registry.
use nexus_core::{
    CoreAccess, CoreError, CoreOpenOptions, CoreService, Principal, RegisterReferenceParams,
};

fn select_creator(home: &std::path::Path, creator: &str, workspace: &str) {
    std::fs::write(
        home.join(".nexus42/config.toml"),
        format!(
            "active_creator_id = \"{creator}\"\n[active_workspace_slug_by_creator]\n\"{creator}\" = \"{workspace}\"\n"
        ),
    )
    .unwrap();
}

/// Materialize the fixture workspace and its creator row, then release the
/// seed writer so a later open can take the admission.
async fn seed_workspace(home: &std::path::Path) {
    std::fs::create_dir_all(home.join(".nexus42")).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        home, "author", "default",
    ))
    .unwrap();
    select_creator(home, "author", "default");
    {
        let db = nexus_home_layout::workspace_state_db_path(home, "author", "default");
        let seed = nexus_local_db::writer_protocol::init_guarded_pool(&db, "author")
            .await
            .unwrap();
        let pool = seed.clone_pool();
        nexus_local_db::creators::ensure_creator_row(&pool, "author", "Author")
            .await
            .unwrap();
        pool.close().await;
    }
}

async fn open_core(home: &std::path::Path) -> (CoreService, Principal) {
    let core = CoreService::open(CoreOpenOptions {
        user_home: home.into(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();
    (core, principal)
}

async fn core_with_creator(home: &std::path::Path) -> (CoreService, Principal) {
    seed_workspace(home).await;
    open_core(home).await
}

fn params(tags: Option<&str>) -> RegisterReferenceParams {
    RegisterReferenceParams {
        source_type: "note".to_string(),
        source_mutability: nexus_local_db::SourceMutability::Refreshable,
        uri: "notes/source.md".to_string(),
        title: "Source".to_string(),
        tags: tags.map(str::to_string),
        body: "canonical body".to_string(),
    }
}

/// Register → get → list round trip: the registry row lands with the
/// operational `wrk_<slug>` binding, the body file is written under the
/// creator root, and the get envelope carries every show-render field.
#[tokio::test]
async fn register_then_get_carries_show_render_fields() {
    let temp = tempfile::tempdir().unwrap();
    let (core, principal) = core_with_creator(temp.path()).await;

    let registered = core
        .register_reference(&principal, params(Some("alpha,beta")))
        .await
        .unwrap();
    assert!(registered.reference_source_id.starts_with("ref_"));
    assert_eq!(registered.title, "Source");
    assert_eq!(registered.source_type, "note");
    assert_eq!(registered.source_mutability, "refreshable");
    assert_eq!(registered.uri, "notes/source.md");
    let content_path = registered.content_path.clone().unwrap();

    // The canonical body.md exists on disk under the creator root.
    let body_path = nexus_home_layout::reference_body_path(
        temp.path(),
        "author",
        &registered.reference_source_id,
    );
    assert_eq!(
        std::fs::read_to_string(&body_path).unwrap(),
        "canonical body"
    );

    let response = core
        .get_reference(&principal, registered.reference_source_id.clone())
        .await
        .unwrap();
    assert_eq!(
        response.reference.reference_source_id,
        registered.reference_source_id
    );
    assert_eq!(
        response.reference.content_path.as_deref(),
        Some(content_path.as_str())
    );
    assert_eq!(response.workspace_id, "wrk_default");
    assert_eq!(response.refresh_policy, "offline");
    assert_eq!(response.tags.as_deref(), Some("alpha,beta"));
    assert!(response.content_hash.is_some());
    assert!(response.updated_at.is_none());

    // The wire envelope (what the daemon/native projection serializes) exposes
    // the show-render fields: `workspace_id`/`refresh_policy` always, the
    // nullable trio only once the registry has a value (`updated_at` stays
    // omitted while the column is NULL — the repo-wide `Option` convention).
    let wire = serde_json::to_value(&response).unwrap();
    let wire_object = wire.as_object().unwrap();
    for field in ["workspace_id", "tags", "content_hash", "refresh_policy"] {
        assert!(
            wire_object.contains_key(field),
            "missing wire field {field}: {wire}"
        );
    }
    assert_eq!(wire["workspace_id"], "wrk_default");
    assert!(!wire_object.contains_key("updated_at"));

    let list = core.list_references(&principal).await.unwrap();
    assert_eq!(list.references.len(), 1);
    assert_eq!(
        list.references[0].reference_source_id,
        registered.reference_source_id
    );
}

/// An unknown id is the legacy `NotFound` resource string, not a storage error.
#[tokio::test]
async fn get_unknown_reference_is_not_found() {
    let temp = tempfile::tempdir().unwrap();
    let (core, principal) = core_with_creator(temp.path()).await;

    let error = core
        .get_reference(&principal, "ref_missing".to_string())
        .await
        .unwrap_err();
    match error {
        CoreError::NotFound { resource } => {
            assert_eq!(resource, "reference_source: ref_missing");
        }
        other => panic!("expected NotFound, got {other:?}"),
    }
}

/// The schema-owned metadata projection omits a NULL optional field (it does
/// not emit JSON `null`); the list payload therefore stays field-identical but
/// not byte-identical to the retired hand-written DTO for a row with no
/// content path.
#[test]
fn null_optional_metadata_is_omitted_not_null() {
    let info = nexus_core::ReferenceSourceInfo {
        reference_source_id: "ref_x".to_string(),
        source_type: "note".to_string(),
        source_mutability: "static".to_string(),
        uri: "notes/x.md".to_string(),
        title: "X".to_string(),
        content_path: None,
        scan_status: "pending".to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
    };
    let wire = serde_json::to_value(&info).unwrap();
    let object = wire.as_object().unwrap();
    assert!(!object.contains_key("content_path"), "{wire}");
    assert_eq!(object.len(), 7, "{wire}");
}

/// A `source_type` outside the contract enum is refused by the core before any
/// registry row or body file is written.
#[tokio::test]
async fn invalid_source_type_is_refused_without_writes() {
    let temp = tempfile::tempdir().unwrap();
    let (core, principal) = core_with_creator(temp.path()).await;

    let mut refused = params(None);
    refused.source_type = "image".to_string();
    let error = core
        .register_reference(&principal, refused)
        .await
        .unwrap_err();
    match error {
        CoreError::InvalidInput { field, .. } => assert_eq!(field, "source_type"),
        other => panic!("expected InvalidInput, got {other:?}"),
    }

    // No registry row and no body unit directory were created.
    let registry = core.list_references(&principal).await.unwrap();
    assert!(registry.references.is_empty(), "{registry:?}");
    let units = nexus_home_layout::reference_body_path(temp.path(), "author", "ref_probe")
        .parent()
        .unwrap()
        .to_path_buf();
    assert!(
        !units.exists(),
        "body unit dir written: {}",
        units.display()
    );
}

/// Greptile #5: a valid register on a `ReadOnly` core is refused with the
/// standard forbidden classification before the registry insert is reached,
/// not reported as an internal storage failure.
#[tokio::test]
async fn read_only_access_refuses_register_without_writes() {
    let temp = tempfile::tempdir().unwrap();
    seed_workspace(temp.path()).await;
    let core = CoreService::open(CoreOpenOptions {
        user_home: temp.path().into(),
        access: CoreAccess::ReadOnly,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();

    assert!(matches!(
        core.register_reference(&principal, params(None)).await,
        Err(CoreError::Forbidden { ref resource })
            if resource == "work: read-only core access"
    ));

    // No registry row and no body unit directory were created.
    let registry = core.list_references(&principal).await.unwrap();
    assert!(registry.references.is_empty(), "{registry:?}");
    let units = nexus_home_layout::reference_body_path(temp.path(), "author", "ref_probe")
        .parent()
        .unwrap()
        .to_path_buf();
    assert!(
        !units.exists(),
        "body unit dir written: {}",
        units.display()
    );

    core.close().await.unwrap();
}

/// A registry row updated by the refresh lifecycle carries `updated_at` on the
/// get wire (the CLI renders it only when present).
#[tokio::test]
async fn refreshed_row_populates_updated_at_on_the_wire() {
    let temp = tempfile::tempdir().unwrap();
    seed_workspace(temp.path()).await;

    let db = nexus_home_layout::workspace_state_db_path(temp.path(), "author", "default");
    let reference_id = {
        let seed = nexus_local_db::writer_protocol::init_guarded_pool(&db, "author")
            .await
            .unwrap();
        let pool = seed.clone_pool();
        let row = nexus_local_db::register_reference(
            &pool,
            nexus_local_db::RegisterParams {
                home: temp.path(),
                creator_id: "author",
                workspace_id: "wrk_default",
                source_type: "note",
                source_mutability: nexus_local_db::SourceMutability::Refreshable,
                uri: "notes/refreshed.md",
                title: "Refreshed",
                tags: None,
                body: "body",
            },
        )
        .await
        .unwrap();
        nexus_local_db::reference_source::mark_refreshed(
            &pool,
            &row.reference_source_id,
            row.content_hash.as_deref().unwrap(),
        )
        .await
        .unwrap();
        pool.close().await;
        row.reference_source_id
    };

    let (core, principal) = open_core(temp.path()).await;
    let response = core
        .get_reference(&principal, reference_id.clone())
        .await
        .unwrap();
    assert!(response.updated_at.is_some(), "{response:?}");
    let wire = serde_json::to_value(&response).unwrap();
    assert!(wire.get("updated_at").is_some(), "{wire}");
    assert_eq!(wire["reference"]["reference_source_id"], reference_id);
}
