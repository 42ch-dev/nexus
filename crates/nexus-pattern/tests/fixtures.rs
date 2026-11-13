//! The committed fixture packages, read from the checkout rather than a temporary root.
//!
//! `tests/fixtures/patterns` is a registry root that ships with the crate: two real fragments
//! (one prompt step, one step-then-judge) and one deliberately malformed manifest. The service
//! end-to-end test copies this directory into a disposable home, so these bytes are the ones a
//! daemon actually lists.

use std::path::PathBuf;

use nexus_contracts::generated::daemon_api::patterns::pattern_summary::{
    NexusPatternSlotKind, PatternSummary, PatternSummaryEffectClass,
};
use nexus_pattern::{scan_pattern_packages, PatternPackage};
use nexus_workflow_ir::IR_VERSION;

/// The committed registry root.
fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/patterns")
}

/// Pins of the fixture fragments. Editing a fragment byte without updating its constant fails
/// this test, which is what keeps a pin from drifting silently under an unchanged version.
const PROMPT_STEP_HASH: &str = "b89e9707bf2d39e27bf825f74c8bfac37acbdf1f7109614d0942fb8613212a04";
const STEP_WITH_JUDGE_HASH: &str =
    "1d62b2b1bb02390d704ce10864b327b2120aed59ebe0ba482fe824347404324e";

fn summary(packages: &[PatternPackage], package_dir: &str) -> PatternSummary {
    let package = packages
        .iter()
        .find(|package| package.package_dir() == package_dir)
        .unwrap_or_else(|| panic!("`{package_dir}` is committed under the fixture root"));
    PatternSummary::from(package)
}

#[test]
fn the_committed_packages_load_with_their_pins() {
    let packages = scan_pattern_packages(&fixture_root(), IR_VERSION).expect("fixture root scans");
    assert_eq!(
        packages
            .iter()
            .map(PatternPackage::package_dir)
            .collect::<Vec<_>>(),
        ["broken-manifest", "prompt-step", "step-with-judge"],
    );

    let prompt_step = summary(&packages, "prompt-step");
    assert!(prompt_step.available, "{:?}", prompt_step.reason);
    assert_eq!(prompt_step.reason, None);
    assert_eq!(prompt_step.id.as_deref(), Some("prompt-step"));
    assert_eq!(prompt_step.version.as_deref(), Some("1.0.0"));
    assert_eq!(
        prompt_step.description.as_deref(),
        Some("Draft an answer to a topic with one prompt step")
    );
    assert_eq!(
        prompt_step.effect_class,
        Some(PatternSummaryEffectClass::External)
    );
    assert_eq!(prompt_step.requires_capabilities, ["acp.prompt"]);
    assert_eq!(prompt_step.slots.len(), 1);
    assert_eq!(prompt_step.slots[0].name.as_str(), "topic");
    assert_eq!(prompt_step.slots[0].kind, NexusPatternSlotKind::String);
    assert!(prompt_step.slots[0].required);
    assert_eq!(prompt_step.content_hash.as_deref(), Some(PROMPT_STEP_HASH));

    let step_with_judge = summary(&packages, "step-with-judge");
    assert!(step_with_judge.available, "{:?}", step_with_judge.reason);
    assert_eq!(step_with_judge.id.as_deref(), Some("step-with-judge"));
    assert_eq!(
        step_with_judge.effect_class,
        Some(PatternSummaryEffectClass::External)
    );
    assert_eq!(
        step_with_judge.requires_capabilities,
        ["acp.prompt", "judge.llm"]
    );
    assert_eq!(
        step_with_judge
            .slots
            .iter()
            .map(|slot| (slot.name.as_str(), slot.kind, slot.required))
            .collect::<Vec<_>>(),
        [
            ("topic", NexusPatternSlotKind::String, true),
            ("rubric", NexusPatternSlotKind::Asset, false),
        ]
    );
    assert_eq!(
        step_with_judge.content_hash.as_deref(),
        Some(STEP_WITH_JUDGE_HASH)
    );

    // The malformed manifest is its own unavailable item and leaves both peers available.
    let broken = summary(&packages, "broken-manifest");
    assert!(!broken.available);
    assert_eq!(broken.id, None);
    assert_eq!(broken.version, None);
    assert_eq!(broken.content_hash, None);
    assert_eq!(broken.effect_class, None);
    assert_eq!(broken.slots.len(), 0);
    assert_eq!(broken.requires_capabilities.len(), 0);
    let reason = broken
        .reason
        .expect("an unavailable item carries its reason");
    assert!(reason.contains("`pattern.toml` is malformed"), "{reason}");
}
