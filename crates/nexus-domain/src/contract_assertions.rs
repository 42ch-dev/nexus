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
    fn domain_to_contract(domain: Creator) -> nexus_contracts::Creator {
        nexus_contracts::Creator::from(domain)
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
    fn domain_to_contract(domain: ReferenceSource) -> nexus_contracts::ReferenceSource {
        nexus_contracts::ReferenceSource::from(domain)
    }
    fn contract_to_domain(contract: nexus_contracts::ReferenceSource) -> ReferenceSource {
        ReferenceSource::from(contract)
    }
    let _ = (domain_to_contract, contract_to_domain);
}

/// Compile-time assertion: ManuscriptState domain ↔ contract conversion exists.
fn _assert_manuscript_state_conversion() {
    fn domain_to_contract(domain: ManuscriptState) -> nexus_contracts::ManuscriptState {
        nexus_contracts::ManuscriptState::from(domain)
    }
    fn contract_to_domain(contract: nexus_contracts::ManuscriptState) -> ManuscriptState {
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
    let contract_creator: nexus_contracts::Creator = nexus_contracts::Creator::from(domain_creator);
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
