//! Product local-state reset proof (v1.192 P0-T3R; plan row 19, compass
//! D18/D20).
//!
//! Real fences, real files: a live admitted writer refuses the reset before a
//! single byte is deleted from any store, and only the exact `state.db` /
//! `state.db-wal` / `state.db-shm` files of each stored workspace are removed.

use std::fs;
use std::path::{Path, PathBuf};

use nexus_local_db::writer_protocol::release_retained_writer_guards;
use nexus_local_db::{init_guarded_pool, reset_local_state, LocalDbError};
use tempfile::TempDir;

const STATE_FILES: [&str; 3] = ["state.db", "state.db-wal", "state.db-shm"];

/// Seed one stored workspace, writing the requested state files.
fn seed_store(home: &Path, creator_id: &str, slug: &str, names: &[&str]) -> PathBuf {
    let dir = home
        .join(".nexus42")
        .join("creators")
        .join(creator_id)
        .join("workspaces")
        .join(slug);
    fs::create_dir_all(&dir).expect("seed store dir");
    for name in names {
        fs::write(dir.join(name), format!("{name}-payload")).expect("seed state file");
    }
    dir
}

/// Presence check that never follows a symlink.
fn exists(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}

#[test]
fn reset_removes_exact_state_files_and_preserves_everything_else() {
    let home = TempDir::new().expect("home");
    let store = seed_store(home.path(), "ctr_alpha", "default", &STATE_FILES);

    // Sibling workspace data and the store's stable admission locks stay.
    fs::write(store.join("workspace.toml"), "keep").expect("toml");
    fs::write(store.join("state.db.migration.lock"), "").expect("migration lock");
    fs::write(store.join("state.db.engine.lock"), "").expect("engine lock");
    fs::create_dir_all(store.join("kb").join("entries")).expect("kb dir");
    fs::write(store.join("kb").join("entries").join("note.md"), "keep").expect("kb note");

    // A store holding only WAL/SHM siblings is an exact-file target too.
    let siblings_only = seed_store(
        home.path(),
        "ctr_beta",
        "ws2",
        &["state.db-wal", "state.db-shm"],
    );
    // A non-workspace entry under `creators/` is not a store.
    fs::write(
        home.path()
            .join(".nexus42")
            .join("creators")
            .join("stray.txt"),
        "keep",
    )
    .expect("stray file");
    // The creative user workspace is never a reset target.
    let user_doc = home.path().join("Documents").join("nexus").join("default");
    fs::create_dir_all(&user_doc).expect("user doc dir");
    fs::write(user_doc.join("state.db"), "user-document").expect("user doc db");

    let reset = reset_local_state(home.path()).expect("reset");

    assert_eq!(reset, 1, "only a store that owned state.db counts as reset");
    for name in STATE_FILES {
        assert!(!exists(&store.join(name)), "{name} must be deleted");
    }
    assert!(!exists(&siblings_only.join("state.db-wal")));
    assert!(!exists(&siblings_only.join("state.db-shm")));
    assert!(
        exists(&store.join("workspace.toml")),
        "sibling data survives"
    );
    assert!(exists(&store.join("kb").join("entries").join("note.md")));
    assert!(
        exists(&store.join("state.db.migration.lock")),
        "lock file survives"
    );
    assert!(
        exists(&store.join("state.db.engine.lock")),
        "lock file survives"
    );
    assert!(exists(
        &home
            .path()
            .join(".nexus42")
            .join("creators")
            .join("stray.txt")
    ));
    assert!(exists(&user_doc.join("state.db")), "user documents survive");
}

#[tokio::test]
async fn live_writer_refuses_the_reset_before_any_deletion() {
    let home = TempDir::new().expect("home");
    let live = seed_store(home.path(), "ctr_alpha", "default", &[]);
    let idle = seed_store(home.path(), "ctr_beta", "ws2", &STATE_FILES);

    // A real admitted direct writer on `ctr_alpha`: shared migration fence plus
    // the retained in-process guard (the daemon/test shape). It owns a real
    // `state.db` (plus its WAL/SHM siblings while the pool is open).
    let guarded = init_guarded_pool(&live.join("state.db"))
        .await
        .expect("init guarded pool");
    assert!(
        exists(&live.join("state.db")),
        "the admitted writer owns a real store"
    );

    let refused = reset_local_state(home.path());
    assert!(
        matches!(refused, Err(LocalDbError::OwnerBusy { .. })),
        "a live writer must refuse the reset, got {refused:?}"
    );
    assert!(
        exists(&live.join("state.db")),
        "the live store survives the refusal"
    );
    for name in STATE_FILES {
        assert!(
            exists(&idle.join(name)),
            "every store survives: all fences precede any deletion ({name})"
        );
    }

    // Release the writer (cooperative pool closed, retained guard dropped) and
    // the same reset succeeds.
    guarded.pool().close().await;
    drop(guarded);
    release_retained_writer_guards(&live.join("state.db"));

    let reset = reset_local_state(home.path()).expect("reset after close");
    assert_eq!(reset, 2, "both stores are reset once the writer is gone");
    assert!(!exists(&live.join("state.db")));
    for name in STATE_FILES {
        assert!(!exists(&idle.join(name)));
    }
}

