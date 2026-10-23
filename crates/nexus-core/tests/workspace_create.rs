//! v1.220 P3-T1: workspace creation over the home-control entry.
//!
//! The create transaction owns slug validation, the single creative-root
//! derivation, exclusivity against every registered workspace, the writable
//! probe, and a registration (`meta.json`) that is written last — so a refused
//! or failed create leaves the home exactly as it was.

use nexus_contracts::daemon_api::workspace::create_workspace_request::CreateWorkspaceRequest;
use nexus_contracts::CoreRegisterCreatorRequest;
use nexus_core::{CoreError, CoreHomeService};
use tempfile::TempDir;

async fn home() -> (TempDir, CoreHomeService) {
    let tmp = tempfile::tempdir().unwrap();
    let svc = CoreHomeService::open(tmp.path().to_path_buf()).expect("open home");
    (tmp, svc)
}

async fn creator(home: &CoreHomeService) -> String {
    home.register_creator(CoreRegisterCreatorRequest {
        display_name: Some("P3 Create Author".parse().unwrap()),
        platform_creator_id: None,
    })
    .await
    .expect("register creator")
    .creator_id
}

fn request(
    creator_id: &str,
    slug: &str,
    display_name: Option<&str>,
    creative_root: Option<&str>,
) -> CreateWorkspaceRequest {
    CreateWorkspaceRequest {
        creator_id: creator_id.to_string(),
        workspace_slug: slug.to_string(),
        display_name: display_name.map(str::to_string),
        creative_root: creative_root.map(str::to_string),
    }
}

fn op_dir(user_home: &std::path::Path, creator_id: &str, slug: &str) -> std::path::PathBuf {
    nexus_home_layout::operational_workspace_dir(user_home, creator_id, slug)
}

#[tokio::test]
async fn create_derives_root_and_registers_workspace() {
    let (tmp, svc) = home().await;
    let user_home = tmp.path().to_path_buf();
    let creator_id = creator(&svc).await;

    let response = svc
        .create_workspace(request(&creator_id, "novel", Some("我的小说"), None))
        .await
        .expect("create workspace");

    let creative_root = user_home.join("Nexus").join("novel");
    assert_eq!(response.creator_id, creator_id);
    assert_eq!(response.workspace_slug, "novel");
    assert_eq!(response.creative_root, creative_root.display().to_string());
    assert_eq!(
        response.operational_dir,
        op_dir(&user_home, &creator_id, "novel")
            .display()
            .to_string()
    );
    assert_eq!(
        response.state_db_path,
        nexus_home_layout::workspace_state_db_path(&user_home, &creator_id, "novel")
            .display()
            .to_string()
    );
    assert!(creative_root
        .join(".nexus42")
        .join("workspace.json")
        .is_file());
    assert!(std::path::Path::new(&response.state_db_path).is_file());

    // Discovery sees the registration with its root and display name.
    let listed = svc.list_workspaces().await.expect("list");
    let item = listed
        .items
        .iter()
        .find(|item| item.workspace_slug == "novel")
        .expect("created workspace is listable");
    assert_eq!(item.creator_id, creator_id);
    assert_eq!(item.creative_root, response.creative_root);
    assert_eq!(item.display_name.as_deref(), Some("我的小说"));
}

#[tokio::test]
async fn create_adopts_empty_dir_and_refuses_foreign_content() {
    let (tmp, svc) = home().await;
    let user_home = tmp.path().to_path_buf();
    let creator_id = creator(&svc).await;

    // An existing empty directory is adopted.
    let adopted = user_home.join("adopted-root");
    std::fs::create_dir_all(&adopted).unwrap();
    let created = svc
        .create_workspace(request(
            &creator_id,
            "adopted",
            Some("Adopted"),
            Some(&adopted.display().to_string()),
        ))
        .await
        .expect("adopt empty dir");
    assert_eq!(created.creative_root, adopted.display().to_string());

    // A non-empty directory is never adopted as a workspace root.
    let foreign = user_home.join("foreign-root");
    std::fs::create_dir_all(&foreign).unwrap();
    std::fs::write(foreign.join("notes.md"), b"user content").unwrap();
    let err = svc
        .create_workspace(request(
            &creator_id,
            "foreign",
            Some("Foreign"),
            Some(&foreign.display().to_string()),
        ))
        .await
        .expect_err("foreign content is refused");
    assert!(
        matches!(err, CoreError::WorkspaceConflict(_)),
        "unexpected: {err:?}"
    );
    // No partial state: neither the operational dir nor our manifest existed.
    assert!(!op_dir(&user_home, &creator_id, "foreign").exists());
    assert!(!foreign.join(".nexus42").exists());
}

