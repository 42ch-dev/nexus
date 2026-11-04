//! N3 Task 3: the pinned `MemoryProviderBinding` and its open/close lifecycle.
//!
//! The binding is crate-private, so these regressions drive it through the
//! public open surface: the selection precedence (`Some` override, disk table
//! read once per open, absent table = local), the pin's stability for the whole
//! open, the real `memory_remote_disabled` refusal while the `memory-remote`
//! cohort does not exist, close idempotence, and the guard release a failed
//! open owes the next one.

use std::fs;
use std::path::PathBuf;

use nexus_core::memory_provider::{LocalMemoryProvider, DEFAULT_SPACE_ID};
use nexus_core::{
    CoreAccess, CoreError, CoreOpenOptions, CoreService, MemoryProviderSelection,
    RemoteMemoryProviderSelection,
};
use nexus_local_db::writer_protocol::{join_live_engine_pool, GuardedPoolOptions};
use tempfile::TempDir;

const CREATOR: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn provider_table(kind: &str) -> String {
    format!("\n[memory_provider]\nkind = \"{kind}\"\n")
}

fn remote_table() -> String {
    format!(
        "\n[memory_provider]\n\
         kind = \"remote\"\n\
         space_id = \"spc_remote\"\n\
         credential_id = \"cred-alpha\"\n\
         connect_endpoint = \"wss://memory.example.test/\"\n\
         remote_pubkey_hex = \"{}\"\n\
         allowlist = [\"peer-alpha\"]\n",
        "ab".repeat(32)
    )
}

fn remote_selection() -> MemoryProviderSelection {
    MemoryProviderSelection::Remote(RemoteMemoryProviderSelection {
        space_id: "spc_remote".to_string(),
        credential_id: "cred-alpha".to_string(),
        connect_endpoint: "wss://memory.example.test/".to_string(),
        remote_pubkey_hex: "ab".repeat(32),
        allowlist: vec!["peer-alpha".to_string()],
    })
}

fn is_remote_disabled(error: &CoreError) -> bool {
    matches!(error, CoreError::Coded { code, .. } if code == "memory_remote_disabled")
}

struct Env {
    _tmp: TempDir,
    user_home: PathBuf,
}

impl Env {
    fn new(memory_table: Option<&str>) -> Self {
        let tmp = TempDir::new().expect("temp home");
        let user_home = tmp.path().to_path_buf();
        fs::create_dir_all(user_home.join(".nexus42")).expect("nexus home");
        fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
            &user_home, CREATOR, "default",
        ))
        .expect("workspace dir");
        let env = Self {
            _tmp: tmp,
            user_home,
        };
        env.write_config(memory_table);
        env
    }

    fn write_config(&self, memory_table: Option<&str>) {
        let mut text = format!(
            "active_creator_id = \"{CREATOR}\"\n\
             [active_workspace_slug_by_creator]\n\
             \"{CREATOR}\" = \"default\"\n"
        );
        if let Some(table) = memory_table {
            text.push_str(table);
        }
        fs::write(self.user_home.join(".nexus42").join("config.toml"), text).expect("seed config");
    }

    fn options(&self, memory: Option<MemoryProviderSelection>) -> CoreOpenOptions {
        CoreOpenOptions {
            user_home: self.user_home.clone(),
            access: CoreAccess::EngineOwner,
            memory,
        }
    }

    fn db_path(&self) -> PathBuf {
        nexus_home_layout::workspace_state_db_path(&self.user_home, CREATOR, "default")
    }
}

fn local_provider(core: &CoreService) -> std::sync::Arc<LocalMemoryProvider> {
    core.memory_provider_for_test().expect("pinned local provider")
}

/// A home with no `[memory_provider]` table keeps the byte-equivalent local
/// default, and that local binding really serves the memory family.
#[tokio::test]
async fn absent_provider_table_pins_the_local_default() {
    let env = Env::new(None);
    let core = CoreService::open(env.options(None)).await.expect("local default");
    let principal = core.active_principal().await.expect("principal");

    let provider = local_provider(&core);
    assert_eq!(provider.space_id(), DEFAULT_SPACE_ID);
    assert!(provider.is_writable(), "an engine owner pins a writable local provider");
    assert_eq!(core.count_pending_reviews(&principal).await.expect("provider read").count, 0);

    let report = core.close().await.expect("close");
    assert!(report.cleanup_confirmed);
}

/// `Some(Local)` is an open-only override: it beats a remote disk selection for
/// this open and never writes configuration, so the next open still sees the
/// disk's real refusal.
#[tokio::test]
async fn explicit_local_overrides_a_remote_disk_selection() {
    let env = Env::new(Some(&remote_table()));
    let core = CoreService::open(env.options(Some(MemoryProviderSelection::Local)))
        .await
        .expect("explicit local override opens");
    assert_eq!(local_provider(&core).space_id(), DEFAULT_SPACE_ID);
    core.close().await.expect("close");

    let error = CoreService::open(env.options(None))
        .await
        .err()
        .expect("the disk selection applies to the next open");
    assert!(
        is_remote_disabled(&error),
        "expected the typed refusal, got {error:?}"
    );
}

