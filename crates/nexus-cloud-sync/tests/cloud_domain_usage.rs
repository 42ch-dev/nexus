//! Cloud-domain usage tests for `nexus-cloud-sync`.
//!
//! Verifies that cloud-sync correctly integrates with `nexus-cloud-domain`
//! for User and Pairing domain invariants:
//!
//! 1. The `cloud_domain` re-export is accessible
//! 2. Cloud-domain User/Pairing types can be used from cloud-sync context
//! 3. Domain functions (register, suspend, lifecycle) work correctly

#![allow(clippy::manual_string_new)]

// ── E3.1: Re-export accessibility ───────────────────────────────────

/// The `cloud_domain` re-export from `nexus_cloud_sync::cloud_domain` must
/// resolve to the `nexus_cloud_domain` crate.
#[test]
fn cloud_domain_reexport_is_accessible() {
    // If this compiles, the re-export works.
    let _ = std::mem::size_of::<nexus_cloud_sync::cloud_domain::user::User>();
    let _ = std::mem::size_of::<nexus_cloud_sync::cloud_domain::pairing::Pairing>();
}

// ── E3.2: User type usage from cloud-sync context ───────────────────

#[test]
fn user_domain_register_and_lifecycle() {
    // Use cloud-domain User through cloud-sync re-export
    let mut user = nexus_cloud_sync::cloud_domain::user::User::register(
        "usr_42",
        "alice",
        "alice@example.com",
        "Alice Writer",
    );

    // Initial state
    assert_eq!(user.account_status, "active");
    assert_eq!(user.subscription_tier, "free");
    assert_eq!(user.user_id, "usr_42");
    assert_eq!(user.username, "alice");

    // Suspend
    user.suspend().expect("suspend should succeed");
    assert_eq!(user.account_status, "suspended");

    // Cannot suspend again
    assert!(user.suspend().is_err());
}

#[test]
fn user_subscription_tier_change() {
    let mut user = nexus_cloud_sync::cloud_domain::user::User::register(
        "usr_99",
        "bob",
        "bob@example.com",
        "Bob",
    );

    // Upgrade to pro
    user.set_subscription_tier(nexus_cloud_sync::cloud_domain::user::SubscriptionTier::Pro)
        .expect("tier change should succeed");
    assert_eq!(user.subscription_tier, "pro");

    // Delete blocks tier change
    user.mark_deleted().expect("delete should succeed");
    assert!(user
        .set_subscription_tier(nexus_cloud_sync::cloud_domain::user::SubscriptionTier::Studio)
        .is_err());
}

#[test]
fn user_soft_delete_prevents_redelete() {
    let mut user = nexus_cloud_sync::cloud_domain::user::User::register(
        "usr_del",
        "charlie",
        "c@example.com",
        "Charlie",
    );
    user.mark_deleted().expect("first delete should succeed");
    assert!(user.mark_deleted().is_err(), "double delete should fail");
}

// ── E3.3: Pairing type usage from cloud-sync context ────────────────

#[test]
fn pairing_domain_creation_and_lifecycle() {
    use nexus_cloud_sync::cloud_domain::pairing::{Pairing, PairingSource};

    let pairing = Pairing::new("pair_1", "ctr_42", "usr_42", PairingSource::AutoCli);

    assert_eq!(pairing.pairing_id, "pair_1");
    assert_eq!(pairing.creator_id, "ctr_42");
    assert_eq!(pairing.user_id, "usr_42");
    assert_eq!(pairing.pairing_source, "auto_cli");
    assert_eq!(pairing.status, "active");
    assert!(pairing.revoked_at.is_none());
}

#[test]
fn pairing_revocation() {
    use nexus_cloud_sync::cloud_domain::pairing::{Pairing, PairingSource};

    let mut pairing = Pairing::new("pair_2", "ctr_42", "usr_42", PairingSource::ManualWeb);

    pairing.revoke().expect("revoke should succeed");
    assert_eq!(pairing.status, "revoked");
    assert!(pairing.revoked_at.is_some());

    // Double revoke fails
    assert!(pairing.revoke().is_err());
}

#[test]
fn pairing_different_sources() {
    use nexus_cloud_sync::cloud_domain::pairing::PairingSource;

    assert_eq!(PairingSource::AutoCli.as_str(), "auto_cli");
    assert_eq!(PairingSource::ManualWeb.as_str(), "manual_web");
    assert_eq!(PairingSource::PlatformAuto.as_str(), "platform_auto");
}

// ── E3.4: Cloud-domain contract roundtrip via cloud-sync ────────────

#[test]
fn user_contract_roundtrip_via_cloud_sync_context() {
    let domain_user = nexus_cloud_sync::cloud_domain::user::User::register(
        "usr_rt",
        "dave",
        "d@example.com",
        "Dave",
    );

    // Convert domain → contract
    let contract_user: nexus_contracts::User = domain_user
        .clone()
        .try_into()
        .expect("domain → contract conversion");

    assert_eq!(contract_user.user_id, "usr_rt");
    assert_eq!(contract_user.username, "dave");

    // Convert contract → domain
    let back: nexus_cloud_sync::cloud_domain::user::User = contract_user.into();
    assert_eq!(back.user_id, domain_user.user_id);
    assert_eq!(back.username, domain_user.username);
    assert_eq!(back.email, domain_user.email);
}

// ── E3.5: Cargo.toml dependency assertion ───────────────────────────

#[test]
fn cloud_sync_manifest_depends_on_cloud_domain() {
    let manifest = include_str!("../Cargo.toml");
    assert!(
        manifest.contains("nexus-cloud-domain"),
        "nexus-cloud-sync Cargo.toml must declare nexus-cloud-domain as a dependency"
    );
}
