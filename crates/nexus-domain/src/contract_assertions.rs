//! Contract Integration Tests
//!
//! Compile-time type assertions and serde roundtrip tests verifying
//! alignment between domain types and nexus-contracts generated types.

use crate::creator::*;
use crate::fork_branch::*;
use crate::key_block::*;
use crate::manuscript_state::*;
use crate::memory_item::*;
use crate::pairing::*;
use crate::reference_source::*;
use crate::source_anchor::*;
use crate::story_manifest::*;
use crate::timeline_event::*;
use crate::user::User;
use crate::world::*;
use crate::world_membership::*;
use crate::{BlockType, MemoryType, TimePolicy, Visibility};

// ── Compile-time assertions ────────────────────────────────────────────

/// Compile-time assertion: SourceAnchor domain ↔ contract conversion exists.
fn _assert_source_anchor_conversion() {
    fn domain_to_contract(domain: SourceAnchor) -> nexus_contracts::SourceAnchor {
        nexus_contracts::SourceAnchor::from(domain)
    }
    fn contract_to_domain(contract: nexus_contracts::SourceAnchor) -> SourceAnchor {
        SourceAnchor::from(contract)
    }
    let _ = (domain_to_contract, contract_to_domain);
}

/// Compile-time assertion: KeyBlock domain ↔ contract conversion exists.
fn _assert_keyblock_conversion() {
    fn domain_to_contract(domain: KeyBlock) -> nexus_contracts::KeyBlock {
        nexus_contracts::KeyBlock::from(domain)
    }
    fn contract_to_domain(contract: nexus_contracts::KeyBlock) -> KeyBlock {
        KeyBlock::from(contract)
    }
    let _ = (domain_to_contract, contract_to_domain);
}

/// Compile-time assertion: Creator domain ↔ contract conversion exists.
fn _assert_creator_conversion() {
    fn domain_to_contract(
        domain: Creator,
    ) -> Result<nexus_contracts::Creator, nexus_creator::errors::CreatorError> {
        domain.try_into()
    }
    fn contract_to_domain(contract: nexus_contracts::Creator) -> Creator {
        Creator::from(contract)
    }
    let _ = (domain_to_contract, contract_to_domain);
}

/// Compile-time assertion: Pairing domain ↔ contract conversion exists.
fn _assert_pairing_conversion() {
    fn domain_to_contract(domain: Pairing) -> nexus_contracts::Pairing {
        nexus_contracts::Pairing::from(domain)
    }
    fn contract_to_domain(contract: nexus_contracts::Pairing) -> Pairing {
        Pairing::from(contract)
    }
    let _ = (domain_to_contract, contract_to_domain);
}

/// Compile-time assertion: World domain ↔ contract conversion exists.
fn _assert_world_conversion() {
    fn domain_to_contract(domain: World) -> nexus_contracts::World {
        nexus_contracts::World::from(domain)
    }
    fn contract_to_domain(contract: nexus_contracts::World) -> World {
        World::from(contract)
    }
    let _ = (domain_to_contract, contract_to_domain);
}

/// Compile-time assertion: WorldMembership domain ↔ contract conversion exists.
fn _assert_world_membership_conversion() {
    fn domain_to_contract(domain: WorldMembership) -> nexus_contracts::WorldMembership {
        nexus_contracts::WorldMembership::from(domain)
    }
    fn contract_to_domain(contract: nexus_contracts::WorldMembership) -> WorldMembership {
        WorldMembership::from(contract)
    }
    let _ = (domain_to_contract, contract_to_domain);
}

/// Compile-time assertion: TimelineEvent domain ↔ contract conversion exists.
fn _assert_timeline_event_conversion() {
    fn domain_to_contract(domain: TimelineEvent) -> nexus_contracts::TimelineEvent {
        nexus_contracts::TimelineEvent::from(domain)
    }
    fn contract_to_domain(contract: nexus_contracts::TimelineEvent) -> TimelineEvent {
        TimelineEvent::from(contract)
    }
    let _ = (domain_to_contract, contract_to_domain);
}

/// Compile-time assertion: ForkBranch domain ↔ contract conversion exists.
fn _assert_fork_branch_conversion() {
    fn domain_to_contract(domain: ForkBranch) -> nexus_contracts::ForkBranch {
        nexus_contracts::ForkBranch::from(domain)
    }
    fn contract_to_domain(contract: nexus_contracts::ForkBranch) -> ForkBranch {
        ForkBranch::from(contract)
    }
    let _ = (domain_to_contract, contract_to_domain);
}

