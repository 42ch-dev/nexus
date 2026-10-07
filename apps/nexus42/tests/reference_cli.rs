//! Direct-core CLI tests — `creator reference register|list|show`
//! (v1.207 P1 Task 3, R-V1190-REF-REGISTER-GAP).
//!
//! The leaves run on the typed core seam (`CoreService::register_reference` /
//! `list_references` / `get_reference`) against a hermetic direct-core home —
//! no daemon, no Node child (`common/direct.rs` precedent). The rendered text
//! is pinned line-for-line (labels, order, line count and the optional-line
//! conditions), so the migration off the local registry cannot drop or reorder
//! CLI output. The nullable `Updated:` line is exercised in its own scenario by
//! moving the registered row through the refresh lifecycle.

#![allow(clippy::too_many_lines)] // one end-to-end CLI scenario per test

#[path = "common/direct.rs"]
mod direct;

use direct::DirectFixture;
use nexus_home_layout::workspace_state_db_path;
use nexus_local_db::writer_protocol::{init_guarded_pool, release_retained_writer_guards};
use std::process::Output;

/// Workspace the fixture materializes and selects.
const WORKSPACE_SLUG: &str = "default";

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Run `creator reference register` with `args` and return its stdout.
fn register_source(fixture: &DirectFixture, args: &[&str]) -> String {
    let mut command_args = vec!["creator", "reference", "register"];
    command_args.extend_from_slice(args);
    let out = fixture
        .command()
        .args(&command_args)
        .output()
        .expect("run reference register");
    assert!(out.status.success(), "register failed: {}", stderr(&out));
    stdout(&out)
}

/// The registered reference id carried by a `reference register` render.
fn registered_id(registered: &str) -> String {
    registered
        .lines()
        .find_map(|line| line.strip_prefix("✓ Reference registered: "))
        .expect("registration line")
        .to_string()
}

/// The one creator the fixture registered, read from the hermetic home.
fn fixture_creator_id(fixture: &DirectFixture) -> String {
    std::fs::read_dir(fixture.home.path().join(".nexus42/creators"))
        .expect("creators dir")
        .next()
        .expect("one creator")
        .expect("creator entry")
        .file_name()
        .into_string()
        .expect("utf8 creator id")
}

