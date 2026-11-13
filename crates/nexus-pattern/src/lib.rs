//! Read-only pattern package registry.
//!
//! A package is one directory holding a strict `pattern.toml` manifest and a `workflow.ir.json`
//! fragment. A scan reports every package directory with the reason it is unavailable; one broken
//! package never hides its peers, and nothing here writes to disk. Presence in the listing is a
//! local readability fact, never publication, provenance or trust.

use std::collections::HashSet;
use std::path::Path;

use nexus_contracts::generated::daemon_api::patterns::pattern_manifest::{
    NexusPatternSlotKind as ManifestSlotKind, PatternManifest, PatternManifestEffectClass,
};
use nexus_contracts::generated::daemon_api::patterns::pattern_summary::{
    NexusPatternSlot as SummarySlot, NexusPatternSlotKind as SummarySlotKind, PatternSummary,
    PatternSummaryEffectClass,
};
use nexus_workflow_ir::{effect_summary, WorkflowDocument, WorkflowDocumentEffectSummary};

/// Manifest file every package directory carries.
pub const MANIFEST_FILE: &str = "pattern.toml";
/// IR fragment file every package directory carries.
pub const FRAGMENT_FILE: &str = "workflow.ir.json";

/// One discovered package directory.
#[derive(Debug, Clone)]
pub struct PatternPackage {
    /// Directory basename; never an absolute path.
    pub package_dir: String,
    /// Manifest parsed from `pattern.toml`; present only when the package is available.
    pub manifest: Option<PatternManifest>,
    /// Fragment parsed from `workflow.ir.json`; present only when the package is available.
    pub fragment: Option<WorkflowDocument>,
    /// Why the package did not load; `None` only when the package is available.
    pub reason: Option<String>,
}

impl PatternPackage {
    /// Whether the package loaded completely.
    #[must_use]
    pub const fn available(&self) -> bool {
        self.reason.is_none()
    }
}

impl From<&PatternPackage> for PatternSummary {
    fn from(package: &PatternPackage) -> Self {
        let manifest = package.manifest.as_ref();
        Self {
            available: package.available(),
            // A fragment pin is content identity; this reader performs no hashing, so an available
            // package carries no pin here.
            content_hash: None,
            description: manifest.map(|manifest| manifest.description.as_str().to_owned()),
            effect_class: manifest.map(|manifest| summary_effect(manifest.effect_class)),
            id: manifest.map(|manifest| manifest.id.as_str().to_owned()),
            package_dir: package.package_dir.clone(),
            reason: package.reason.clone(),
            requires_capabilities: manifest.map_or_else(Vec::new, |manifest| {
                manifest
                    .requires_capabilities
                    .iter()
                    .map(|capability| capability.as_str().to_owned())
                    .collect()
            }),
            slots: manifest.map_or_else(Vec::new, summary_slots),
            version: manifest.map(|manifest| manifest.version.as_str().to_owned()),
        }
    }
}

/// Read every package under `patterns_dir`.
///
/// Entries are ordered by `package_dir`. Hidden and `_`-prefixed directories are skipped, matching
/// the preset scan convention; a missing root directory is an empty listing. `ir_version` is the
/// daemon's current workflow IR version, checked against each manifest's `ir_compat` range.
///
/// # Errors
///
/// Returns the underlying I/O error when the root directory exists but cannot be enumerated. A
/// package that cannot be read is reported as unavailable instead.
pub fn scan_pattern_packages(
    patterns_dir: &Path,
    ir_version: u64,
) -> std::io::Result<Vec<PatternPackage>> {
    let entries = match std::fs::read_dir(patterns_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };

    let mut packages = Vec::new();
    for entry in entries {
        let entry = entry?;
        if !entry.path().is_dir() {
            continue;
        }
        let package_dir = entry.file_name().to_string_lossy().into_owned();
        if package_dir.starts_with('.') || package_dir.starts_with('_') {
            continue;
        }
        packages.push(read_package(&entry.path(), package_dir, ir_version));
    }
    packages.sort_by(|left, right| left.package_dir.cmp(&right.package_dir));
    Ok(packages)
}

fn read_package(directory: &Path, package_dir: String, ir_version: u64) -> PatternPackage {
    match load(directory, &package_dir, ir_version) {
        Ok((manifest, fragment)) => PatternPackage {
            package_dir,
            manifest: Some(manifest),
            fragment: Some(fragment),
            reason: None,
        },
        Err(reason) => PatternPackage {
            package_dir,
            manifest: None,
            fragment: None,
            reason: Some(reason.to_string()),
        },
    }
}

type Loaded = Result<(PatternManifest, WorkflowDocument), Unavailable>;

fn load(directory: &Path, package_dir: &str, ir_version: u64) -> Loaded {
    let manifest: PatternManifest =
        toml::from_str(&read_text(directory, MANIFEST_FILE)?).map_err(Unavailable::Manifest)?;
    check_manifest(&manifest, package_dir, ir_version)?;

    let fragment: WorkflowDocument = serde_json::from_str(&read_text(directory, FRAGMENT_FILE)?)
        .map_err(Unavailable::Fragment)?;
    // The rollup covers this document's nodes; a v0 fragment resolves no nest children.
    let rollup = effect_summary(&fragment.nodes);
    if document_effect(manifest.effect_class) != rollup {
        return Err(Unavailable::EffectMismatch {
            declared: manifest.effect_class,
            rollup,
        });
    }
    Ok((manifest, fragment))
}

