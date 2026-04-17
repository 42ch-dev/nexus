use nexus_orchestration::capability::CapabilityRegistry;

#[tokio::test]
async fn registry_lookup_builtin_workspace_open() {
    let reg = CapabilityRegistry::with_builtins();
    let cap = reg.get("workspace.open").expect("workspace.open registered");
    assert_eq!(cap.name(), "workspace.open");
}

#[tokio::test]
async fn registry_has_eleven_builtins() {
    let reg = CapabilityRegistry::with_builtins();
    assert_eq!(reg.len(), 11);
}

#[test]
fn registry_returns_none_for_missing() {
    let reg = CapabilityRegistry::with_builtins();
    assert!(reg.get("acp.prompt").is_none());
    assert!(reg.get("judge.llm").is_none());
}