/// Compile-time assertion: StoryManifest domain ↔ contract conversion exists.
fn _assert_story_manifest_conversion() {
    fn domain_to_contract(domain: StoryManifest) -> nexus_contracts::StoryManifest {
        nexus_contracts::StoryManifest::from(domain)
    }
    fn contract_to_domain(contract: nexus_contracts::StoryManifest) -> StoryManifest {
        StoryManifest::from(contract)
    }
    let _ = (domain_to_contract, contract_to_domain);
}

/// Compile-time assertion: MemoryItem domain ↔ contract conversion exists.
fn _assert_memory_item_conversion() {
    fn domain_to_contract(domain: MemoryItem) -> nexus_contracts::Memory {
        nexus_contracts::Memory::from(domain)
    }
    fn contract_to_domain(contract: nexus_contracts::Memory) -> MemoryItem {
        MemoryItem::from(contract)
    }
    let _ = (domain_to_contract, contract_to_domain);
}

/// Compile-time assertion: ReferenceSource domain ↔ contract conversion exists.
fn _assert_reference_source_conversion() {
    fn domain_to_contract(
        domain: ReferenceSource,
    ) -> nexus_contracts::local::domain::ReferenceSource {
        nexus_contracts::local::domain::ReferenceSource::from(domain)
    }
    fn contract_to_domain(
        contract: nexus_contracts::local::domain::ReferenceSource,
    ) -> ReferenceSource {
        ReferenceSource::from(contract)
    }
    let _ = (domain_to_contract, contract_to_domain);
}

/// Compile-time assertion: ManuscriptState domain ↔ contract conversion exists.
fn _assert_manuscript_state_conversion() {
    fn domain_to_contract(
        domain: ManuscriptState,
    ) -> nexus_contracts::local::domain::ManuscriptState {
        nexus_contracts::local::domain::ManuscriptState::from(domain)
    }
    fn contract_to_domain(
        contract: nexus_contracts::local::domain::ManuscriptState,
    ) -> ManuscriptState {
        ManuscriptState::from(contract)
    }
    let _ = (domain_to_contract, contract_to_domain);
}

// ── Enum variant count assertions ──────────────────────────────────────

#[test]
fn test_block_type_count_matches_schema() {
    // Schema has 8 BlockType variants
    let expected_variants = [
        "character",
        "ability",
        "scene",
        "organization",
        "item",
        "conflict",
        "info_point",
        "event",
    ];
    for v in &expected_variants {
        let json = format!("\"{}\"", v);
        let bt: nexus_contracts::BlockType = serde_json::from_str(&json).unwrap();
        assert_eq!(serde_json::to_string(&bt).unwrap(), json);
    }
}

#[test]
fn test_memory_type_count_matches_schema() {
    let expected_variants = ["canon", "working", "experience"];
    for v in &expected_variants {
        let json = format!("\"{}\"", v);
        let mt: nexus_contracts::MemoryType = serde_json::from_str(&json).unwrap();
        assert_eq!(serde_json::to_string(&mt).unwrap(), json);
    }
}

#[test]
fn test_manuscript_phase_count_matches_schema() {
    let expected_variants = ["brainstorm", "draft", "review", "finalize", "published"];
    for v in &expected_variants {
        let json = format!("\"{}\"", v);
        let mp: nexus_contracts::ManuscriptPhase = serde_json::from_str(&json).unwrap();
        assert_eq!(serde_json::to_string(&mp).unwrap(), json);
    }
}

// ── Serde roundtrip tests ─────────────────────────────────────────────

#[test]
fn test_keyblock_domain_contract_roundtrip() {
    let domain_kb = KeyBlock::new("wld_test", BlockType::Event, "Test Event");
    let contract_kb: nexus_contracts::KeyBlock = nexus_contracts::KeyBlock::from(domain_kb);
    assert_eq!(contract_kb.block_type, nexus_contracts::BlockType::Event);
    assert_eq!(
        contract_kb.status,
        nexus_contracts::KeyBlockStatus::Provisional
    );
    assert!(contract_kb.key_block_id.starts_with("kb_"));
}