/// Register → list → show keeps the full human render, and the not-found path
/// keeps its verbatim wording.
#[tokio::test]
async fn reference_register_list_show_round_trip() {
    let fixture = DirectFixture::new().await;

    let registered = register_source(
        &fixture,
        &[
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
        ],
    );
    let reference_id = registered_id(&registered);
    assert!(reference_id.starts_with("ref_"), "{registered}");
    assert_eq!(
        registered,
        format!(
            "✓ Reference registered: {reference_id}\n  Title:  Smoke Source\n  Type:   note\n  URI:    notes/source.md\n  Body:   references/units/{reference_id}/body.md\n"
        ),
        "register render is not byte-identical"
    );

    let list = fixture
        .command()
        .args(["creator", "reference", "list"])
        .output()
        .expect("run reference list");
    assert!(list.status.success(), "list failed: {}", stderr(&list));
    let listed = stdout(&list);
    let lines: Vec<&str> = listed.lines().collect();
    assert_eq!(lines.len(), 2, "list line count changed: {listed}");
    assert_eq!(
        lines[0],
        format!(
            "{:<40} {:<10} {:<12} {:<40} CREATED_AT",
            "ID", "TYPE", "MUTABILITY", "TITLE"
        )
    );
    // Every static column is pinned exactly; only the trailing timestamp is
    // dynamic (and is the last field, single-token).
    let row_prefix = format!(
        "{reference_id:<40} {:<10} {:<12} {:<40} ",
        "note", "refreshable", "Smoke Source"
    );
    assert!(
        lines[1].starts_with(&row_prefix),
        "list row layout changed: {}",
        lines[1]
    );
    let created_at = &lines[1][row_prefix.len()..];
    assert!(
        created_at.contains('T') && !created_at.contains(' '),
        "list row timestamp changed: {created_at:?}"
    );

    let show = fixture
        .command()
        .args(["creator", "reference", "show", &reference_id])
        .output()
        .expect("run reference show");
    assert!(show.status.success(), "show failed: {}", stderr(&show));
    let shown = stdout(&show);
    let lines: Vec<&str> = shown.lines().collect();
    // Tags are set and `updated_at` is NULL, so `Updated:` is the only line
    // that is absent.
    assert_eq!(lines.len(), 11, "show line count changed: {shown}");
    assert_eq!(lines[0], format!("Reference: {reference_id}"));
    assert_eq!(lines[1], "  Title:        Smoke Source");
    assert_eq!(lines[2], "  Type:         note");
    assert_eq!(lines[3], "  Mutability:   refreshable");
    assert_eq!(lines[4], "  URI:          notes/source.md");
    assert_eq!(lines[5], format!("  Workspace:    wrk_{WORKSPACE_SLUG}"));
    assert_eq!(lines[6], "  Scan Status:  pending");
    assert!(lines[7].starts_with("  Created:      "), "{shown}");
    assert_eq!(lines[8], "  Tags:         alpha,beta");
    assert!(lines[9].starts_with("  Content Hash: "), "{shown}");
    assert_eq!(
        lines[10],
        format!("  Body Path:    references/units/{reference_id}/body.md")
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

    // The core owns the source-type grammar; the leaf keeps the legacy wording
    // for the refusal it renders.
    let bad_type = fixture
        .command()
        .args([
            "creator",
            "reference",
            "register",
            "--source",
            "notes/bad.md",
            "--source-type",
            "image",
            "--title",
            "Bad Source",
            "--body",
            "canonical body",
        ])
        .output()
        .expect("run reference register with a bad source type");
    assert!(!bad_type.status.success());
    assert!(
        stderr(&bad_type)
            .contains("Invalid source type \"image\". Must be one of: file, url, pdf, note."),
        "source-type refusal wording changed: {}",
        stderr(&bad_type)
    );
}

/// `reference show` renders the nullable `Updated:` line — and, with no tags
/// registered, drops exactly the `Tags:` line — once the refresh lifecycle has
/// touched the row.
#[tokio::test]
async fn reference_show_renders_updated_at_when_set() {
    let fixture = DirectFixture::new().await;

    let reference_id = registered_id(&register_source(
        &fixture,
        &[
            "--source",
            "notes/refreshed.md",
            "--title",
            "Refreshed Source",
            "--body",
            "canonical body",
        ],
    ));

    // Move the registered row through the refresh lifecycle (which owns
    // `updated_at`) on the fixture's state DB, then release the writer before
    // the CLI child runs.
    let db = workspace_state_db_path(
        fixture.home.path(),
        &fixture_creator_id(&fixture),
        WORKSPACE_SLUG,
    );
    {
        let seed = init_guarded_pool(&db, &fixture_creator_id(&fixture))
            .await
            .expect("seed writer");
        let pool = seed.clone_pool();
        nexus_local_db::reference_source::mark_refreshed(&pool, &reference_id, &"a".repeat(64))
            .await
            .expect("mark refreshed");
        pool.close().await;
    }
    release_retained_writer_guards(&db);

    let show = fixture
        .command()
        .args(["creator", "reference", "show", &reference_id])
        .output()
        .expect("run reference show");
    assert!(show.status.success(), "show failed: {}", stderr(&show));
    let shown = stdout(&show);
    let lines: Vec<&str> = shown.lines().collect();
    assert_eq!(lines.len(), 11, "show line count changed: {shown}");
    assert_eq!(lines[0], format!("Reference: {reference_id}"));
    assert_eq!(lines[1], "  Title:        Refreshed Source");
    assert_eq!(lines[2], "  Type:         note");
    assert_eq!(lines[3], "  Mutability:   static");
    assert_eq!(lines[4], "  URI:          notes/refreshed.md");
    assert_eq!(lines[5], format!("  Workspace:    wrk_{WORKSPACE_SLUG}"));
    assert_eq!(lines[6], "  Scan Status:  pending");
    assert!(lines[7].starts_with("  Created:      "), "{shown}");
    assert!(lines[8].starts_with("  Updated:      "), "{shown}");
    assert!(lines[9].starts_with("  Content Hash: "), "{shown}");
    assert_eq!(
        lines[10],
        format!("  Body Path:    references/units/{reference_id}/body.md")
    );
    assert!(
        !lines.iter().any(|line| line.starts_with("  Tags:")),
        "tags line present for a tagless row: {shown}"
    );
}

/// A stored terminal field keeps its own trailing whitespace: the render buffer
/// is emitted complete (only the appended line delimiter is dropped), where a
/// blanket trim would have removed field bytes.
#[tokio::test]
async fn reference_show_preserves_terminal_field_bytes() {
    let fixture = DirectFixture::new().await;
    let reference_id = registered_id(&register_source(
        &fixture,
        &[
            "--source",
            "notes/padded.md",
            "--title",
            "Padded Source",
            "--body",
            "canonical body",
        ],
    ));

    // Give the row a content path whose trailing bytes are load-bearing.
    let creator_id = fixture_creator_id(&fixture);
    let db = workspace_state_db_path(fixture.home.path(), &creator_id, WORKSPACE_SLUG);
    {
        let seed = init_guarded_pool(&db, &creator_id)
            .await
            .expect("seed writer");
        let pool = seed.clone_pool();
        sqlx::query(
            "UPDATE reference_sources SET content_path = content_path || '  ' \
             WHERE reference_source_id = ?",
        )
        .bind(&reference_id)
        .execute(&pool)
        .await
        .expect("pad content path");
        pool.close().await;
    }
    release_retained_writer_guards(&db);

    let show = fixture
        .command()
        .args(["creator", "reference", "show", &reference_id])
        .output()
        .expect("run reference show");
    assert!(show.status.success(), "show failed: {}", stderr(&show));
    let shown = stdout(&show);
    assert!(
        shown.ends_with(&format!(
            "  Body Path:    references/units/{reference_id}/body.md  \n"
        )),
        "terminal field bytes changed: {shown:?}"
    );
}
