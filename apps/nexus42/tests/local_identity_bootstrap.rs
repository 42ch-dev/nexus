//! Local identity mint preflight (I-000330).
//!
//! `mint_and_materialize` runs the config document's fallible admission read
//! (the I-000304 refusal) under the writer lock before the identity INSERT,
//! so a refused bootstrap persists nothing instead of leaving a minted,
//! unselected persistent identity row behind.

use assert_cmd::Command;
use std::process::Output;

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// A config document that cannot be read (the I-000304 refusal shape) must
/// fail the persistent identity mint before any store write: the CLI exits
/// non-zero with zero identity rows committed — nothing left minted and
/// unselected.
#[tokio::test]
async fn a_refused_config_document_leaves_no_identity_row_behind() {
    let home = tempfile::tempdir().expect("temp home");
    let nexus = home.path().join(".nexus42");
    std::fs::create_dir_all(&nexus).expect("nexus home");
    // A directory at the document path is an existing entry that cannot be
    // read — the portable stand-in for a dangling link, same refusal shape.
    std::fs::create_dir_all(nexus.join("config.toml")).expect("unreadable config entry");

    // The named mint leg reaches `mint_and_materialize` straight from the
    // empty-store decision tree; the nameless leg would refuse earlier, at
    // the plain `CliConfig::load` of the selection probe.
    let output = Command::cargo_bin("nexus42")
        .expect("nexus42 binary")
        .args(["system", "identity", "create", "--name", "refused-mint"])
        .env("HOME", home.path())
        .env("RUST_LOG", "off")
        .output()
        .expect("spawn nexus42 identity create");
    assert!(
        !output.status.success(),
        "the unreadable config must refuse the mint: {}",
        stderr(&output)
    );

    // The store the mint leg created must hold zero identity rows: the
    // INSERT never ran.
    let pool = nexus_local_db::open_pool_read_only(&nexus.join("state.db"))
        .await
        .expect("identity store");
    let rows = nexus_local_db::list_local_identities(&pool)
        .await
        .expect("list identities");
    assert!(
        rows.is_empty(),
        "a refused mint leaves no identity rows behind: {rows:?}"
    );
}