/// Manifest rules the generated type cannot express: cross-field identity, the reader's own
/// `SemVer` and compatibility checks, and per-package uniqueness.
fn check_manifest(
    manifest: &PatternManifest,
    package_dir: &str,
    ir_version: u64,
) -> Result<(), Unavailable> {
    if manifest.id.as_str() != package_dir {
        return Err(Unavailable::IdMismatch {
            id: manifest.id.as_str().to_owned(),
            package_dir: package_dir.to_owned(),
        });
    }
    semver::Version::parse(manifest.version.as_str())
        .map_err(|_| Unavailable::Version(manifest.version.to_string()))?;

    let (min, max) = (manifest.ir_compat.min, manifest.ir_compat.max);
    if min > max {
        return Err(Unavailable::InvertedCompat { min, max });
    }
    if !(min..=max).contains(&ir_version) {
        return Err(Unavailable::UnsupportedCompat {
            min,
            max,
            ir_version,
        });
    }

    let mut slot_names = HashSet::new();
    for slot in &manifest.slots {
        if slot.name.is_empty() {
            return Err(Unavailable::EmptySlotName);
        }
        if !slot_names.insert(slot.name.as_str()) {
            return Err(Unavailable::DuplicateSlot(slot.name.to_string()));
        }
    }
    let mut capabilities = HashSet::new();
    for capability in &manifest.requires_capabilities {
        if capability.is_empty() {
            return Err(Unavailable::EmptyCapability);
        }
        if !capabilities.insert(capability.as_str()) {
            return Err(Unavailable::DuplicateCapability(capability.to_string()));
        }
    }
    Ok(())
}

fn read_text(directory: &Path, file: &'static str) -> Result<String, Unavailable> {
    std::fs::read_to_string(directory.join(file))
        .map_err(|source| Unavailable::Read { file, source })
}

const fn document_effect(class: PatternManifestEffectClass) -> WorkflowDocumentEffectSummary {
    match class {
        PatternManifestEffectClass::Pure => WorkflowDocumentEffectSummary::Pure,
        PatternManifestEffectClass::Read => WorkflowDocumentEffectSummary::Read,
        PatternManifestEffectClass::WriteCandidate => WorkflowDocumentEffectSummary::WriteCandidate,
        PatternManifestEffectClass::External => WorkflowDocumentEffectSummary::External,
    }
}

const fn summary_effect(class: PatternManifestEffectClass) -> PatternSummaryEffectClass {
    match class {
        PatternManifestEffectClass::Pure => PatternSummaryEffectClass::Pure,
        PatternManifestEffectClass::Read => PatternSummaryEffectClass::Read,
        PatternManifestEffectClass::WriteCandidate => PatternSummaryEffectClass::WriteCandidate,
        PatternManifestEffectClass::External => PatternSummaryEffectClass::External,
    }
}

fn summary_slots(manifest: &PatternManifest) -> Vec<SummarySlot> {
    manifest
        .slots
        .iter()
        .map(|slot| SummarySlot {
            // Both DTOs validate the name as non-empty only; re-parse into the summary's newtype.
            name: slot
                .name
                .as_str()
                .parse()
                .expect("non-empty slot name is a valid summary slot name"),
            kind: summary_slot_kind(slot.kind),
            required: slot.required,
        })
        .collect()
}

const fn summary_slot_kind(kind: ManifestSlotKind) -> SummarySlotKind {
    match kind {
        ManifestSlotKind::String => SummarySlotKind::String,
        ManifestSlotKind::Asset => SummarySlotKind::Asset,
        ManifestSlotKind::Capability => SummarySlotKind::Capability,
    }
}

/// Why one package could not be loaded. Every variant is rendered into the listing's `reason`.
#[derive(Debug, thiserror::Error)]
enum Unavailable {
    #[error("cannot read `{file}`: {source}")]
    Read {
        file: &'static str,
        source: std::io::Error,
    },
    #[error("`pattern.toml` is malformed: {0}")]
    Manifest(toml::de::Error),
    #[error("`workflow.ir.json` is malformed: {0}")]
    Fragment(serde_json::Error),
    #[error("manifest id `{id}` does not match package directory `{package_dir}`")]
    IdMismatch { id: String, package_dir: String },
    #[error("manifest version `{0}` is not SemVer 2.0.0")]
    Version(String),
    #[error("manifest ir_compat range {min}..={max} is inverted")]
    InvertedCompat { min: u64, max: u64 },
    #[error("manifest ir_compat {min}..={max} excludes the daemon ir_version {ir_version}")]
    UnsupportedCompat { min: u64, max: u64, ir_version: u64 },
    #[error("manifest declares an empty slot name")]
    EmptySlotName,
    #[error("manifest declares slot `{0}` more than once")]
    DuplicateSlot(String),
    #[error("manifest declares an empty capability id")]
    EmptyCapability,
    #[error("manifest declares capability `{0}` more than once")]
    DuplicateCapability(String),
    #[error("declared effect_class `{declared}` disagrees with the fragment rollup `{rollup}`")]
    EffectMismatch {
        declared: PatternManifestEffectClass,
        rollup: WorkflowDocumentEffectSummary,
    },
}
