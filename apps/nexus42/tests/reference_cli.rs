//! Direct-core CLI tests — `creator reference register|list|show`
//! (v1.207 P1 Task 3, R-V1190-REF-REGISTER-GAP).
//!
//! The leaves run on the typed core seam (`CoreService::register_reference` /
//! `list_references` / `get_reference`) against a hermetic direct-core home —
//! no daemon, no Node child (`common/direct.rs` precedent). The rendered text
//! is pinned field-for-field, including the show-render fields the get
//! envelope gained (`Workspace`/`Tags`/`Content Hash`), so the migration off
//! the local registry cannot drop CLI output.

#![allow(clippy::too_many_lines)] // one end-to-end CLI scenario per test

#[path = "common/direct.rs"]
mod direct;

use direct::DirectFixture;
use std::process::Output;

/// Workspace the fixture materializes and selects.
const WORKSPACE_SLUG: &str = "default";

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Register → list → show keeps the full human render, and the not-found path
/// keeps its verbatim wording.
#[tokio::test]
async fn reference_register_list_show_round_trip() {
    let fixture = DirectFixture::new().await;

    let register = fixture
        .command()
        .args([
            "creator",
            "reference",
            "register",
            "--source",
            "notes/source.md",
            "--source-type",
            "note",
            "--title",
            "Smoke Source",
            "--tags",
            "alpha,beta",
            "--mutability",
            "refreshable",
            "--body",
            "canonical body",
        ])
        .output()
        .expect("run reference register");
    assert!(
        register.status.success(),
        "register failed: {}",
        stderr(&register)
    );
    let registered = stdout(&register);
    let reference_id = registered
        .lines()
        .find_map(|line| line.strip_prefix("✓ Reference registered: "))
        .expect("registration line")
        .to_string();
    assert!(reference_id.starts_with("ref_"), "{registered}");
    assert_eq!(
        registered.trim_end(),
        format!(
            "✓ Reference registered: {reference_id}\n  Title:  Smoke Source\n  Type:   note\n  URI:    notes/source.md\n  Body:   references/units/{reference_id}/body.md"
        ),
        "register render changed"
    );

    let list = fixture
        .command()
        .args(["creator", "reference", "list"])
        .output()
        .expect("run reference list");
    assert!(list.status.success(), "list failed: {}", stderr(&list));
    let listed = stdout(&list);
    let mut lines = listed.lines();
    assert_eq!(
        lines.next().expect("header"),
        format!(
            "{:<40} {:<10} {:<12} {:<40} CREATED_AT",
            "ID", "TYPE", "MUTABILITY", "TITLE"
        )
    );
    let row = lines.next().expect("row");
    assert!(
        row.starts_with(&format!(
            "{reference_id:<40} note       refreshable  Smoke Source"
        )),
        "list row render changed: {row}"
    );

    let show = fixture
        .command()
        .args(["creator", "reference", "show", &reference_id])
        .output()
        .expect("run reference show");
    assert!(show.status.success(), "show failed: {}", stderr(&show));
    let shown = stdout(&show);
    for expected in [
        format!("Reference: {reference_id}"),
        "  Title:        Smoke Source".to_string(),
        "  Type:         note".to_string(),
        "  Mutability:   refreshable".to_string(),
        "  URI:          notes/source.md".to_string(),
        format!("  Workspace:    wrk_{WORKSPACE_SLUG}"),
        "  Scan Status:  pending".to_string(),
        "  Tags:         alpha,beta".to_string(),
        format!("  Body Path:    references/units/{reference_id}/body.md"),
    ] {
        assert!(
            shown.contains(&expected),
            "show render missing {expected:?}: {shown}"
        );
    }
    assert!(
        shown.contains("  Content Hash: "),
        "show render missing content hash: {shown}"
    );
    assert!(
        shown.contains("  Created:      "),
        "show render missing created: {shown}"
    );
    // The registry has no update yet, so the nullable line stays omitted.
    assert!(
        !shown.contains("  Updated:      "),
        "unexpected updated line: {shown}"
    );

    let missing = fixture
        .command()
        .args(["creator", "reference", "show", "ref_missing"])
        .output()
        .expect("run reference show missing");
    assert!(!missing.status.success());
    assert!(
        stderr(&missing).contains("Reference ref_missing not found."),
        "not-found wording changed: {}",
        stderr(&missing)
    );
}
