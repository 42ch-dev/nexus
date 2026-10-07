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

async fn core_with_creator(home: &std::path::Path) -> (CoreService, Principal) {
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
    let core = CoreService::open(CoreOpenOptions {
        user_home: home.into(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();
    (core, principal)
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
