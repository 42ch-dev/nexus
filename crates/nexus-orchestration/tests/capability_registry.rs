use nexus_orchestration::capability::CapabilityRegistry;

#[tokio::test]
async fn registry_lookup_builtin_workspace_open() {
    let reg = CapabilityRegistry::with_builtins();
    let cap = reg
        .get("workspace.open")
        .expect("workspace.open registered");
    assert_eq!(cap.name(), "workspace.open");
}

#[tokio::test]
async fn registry_has_eighteen_builtins() {
    // V1.33 P2 added `creator.write_brief`; previously 17, now 18.
    let reg = CapabilityRegistry::with_builtins();
    assert_eq!(reg.len(), 18);
}

#[tokio::test]
async fn registry_lookup_acp_capabilities() {
    let reg = CapabilityRegistry::with_builtins();
    assert!(reg.get("acp.prompt").is_some());
    assert!(reg.get("acp.session_load").is_some());
    assert!(reg.get("judge.llm").is_some());
    assert!(reg.get("context.summarize").is_some());
}

#[test]
fn registry_returns_none_for_missing() {
    let reg = CapabilityRegistry::with_builtins();
    assert!(reg.get("nonexistent").is_none());
}