#[cfg(unix)]
#[test]
fn symlinked_state_file_refuses_the_reset() {
    use std::os::unix::fs::symlink;

    let home = TempDir::new().expect("home");
    let intact = seed_store(home.path(), "ctr_alpha", "default", &STATE_FILES);
    let escape_target = home.path().join("outside-state.db");
    fs::write(&escape_target, "outside").expect("outside file");
    let linked = seed_store(home.path(), "ctr_beta", "ws2", &["state.db"]);
    symlink(&escape_target, linked.join("state.db-wal")).expect("symlink");

    let refused = reset_local_state(home.path());
    assert!(
        matches!(&refused, Err(LocalDbError::PathEscape { path, .. }) if path.ends_with("state.db-wal")),
        "a symlinked state file must refuse the reset, got {refused:?}"
    );
    // The refusal is a scan-time denial: nothing anywhere was deleted.
    for name in STATE_FILES {
        assert!(
            exists(&intact.join(name)),
            "{name} must survive the refusal"
        );
    }
    assert!(exists(&linked.join("state.db")));
    assert!(
        fs::symlink_metadata(linked.join("state.db-wal"))
            .expect("symlink metadata")
            .file_type()
            .is_symlink(),
        "the symlink itself is preserved, never followed or removed"
    );
    assert!(exists(&escape_target), "the symlink target is untouched");
}

#[cfg(unix)]
#[test]
fn symlinked_workspace_directory_refuses_the_reset() {
    use std::os::unix::fs::symlink;

    let home = TempDir::new().expect("home");
    let intact = seed_store(home.path(), "ctr_alpha", "default", &STATE_FILES);
    let outside = home.path().join("outside-workspace");
    fs::create_dir_all(&outside).expect("outside workspace");
    fs::write(outside.join("state.db"), "outside").expect("outside db");
    let workspaces = home
        .path()
        .join(".nexus42")
        .join("creators")
        .join("ctr_beta")
        .join("workspaces");
    fs::create_dir_all(&workspaces).expect("workspaces dir");
    symlink(&outside, workspaces.join("linked")).expect("symlink");

    let refused = reset_local_state(home.path());
    assert!(
        matches!(refused, Err(LocalDbError::PathEscape { .. })),
        "a symlinked workspace directory must refuse the reset, got {refused:?}"
    );
    for name in STATE_FILES {
        assert!(
            exists(&intact.join(name)),
            "{name} must survive the refusal"
        );
    }
    assert!(
        exists(&outside.join("state.db")),
        "the symlink target is untouched"
    );
}

#[test]
fn non_file_state_target_refuses_the_reset() {
    let home = TempDir::new().expect("home");
    let intact = seed_store(home.path(), "ctr_alpha", "default", &STATE_FILES);
    let odd = seed_store(home.path(), "ctr_beta", "ws2", &[]);
    fs::create_dir_all(odd.join("state.db")).expect("directory named state.db");

    let refused = reset_local_state(home.path());
    assert!(
        matches!(refused, Err(LocalDbError::PathEscape { .. })),
        "a non-file where a state file belongs must refuse the reset, got {refused:?}"
    );
    for name in STATE_FILES {
        assert!(
            exists(&intact.join(name)),
            "{name} must survive the refusal"
        );
    }
    assert!(odd.join("state.db").is_dir());
}

#[test]
fn absent_product_state_resets_zero_stores() {
    let home = TempDir::new().expect("home");
    assert_eq!(reset_local_state(home.path()).expect("bare home"), 0);
    fs::create_dir_all(home.path().join(".nexus42").join("creators")).expect("creators dir");
    assert_eq!(reset_local_state(home.path()).expect("empty creators"), 0);
}

#[test]
fn relative_home_is_refused() {
    let refused = reset_local_state(Path::new("relative-home"));
    assert!(
        matches!(refused, Err(LocalDbError::ValidationError(_))),
        "a relative home must be refused, got {refused:?}"
    );
}
