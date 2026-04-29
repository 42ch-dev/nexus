use nexus_orchestration::{
    engine::SessionFilter, CapabilityRegistry, GraphFlowEngine, OrchestrationEngine,
};
use std::sync::Arc;

#[tokio::test]
async fn new_session_and_list_active_roundtrip() {
    let storage = Arc::new(graph_flow::InMemorySessionStorage::new());
    let engine =
        GraphFlowEngine::new_with_storage(storage, Arc::new(CapabilityRegistry::with_builtins()));
    let ctx = nexus_orchestration::engine::Context::new();
    let key = nexus_orchestration::engine::SessionKey::test_fixture();
    let sid = engine.new_session(key, ctx).await.expect("new_session");
    let listed = engine
        .list_active(SessionFilter::default())
        .await
        .expect("list_active");
    assert!(listed.iter().any(|s| s.session_id == sid));
}
