// contract — docs/specs/architecture/spoke-adapter-architecture.md.
//! v1.191 P1 T8: request-bound construction at the port surface

/// `v1191_holder_ports`: the KE-bearing families are served by a *scoped*
/// adapter ([`NexusAdapter::new`]) and cannot be reached through the host
/// metadata/tools construction ([`NexusAdapter::new_host`]), whose KE ports
/// fail closed with the `read_scope_missing` marker.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v1191_holder_ports_scoped_and_host_constructions_are_distinct() {
    use nexus_knowledge::world_kb::knowledge_entry::KnowledgeOwnerRef;
    use nexus_knowledge::world_kb::KnowledgeReadScope;
    use nexus_spoke_adapter::{KnowledgeEntryPort, NexusAdapter, SpokeRejectCode, SpokeResult};

    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let pool = nexus_local_db::open_pool(&db_path).await.expect("pool");
    nexus_local_db::run_migrations(&pool)
        .await
        .expect("migrations");

    let scope = KnowledgeReadScope::creator_management(
        vec![KnowledgeOwnerRef::world("wld_surface")],
        Vec::new(),
    );
    let scoped = NexusAdapter::new(pool.clone(), scope);
    assert!(
        scoped.read_scope().is_some(),
        "the KE-capable construction keeps its bound selection"
    );
    let host = NexusAdapter::new_host(pool);
    assert!(
        host.read_scope().is_none(),
        "the host construction binds no knowledge scope"
    );

    match host.get_knowledge_entry("kb_any").await {
        SpokeResult::Reject(r) => {
            assert_eq!(r.code, SpokeRejectCode::InternalError, "got {r:?}");
            assert_eq!(
                r.details
                    .as_ref()
                    .and_then(|d| d.get("read_scope_missing"))
                    .and_then(serde_json::Value::as_bool),
                Some(true),
                "the refusal must carry the missing-scope marker"
            );
        }
        SpokeResult::Ok(_) => panic!("a host-only adapter must not read knowledge"),
    }
}