#[tokio::test]
async fn create_refuses_duplicate_slug_and_claimed_root() {
    let (tmp, svc) = home().await;
    let user_home = tmp.path().to_path_buf();
    let creator_id = creator(&svc).await;

    svc.create_workspace(request(&creator_id, "novel", Some("Novel"), None))
        .await
        .expect("first create");

    // Same slug for the same creator.
    let duplicate = svc
        .create_workspace(request(&creator_id, "novel", Some("Novel 2"), None))
        .await
        .expect_err("duplicate slug");
    assert!(
        matches!(duplicate, CoreError::WorkspaceConflict(_)),
        "unexpected: {duplicate:?}"
    );

    // A different slug cannot claim a root that is already registered.
    let claimed_root = user_home.join("Nexus").join("novel");
    let claimed = svc
        .create_workspace(request(
            &creator_id,
            "other",
            Some("Other"),
            Some(&claimed_root.display().to_string()),
        ))
        .await
        .expect_err("claimed root");
    assert!(
        matches!(claimed, CoreError::WorkspaceConflict(_)),
        "unexpected: {claimed:?}"
    );
    assert!(!op_dir(&user_home, &creator_id, "other").exists());
}

#[tokio::test]
async fn create_refuses_invalid_slug_name_and_override() {
    let (tmp, svc) = home().await;
    let user_home = tmp.path().to_path_buf();
    let creator_id = creator(&svc).await;

    for slug in ["", "a/b", "..", "a\\b", "a\nb", "a\0b"] {
        let err = svc
            .create_workspace(request(&creator_id, slug, Some("Bad"), None))
            .await
            .expect_err("invalid slug");
        assert!(
            matches!(err, CoreError::InvalidInput { ref field, .. } if field == "workspace_slug"),
            "slug {slug:?}: {err:?}"
        );
    }
    // Control characters are a validation failure, not a path-safety accident.
    let control = svc
        .create_workspace(request(&creator_id, "a\nb", Some("Bad"), None))
        .await
        .expect_err("control character in slug");
    assert!(
        matches!(
            control,
            CoreError::InvalidInput { ref field, ref reason }
                if field == "workspace_slug" && reason == "must not contain control characters"
        ),
        "unexpected: {control:?}"
    );

    let bad_name = svc
        .create_workspace(request(&creator_id, "ctrl", Some("bad\u{7}name"), None))
        .await
        .expect_err("control character in display name");
    assert!(
        matches!(bad_name, CoreError::InvalidInput { ref field, .. } if field == "display_name"),
        "unexpected: {bad_name:?}"
    );

    let relative = svc
        .create_workspace(request(
            &creator_id,
            "relative",
            Some("Relative"),
            Some("relative/root"),
        ))
        .await
        .expect_err("relative override");
    assert!(
        matches!(relative, CoreError::InvalidInput { ref field, .. } if field == "creative_root"),
        "unexpected: {relative:?}"
    );

    let traversal = user_home.join("elsewhere").join("..").join("root");
    let unsafe_override = svc
        .create_workspace(request(
            &creator_id,
            "traversal",
            Some("Traversal"),
            Some(&traversal.display().to_string()),
        ))
        .await
        .expect_err("traversal override");
    assert!(
        matches!(unsafe_override, CoreError::InvalidInput { ref field, .. } if field == "creative_root"),
        "unexpected: {unsafe_override:?}"
    );

    assert!(svc.list_workspaces().await.expect("list").items.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn create_refuses_unwritable_root() {
    use std::os::unix::fs::PermissionsExt;

    let (tmp, svc) = home().await;
    let user_home = tmp.path().to_path_buf();
    let creator_id = creator(&svc).await;

    let locked = user_home.join("locked-root");
    std::fs::create_dir_all(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
    let err = svc
        .create_workspace(request(
            &creator_id,
            "denied",
            Some("Denied"),
            Some(&locked.display().to_string()),
        ))
        .await
        .expect_err("unwritable root");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert!(
        matches!(
            err,
            CoreError::InvalidInput { ref field, ref reason }
                if field == "creative_root" && reason == "not writable"
        ),
        "unexpected: {err:?}"
    );
    assert!(!op_dir(&user_home, &creator_id, "denied").exists());
    // The adopted directory (pre-existing) is kept and carries no probe residue.
    assert!(locked.is_dir());
    let residue = std::fs::read_dir(&locked)
        .expect("locked readable")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(residue.is_empty(), "probe left residue: {residue:?}");
}

#[cfg(unix)]
#[tokio::test]
async fn create_probe_failure_leaves_no_directory() {
    use std::os::unix::fs::PermissionsExt;

    let (tmp, svc) = home().await;
    let user_home = tmp.path().to_path_buf();
    let creator_id = creator(&svc).await;

    // A read-only parent makes `create_dir_all` fail before any artifact.
    let locked_parent = user_home.join("locked-parent");
    std::fs::create_dir_all(&locked_parent).unwrap();
    std::fs::set_permissions(&locked_parent, std::fs::Permissions::from_mode(0o555)).unwrap();
    let derived_root = locked_parent.join("child");
    let err = svc
        .create_workspace(request(
            &creator_id,
            "blocked",
            Some("Blocked"),
            Some(&derived_root.display().to_string()),
        ))
        .await
        .expect_err("probe cannot create the root");
    std::fs::set_permissions(&locked_parent, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert!(
        matches!(
            err,
            CoreError::InvalidInput { ref field, ref reason }
                if field == "creative_root" && reason == "not writable"
        ),
        "unexpected: {err:?}"
    );
    assert!(
        !derived_root.exists(),
        "failed probe left its directory behind"
    );
    assert!(locked_parent.is_dir());
    assert!(!op_dir(&user_home, &creator_id, "blocked").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn create_refuses_unreadable_root() {
    use std::os::unix::fs::PermissionsExt;

    let (tmp, svc) = home().await;
    let user_home = tmp.path().to_path_buf();
    let creator_id = creator(&svc).await;

    let unreadable = user_home.join("unreadable-root");
    std::fs::create_dir_all(&unreadable).unwrap();
    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();
    let err = svc
        .create_workspace(request(
            &creator_id,
            "unreadable",
            Some("Unreadable"),
            Some(&unreadable.display().to_string()),
        ))
        .await
        .expect_err("unreadable root");
    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o755)).unwrap();

    // A client-side root problem folds to input validation, never an internal fault.
    assert!(
        matches!(
            err,
            CoreError::InvalidInput { ref field, ref reason }
                if field == "creative_root" && reason == "not readable"
        ),
        "unexpected: {err:?}"
    );
    assert!(!op_dir(&user_home, &creator_id, "unreadable").exists());
}

#[tokio::test]
async fn concurrent_same_slug_creates_commit_exactly_one() {
    let (tmp, svc) = home().await;
    let user_home = tmp.path().to_path_buf();
    let creator_id = creator(&svc).await;

    let (left, right) = tokio::join!(
        svc.create_workspace(request(&creator_id, "race", Some("Race A"), None)),
        svc.create_workspace(request(&creator_id, "race", Some("Race B"), None)),
    );
    let successes = usize::from(left.is_ok()) + usize::from(right.is_ok());
    assert_eq!(successes, 1, "left={left:?} right={right:?}");
    let loser = if left.is_err() { left } else { right };
    assert!(
        matches!(loser, Err(CoreError::WorkspaceConflict(_))),
        "unexpected: {loser:?}"
    );

    // One registration, with the committed directory pair intact.
    let listed = svc.list_workspaces().await.expect("list");
    assert_eq!(
        listed
            .items
            .iter()
            .filter(|item| item.workspace_slug == "race")
            .count(),
        1
    );
    let op_dir = op_dir(&user_home, &creator_id, "race");
    assert!(op_dir.join("meta.json").is_file());
    assert!(nexus_home_layout::workspace_state_db_path(&user_home, &creator_id, "race").is_file());
    assert!(user_home
        .join("Nexus")
        .join("race")
        .join(".nexus42")
        .join("workspace.json")
        .is_file());
}

#[tokio::test]
async fn concurrent_same_root_creates_keep_one_owner() {
    let (tmp, svc) = home().await;
    let user_home = tmp.path().to_path_buf();
    let creator_id = creator(&svc).await;
    let shared_root = user_home.join("shared-root");
    std::fs::create_dir_all(&shared_root).unwrap();
    let root = shared_root.display().to_string();

    let (left, right) = tokio::join!(
        svc.create_workspace(request(&creator_id, "left", Some("Left"), Some(&root))),
        svc.create_workspace(request(&creator_id, "right", Some("Right"), Some(&root))),
    );
    let successes = usize::from(left.is_ok()) + usize::from(right.is_ok());
    assert_eq!(successes, 1, "left={left:?} right={right:?}");
    let loser = if left.is_err() { left } else { right };
    assert!(
        matches!(loser, Err(CoreError::WorkspaceConflict(_))),
        "unexpected: {loser:?}"
    );

    let listed = svc.list_workspaces().await.expect("list");
    assert_eq!(listed.items.len(), 1);
    let winner = &listed.items[0];
    assert_eq!(winner.creative_root, root);
    // The loser left no operational directory behind.
    let loser_slug = if winner.workspace_slug == "left" {
        "right"
    } else {
        "left"
    };
    assert!(!op_dir(&user_home, &creator_id, loser_slug).exists());
}

#[tokio::test]
async fn create_requires_active_creator_and_matching_creator_id() {
    let (_tmp, svc) = home().await;

    // No active creator yet: creation is an uninitialized refusal, not a mint.
    let uninitialized = svc
        .create_workspace(request(
            "ctr_local000000000000",
            "novel",
            Some("Novel"),
            None,
        ))
        .await
        .expect_err("no active creator");
    assert!(
        matches!(uninitialized, CoreError::Uninitialized),
        "unexpected: {uninitialized:?}"
    );

    let creator_id = creator(&svc).await;
    let mismatched = svc
        .create_workspace(request("ctr_other", "novel", Some("Novel"), None))
        .await
        .expect_err("creator mismatch");
    assert!(
        matches!(mismatched, CoreError::InvalidInput { ref field, .. } if field == "creator_id"),
        "unexpected: {mismatched:?}"
    );
    // The active creator itself is accepted (the mismatch above created nothing).
    svc.create_workspace(request(&creator_id, "novel", Some("Novel"), None))
        .await
        .expect("active creator can create");
}
