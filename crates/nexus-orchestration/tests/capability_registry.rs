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
async fn registry_has_twenty_six_builtins() {
    // V1.36 P1 added `novel.project_scaffold` (18→19);
    // P3 added `novel.chapter_transition` (19→20);
    // V1.51 T-A P0 added `nexus.llm.extract` (20→21);
    // V1.52 T-A P2 added `essay.project_scaffold` (21→22);
    // V1.54 P1 added `game_bible.project_scaffold` (22→23);
    // V1.55 P3 added `script.project_scaffold` (23→24);
    // V1.56 P-last (R-V155P2-F002) added `game_bible.section_status.update` (24→25).
    // V1.58 P1 (DF-44) added `nexus.reference.refresh` (25→26).
    // V1.60 P0 (DF-46) added 5 orchestration capabilities (26→31):
    //   world.state.query, world.delta.propose, world.delta.apply,
    //   timeline.event.append, fork.create.
    // UPDATE THIS TEST when adding a new builtin.
    let reg = CapabilityRegistry::with_builtins();
    assert_eq!(reg.len(), 31);
}

#[tokio::test]
async fn registry_lookup_acp_capabilities() {
    let reg = CapabilityRegistry::with_builtins();
    assert!(reg.get("acp.prompt").is_some());
    assert!(reg.get("acp.session_load").is_some());
    assert!(reg.get("judge.llm").is_some());
    assert!(reg.get("context.summarize").is_some());
    // V1.51 T-A P0: nexus.llm.extract (sibling to judge.llm).
    assert!(reg.get("nexus.llm.extract").is_some());
}

#[test]
fn registry_returns_none_for_missing() {
    let reg = CapabilityRegistry::with_builtins();
    assert!(reg.get("nonexistent").is_none());
}
