use nexus_capability::CapabilityRegistry;
use nexus_orchestration::{engine::SessionFilter, GraphFlowEngine, OrchestrationEngine};
use std::sync::Arc;

#[tokio::test]
async fn new_session_and_list_active_roundtrip() {
    let storage = Arc::new(graph_flow::InMemorySessionStorage::new());
    let engine = GraphFlowEngine::new_with_storage(
        storage,
        nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        )),
    );
    let ctx = nexus_orchestration::engine::Context::new();
    let key = nexus_orchestration::engine::SessionKey {
        creator_id: "test-creator".to_string(),
        preset_id: "test-preset".to_string(),
        instance_id: "test-instance".to_string(),
    };
    let sid = engine.new_session(key, ctx).await.expect("new_session");
    let listed = engine
        .list_active(SessionFilter::default())
        .await
        .expect("list_active");
    assert!(listed.iter().any(|s| s.session_id == sid));
}