#[test]
fn test_creator_domain_contract_roundtrip() {
    let domain_creator = Creator::register("ctr_test", "Test", RegistrationSource::Cli, false);
    let contract_creator: nexus_contracts::Creator = domain_creator.try_into().unwrap();
    assert_eq!(
        contract_creator.registration_source,
        nexus_contracts::RegistrationSource::Cli
    );
    assert_eq!(
        contract_creator.status,
        nexus_contracts::CreatorStatus::Active
    );
}

#[test]
fn test_memory_item_domain_contract_roundtrip() {
    let mut domain_mi = MemoryItem::new("ctr_test", "wld_test", MemoryType::Canon, Some("generic"));
    domain_mi.set_summary("Test summary");
    let contract_mi: nexus_contracts::Memory = nexus_contracts::Memory::from(domain_mi);
    assert_eq!(contract_mi.memory_type, nexus_contracts::MemoryType::Canon);
    assert_eq!(contract_mi.summary.as_deref(), Some("Test summary"));
}

#[test]
fn test_fork_branch_domain_contract_roundtrip() {
    let domain_fb = ForkBranch::fork_from("wld_child", "wld_parent", "fbk_root", "evt_1", "ctr_1");
    let contract_fb: nexus_contracts::ForkBranch = nexus_contracts::ForkBranch::from(domain_fb);
    assert_eq!(
        contract_fb.status,
        nexus_contracts::ForkBranchStatus::Active
    );
    assert_eq!(
        contract_fb.verification_status,
        nexus_contracts::VerificationStatus::Unverified
    );
}

/// TD-7: `parent_branch_id` and `forked_from_event_id` must match across domain, contracts, and
/// `schemas/domain/fork-branch.schema.json` (verified manually in knowledge doc).
#[test]
fn test_fork_branch_parent_branch_and_event_ids_roundtrip() {
    let domain_fb = ForkBranch::fork_from(
        "wld_child",
        "wld_parent",
        "fbk_parent_branch_99",
        "evt_fork_point_abc",
        "ctr_owner",
    );
    let contract_fb: nexus_contracts::ForkBranch =
        nexus_contracts::ForkBranch::from(domain_fb.clone());
    assert_eq!(contract_fb.parent_branch_id, "fbk_parent_branch_99");
    assert_eq!(contract_fb.forked_from_event_id, "evt_fork_point_abc");

    let back = ForkBranch::from(contract_fb);
    assert_eq!(back.parent_branch_id, "fbk_parent_branch_99");
    assert_eq!(back.forked_from_event_id, "evt_fork_point_abc");
    assert_eq!(back.world_id, domain_fb.world_id);
    assert_eq!(back.parent_world_id, domain_fb.parent_world_id);
}

#[test]
fn test_all_aggregates_have_schema_version_1() {
    let kb = KeyBlock::new("wld_test", BlockType::Character, "Hero");
    assert_eq!(kb.schema_version, 1);

    let cr = Creator::register("ctr_test", "Test", RegistrationSource::Cli, false);
    assert_eq!(cr.schema_version, 1);

    let pr = Pairing::new("prg_test", "ctr_test", "usr_test", PairingSource::AutoCli);
    assert_eq!(pr.schema_version, 1);

    let w = World::new(
        "wld_test",
        "ctr_test",
        "Test",
        "test",
        Visibility::Private,
        TimePolicy::Manual,
    );
    assert_eq!(w.schema_version, 1);

    let te = TimelineEvent::new("wld_test", "fbk_root", TimelineEventType::StoryAdvance, 1);
    assert_eq!(te.schema_version, 1);

    let fb = ForkBranch::fork_from("wld_child", "wld_parent", "fbk_root", "evt_1", "ctr_1");
    assert_eq!(fb.schema_version, 1);

    let sm = StoryManifest::new(
        "wld_test",
        "ctr_test",
        ManifestType::Chapter,
        "Ch1",
        "sum_1",
    );
    assert_eq!(sm.schema_version, 1);

    let mi = MemoryItem::new("ctr_test", "wld_test", MemoryType::Canon, None);
    assert_eq!(mi.schema_version, 1);

    let rs = ReferenceSource::register(
        "wrk_test",
        ReferenceSourceType::File,
        "file:///test",
        "Test",
    );
    assert_eq!(rs.schema_version, 1);

    let ms = ManuscriptState::new("wrk_test", "wld_test", "ctr_test");
    assert_eq!(ms.schema_version, 1);

    let u = User::register("usr_test", "u_test", "u@example.com", "U Test");
    assert_eq!(u.schema_version, 1);
}

