use std::path::PathBuf;

use nexus_memory_conformance::FixtureMemoryProvider;
use tempfile::TempDir;

fn database_path(home: &TempDir) -> PathBuf {
    home.path().join("fixture-memory.sqlite3")
}

#[tokio::test]
async fn open_creates_a_file_backed_space() {
    let home = TempDir::new().expect("temp home");
    let database = database_path(&home);
    assert!(!database.exists(), "a fresh scenario starts without a store");

    let provider = FixtureMemoryProvider::open(&database)
        .await
        .expect("first open provisions the fixture");
    assert!(
        database.is_file(),
        "the fixture store lives in the scenario database file"
    );
    let identity = provider.identity();
    assert!(
        identity.space_id.starts_with("spc_"),
        "unexpected space id {}",
        identity.space_id
    );
    assert!(
        !identity.authority_host_id.is_empty(),
        "the authority host label is provisioned with the space"
    );
    assert_eq!(identity.authority_generation, 1);
    provider.close().await;
}

#[tokio::test]
async fn reopen_same_file_reads_the_persisted_space_and_authority() {
    let home = TempDir::new().expect("temp home");
    let database = database_path(&home);

    let provider = FixtureMemoryProvider::open(&database)
        .await
        .expect("first open");
    let first = provider.identity().clone();
    provider.close().await;

    let reopened = FixtureMemoryProvider::open(&database).await.expect("reopen");
    assert_eq!(
        reopened.identity(),
        &first,
        "the space and authority row must survive a restart"
    );
    reopened.close().await;

    let reopened_again = FixtureMemoryProvider::open(&database)
        .await
        .expect("second reopen");
    assert_eq!(
        reopened_again.identity(),
        &first,
        "reopening must not mint a second space"
    );
    reopened_again.close().await;
}

#[tokio::test]
async fn fresh_homes_bootstrap_isolated_authorities() {
    let home_a = TempDir::new().expect("temp home a");
    let home_b = TempDir::new().expect("temp home b");
    let database_a = database_path(&home_a);
    let database_b = database_path(&home_b);
    assert_ne!(database_a, database_b);

    let provider_a = FixtureMemoryProvider::open(&database_a)
        .await
        .expect("open a");
    let provider_b = FixtureMemoryProvider::open(&database_b)
        .await
        .expect("open b");
    let identity_a = provider_a.identity().clone();
    let identity_b = provider_b.identity().clone();

    assert_ne!(
        identity_a.space_id, identity_b.space_id,
        "each scenario home provisions its own space"
    );
    assert_ne!(
        identity_a.authority_host_id, identity_b.authority_host_id,
        "each scenario home is its own authority"
    );

    provider_b.close().await;
    provider_a.close().await;

    let reopened_a = FixtureMemoryProvider::open(&database_a)
        .await
        .expect("reopen a");
    assert_eq!(
        reopened_a.identity(),
        &identity_a,
        "a home keeps its own identity after a sibling home exists"
    );
    reopened_a.close().await;
}
