//! Product local-state reset proof (v1.192 P0-T3R; plan row 19, compass
//! D18/D20).
//!
//! Real fences, real files: a live admitted writer refuses the reset before a
//! single byte is deleted from any store, and only the exact `state.db` /
//! `state.db-wal` / `state.db-shm` files of each stored workspace are removed.

use std::fs;
use std::path::{Path, PathBuf};

use nexus_local_db::writer_protocol::release_retained_writer_guards;
use nexus_local_db::{
    init_guarded_pool, reset_local_state, reset_local_state_with_post_fence_hook, LocalDbError,
};
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

/// A final target swapped for a symlink inside the fence window: the deletion
/// re-checks the admitted entry and refuses instead of following or removing it.
#[cfg(unix)]
#[test]
fn a_state_file_swapped_for_a_symlink_after_fencing_is_refused() {
    use std::os::unix::fs::symlink;

    let home = TempDir::new().expect("home");
    let store = seed_store(home.path(), "ctr_alpha", "default", &STATE_FILES);
    let outside = home.path().join("outside");
    fs::create_dir_all(&outside).expect("outside dir");
    let victim = outside.join("state.db");
    fs::write(&victim, "victim-payload").expect("victim file");

    // The seam runs with every fence held and before the first deletion: it
    // swaps the admitted `state.db` for a symlink out of the store.
    let refused = reset_local_state_with_post_fence_hook(home.path(), &mut || {
        fs::remove_file(store.join("state.db")).expect("remove the admitted file");
        symlink(&victim, store.join("state.db")).expect("swap for a symlink");
    });

    assert!(
        matches!(&refused, Err(LocalDbError::PathEscape { path, .. }) if path.ends_with("state.db")),
        "a state file swapped for a symlink must refuse the reset, got {refused:?}"
    );
    assert_eq!(
        fs::read_to_string(&victim).expect("victim survives"),
        "victim-payload",
        "the swapped-in link's target is never deleted"
    );
    assert!(
        exists(&store.join("state.db-wal")) && exists(&store.join("state.db-shm")),
        "the refusal precedes every deletion"
    );
    assert!(
        fs::symlink_metadata(store.join("state.db"))
            .expect("swapped entry metadata")
            .file_type()
            .is_symlink(),
        "the swapped-in symlink is preserved, never followed or removed"
    );
}

/// A store directory swapped out of its admitted path inside the fence window:
/// deletion resolves against the admitted directory, so the replacement at that
/// path keeps its data and the admitted store is the one that is reset.
#[cfg(unix)]
#[test]
fn a_store_directory_swapped_after_fencing_cannot_redirect_the_deletion() {
    use std::os::unix::fs::symlink;

    let home = TempDir::new().expect("home");
    let store = seed_store(home.path(), "ctr_alpha", "default", &STATE_FILES);
    fs::write(store.join("workspace.toml"), "keep").expect("sibling data");
    // A replacement directory put at the admitted path: a rival store whose
    // state files are not this reset's target.
    let replacement = home.path().join("replacement-workspace");
    fs::create_dir_all(&replacement).expect("replacement dir");
    for name in STATE_FILES {
        fs::write(replacement.join(name), "replacement-payload").expect("replacement state file");
    }
    let admitted_move = home.path().join("moved-admitted");

    // The seam runs with every fence held: move the admitted directory aside and
    // point its path at the replacement.
    let reset = reset_local_state_with_post_fence_hook(home.path(), &mut || {
        fs::rename(&store, &admitted_move).expect("move the admitted directory");
        symlink(&replacement, &store).expect("point the admitted path at the replacement");
    })
    .expect("reset");

    assert_eq!(reset, 1, "the admitted store is the one that was reset");
    for name in STATE_FILES {
        assert!(
            !exists(&admitted_move.join(name)),
            "the admitted store's {name} must be deleted"
        );
        assert!(
            exists(&replacement.join(name)),
            "the directory swapped into the path must keep its {name}"
        );
    }
    assert!(
        exists(&admitted_move.join("workspace.toml")),
        "sibling data of the admitted store survives"
    );
    assert!(
        fs::symlink_metadata(&store)
            .expect("swapped path metadata")
            .file_type()
            .is_symlink(),
        "the swapped-in symlink is preserved, never followed or removed"
    );
}