#[test]
fn test_world_domain_contract_roundtrip() {
    let domain_world = World::new(
        "wld_roundtrip",
        "ctr_owner",
        "Roundtrip World",
        "roundtrip-world",
        Visibility::Private,
        TimePolicy::Manual,
    );

    // Domain → Contract
    let contract_world: nexus_contracts::World = nexus_contracts::World::from(domain_world.clone());
    assert_eq!(contract_world.world_id, "wld_roundtrip");
    assert_eq!(contract_world.owner_creator_id, "ctr_owner");
    assert_eq!(contract_world.title, "Roundtrip World");
    assert_eq!(contract_world.slug, "roundtrip-world");
    assert_eq!(contract_world.status, nexus_contracts::WorldStatus::Active);
    assert_eq!(
        contract_world.visibility,
        nexus_contracts::Visibility::Private
    );
    assert_eq!(
        contract_world.time_policy,
        nexus_contracts::TimePolicy::Manual
    );
    assert_eq!(contract_world.schema_version, 1);
    assert_eq!(contract_world.canon_revision, Some(0));

    // Contract → Domain
    let roundtrip_domain: World = World::from(contract_world.clone());
    assert_eq!(roundtrip_domain.world_id, domain_world.world_id);
    assert_eq!(
        roundtrip_domain.owner_creator_id,
        domain_world.owner_creator_id
    );
    assert_eq!(roundtrip_domain.title, domain_world.title);
    assert_eq!(roundtrip_domain.slug, domain_world.slug);
    assert_eq!(roundtrip_domain.visibility, domain_world.visibility);
    assert_eq!(roundtrip_domain.time_policy, domain_world.time_policy);
    assert_eq!(roundtrip_domain.schema_version, domain_world.schema_version);
}

#[test]
fn test_world_membership_domain_contract_roundtrip() {
    let domain_membership =
        WorldMembership::new("wld_roundtrip", "ctr_owner", MembershipRole::Owner);

    // Domain → Contract
    let contract_membership: nexus_contracts::WorldMembership =
        nexus_contracts::WorldMembership::from(domain_membership.clone());
    assert_eq!(contract_membership.world_id, "wld_roundtrip");
    assert_eq!(contract_membership.creator_id, "ctr_owner");
    assert_eq!(
        contract_membership.role,
        nexus_contracts::MembershipRole::Owner
    );
    assert_eq!(
        contract_membership.membership_status,
        nexus_contracts::MembershipStatus::Active
    );
    assert_eq!(contract_membership.schema_version, 1);
    assert!(contract_membership.permissions.is_some());

    // Contract → Domain
    let roundtrip_domain: WorldMembership = WorldMembership::from(contract_membership.clone());
    assert_eq!(roundtrip_domain.world_id, domain_membership.world_id);
    assert_eq!(roundtrip_domain.creator_id, domain_membership.creator_id);
    assert_eq!(roundtrip_domain.role, domain_membership.role);
    assert_eq!(
        roundtrip_domain.membership_status,
        domain_membership.membership_status
    );
    assert_eq!(
        roundtrip_domain.schema_version,
        domain_membership.schema_version
    );

    // NOTE: MembershipRole enum values are now aligned with v1-spec §5.4, §7.
    // Both schema and domain use: Owner, Maintainer, Collaborator, OfficialCreator.
    // This alignment was completed in DM-R5 resolution.
}

#[test]
fn test_user_domain_contract_roundtrip() {
    let domain_user = User::register("usr_roundtrip", "roundy", "r@example.com", "Round Trip");

    let contract_user: nexus_contracts::User =
        nexus_contracts::User::try_from(domain_user.clone()).unwrap();
    assert_eq!(contract_user.user_id, "usr_roundtrip");
    assert_eq!(contract_user.username, "roundy");
    assert_eq!(
        contract_user.account_status,
        nexus_contracts::AccountStatus::Active
    );
    assert_eq!(
        contract_user.subscription_tier,
        nexus_contracts::SubscriptionTier::Free
    );
    assert_eq!(contract_user.schema_version, 1);

    let roundtrip: User = User::from(contract_user);
    assert_eq!(roundtrip.user_id, domain_user.user_id);
    assert_eq!(roundtrip.username, domain_user.username);
    assert_eq!(roundtrip.account_status, domain_user.account_status);
}