/// `Some(...)` never reads the provider table, so a malformed one cannot change
/// an explicit open.
#[tokio::test]
async fn explicit_local_never_reads_the_provider_table() {
    let env = Env::new(Some(&provider_table("bogus")));
    let core = CoreService::open(env.options(Some(MemoryProviderSelection::Local)))
        .await
        .expect("an explicit selection does not read the malformed table");
    assert_eq!(local_provider(&core).space_id(), DEFAULT_SPACE_ID);
    core.close().await.expect("close");
}

/// The disk table is read once per open: a live binding never re-reads or
/// retargets, while the next open resolves the current table. A sibling open
/// that fails on that reread releases only its own admissions.
#[tokio::test]
async fn provider_selection_is_pinned_for_the_open_and_reread_between_opens() {
    let env = Env::new(None);
    let core = CoreService::open(env.options(None)).await.expect("local default");
    let principal = core.active_principal().await.expect("principal");
    assert_eq!(core.count_pending_reviews(&principal).await.expect("provider read").count, 0);

    env.write_config(Some(&remote_table()));
    assert_eq!(
        local_provider(&core).space_id(),
        DEFAULT_SPACE_ID,
        "a live open never re-reads or retargets its provider"
    );
    assert_eq!(
        core.count_pending_reviews(&principal).await.expect("provider read").count,
        0,
        "the pinned local binding keeps serving reads after the table changed"
    );

    let error = CoreService::open(env.options(None))
        .await
        .err()
        .expect("the next open reads the changed table");
    assert!(
        is_remote_disabled(&error),
        "expected the typed refusal, got {error:?}"
    );
    assert_eq!(
        core.count_pending_reviews(&principal).await.expect("provider read").count,
        0,
        "the failed sibling open released only its own admissions"
    );

    core.close().await.expect("close");

    env.write_config(None);
    let reopened = CoreService::open(env.options(None))
        .await
        .expect("a later open reads the restored table");
    assert_eq!(local_provider(&reopened).space_id(), DEFAULT_SPACE_ID);
    reopened.close().await.expect("close");

    env.write_config(Some(&remote_table()));
    let error = CoreService::open(env.options(None))
        .await
        .err()
        .expect("the reread sees the remote table again");
    assert!(
        is_remote_disabled(&error),
        "expected the typed refusal, got {error:?}"
    );
}

/// A remote selection is refused with the real `memory_remote_disabled` code
/// and the failed open releases the writer admission it took, so the next open
/// acquires fresh instead of joining a half-open service.
#[tokio::test]
async fn remote_selection_is_refused_and_releases_its_own_guards() {
    let env = Env::new(Some(&remote_table()));
    let error = CoreService::open(env.options(None))
        .await
        .err()
        .expect("a remote selection is refused while the feature does not exist");
    assert!(
        is_remote_disabled(&error),
        "expected the typed refusal, got {error:?}"
    );
    assert!(
        join_live_engine_pool(&env.db_path(), GuardedPoolOptions::default())
            .await
            .expect("retained guard probe")
            .is_none(),
        "the failed open must release its own retained engine admission"
    );

    env.write_config(None);
    let core = CoreService::open(env.options(None))
        .await
        .expect("the next open acquires the released admissions");
    assert_eq!(local_provider(&core).space_id(), DEFAULT_SPACE_ID);
    assert!(core.close().await.expect("close").cleanup_confirmed);

    let reopened = CoreService::open(env.options(None))
        .await
        .expect("a closed open released its admissions once more");
    assert!(reopened.close().await.expect("close").cleanup_confirmed);
}

/// Close is idempotent: every caller observes the ONE settled report, memory
/// access refuses through the closing barrier, and the home opens again.
#[tokio::test]
async fn close_is_idempotent_and_releases_the_binding() {
    let env = Env::new(None);
    let core = CoreService::open(env.options(None)).await.expect("local default");

    let first = core.close().await.expect("first close");
    let second = core.close().await.expect("second close");
    assert_eq!(first.state, second.state);
    assert_eq!(first.cleanup_confirmed, second.cleanup_confirmed);
    assert!(first.cleanup_confirmed, "the drain settled");
    assert!(matches!(
        core.memory_provider_for_test(),
        Err(CoreError::Closing)
    ));

    let reopened = CoreService::open(env.options(None))
        .await
        .expect("a closed service released its home");
    assert_eq!(local_provider(&reopened).space_id(), DEFAULT_SPACE_ID);
    reopened.close().await.expect("close");
}

/// The public `Some(Remote(..))` override is the same typed refusal as the disk
/// path, before any credential file exists: no fake connection and no silent
/// local fallback.
#[tokio::test]
async fn explicit_remote_selection_is_refused_without_reading_credentials() {
    let env = Env::new(None);
    let error = CoreService::open(env.options(Some(remote_selection())))
        .await
        .err()
        .expect("an explicit remote selection is refused");
    assert!(
        is_remote_disabled(&error),
        "expected the typed refusal, got {error:?}"
    );
}
