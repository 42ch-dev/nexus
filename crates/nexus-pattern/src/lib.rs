//! Read-only pattern package registry.
//!
//! A package is one directory holding a strict `pattern.toml` manifest and a `workflow.ir.json`
//! fragment. A scan reports every package directory with the reason it is unavailable; one broken
//! package never hides its peers, and nothing here writes to disk. Presence in the listing is a
//! local readability fact, never publication, provenance or trust.
//!
//! Symlinked package directories are followed; entries that are not directories, broken symlinks
//! included, are skipped. `package_dir` is a directory name's cursor identity, not its lossy
//! rendering: valid UTF-8 is kept verbatim and `%` and every byte outside valid UTF-8 are
//! percent-escaped, so two distinct directories never share one identity and every entry stays
//! addressable by its own cursor. A name that is not UTF-8 is reported as unavailable instead of
//! being loaded. Availability means the manifest was readable and valid, the fragment parsed and its
//! content hash was computed; it does not mean the fragment is structurally valid, which is the
//! validate and compose stage's check.
//!
//! Only a regular `pattern.toml` and `workflow.ir.json` no larger than [`MAX_MEMBER_BYTES`] are
//! read, so a FIFO, a device node or an oversized member is reported as one unavailable package
//! instead of stalling or exhausting the scan.

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::path::Path;

use nexus_contracts::generated::daemon_api::patterns::pattern_manifest::{
    NexusPatternSlotKind as ManifestSlotKind, PatternManifest, PatternManifestEffectClass,
};
use nexus_contracts::generated::daemon_api::patterns::pattern_summary::{
    NexusPatternSlot as SummarySlot, NexusPatternSlotKind as SummarySlotKind, PatternSummary,
    PatternSummaryEffectClass,
};
use nexus_workflow_ir::{
    effect_summary, CanonicalHashError, WorkflowDocument, WorkflowDocumentEffectSummary,
};

/// Manifest file every package directory carries.
pub const MANIFEST_FILE: &str = "pattern.toml";
/// IR fragment file every package directory carries.
pub const FRAGMENT_FILE: &str = "workflow.ir.json";
/// Largest member file the reader loads.
///
/// A manifest is a few hundred bytes and a fragment a few kilobytes; the ceiling is what keeps a
/// device member or a runaway file from exhausting the daemon while it lists.
pub const MAX_MEMBER_BYTES: u64 = 1 << 20;

/// One discovered package directory.
#[derive(Debug, Clone)]
pub struct PatternPackage {
    /// Directory name as a cursor identity: valid UTF-8 verbatim, with `%` and every byte outside
    /// valid UTF-8 percent-escaped; never an absolute path.
    pub package_dir: String,
    /// Manifest parsed from `pattern.toml`; absent only when the manifest itself could not be read
    /// or parsed, so an unavailable package can still be named by id and version.
    pub manifest: Option<PatternManifest>,
    /// Fragment parsed from `workflow.ir.json`; present only when the package is available.
    pub fragment: Option<WorkflowDocument>,
    /// Canonical content hash of the fragment document — the pin staging approval compares.
    /// Manifest metadata is not part of it. Present only when the package is available.
    pub content_hash: Option<String>,
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
            // The fragment's canonical hash: content identity, so a manifest-only edit keeps it.
            content_hash: package.content_hash.clone(),
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
/// Entries are ordered by `package_dir` ascending as UTF-8 bytes. Hidden and `_`-prefixed
/// directories are skipped, matching the preset scan convention; a missing root directory is an
/// empty listing. `ir_version` is the daemon's current workflow IR version, checked against each
/// manifest's `ir_compat` range.
///
/// # Errors
///
/// Returns the underlying I/O error when the root directory cannot be enumerated. A directory entry
/// that fails mid-iteration is skipped, and a package that cannot be read is reported as unavailable
/// instead.
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
        let Ok(entry) = entry else { continue };
        if !entry.path().is_dir() {
            continue;
        }
        if let Some(package) = package_entry(&entry.path(), entry.file_name(), ir_version) {
            packages.push(package);
        }
    }
    packages.sort_by(|left, right| left.package_dir.cmp(&right.package_dir));
    Ok(packages)
}

/// Classify one scanned directory entry; `None` when the discovery convention skips it.
fn package_entry(directory: &Path, file_name: OsString, ir_version: u64) -> Option<PatternPackage> {
    // Lossy conversion preserves a leading ASCII byte, so the skip prefix is judged on it either way.
    let lossy = file_name.to_string_lossy();
    if lossy.starts_with('.') || lossy.starts_with('_') {
        return None;
    }
    let package_dir = package_dir_name(&file_name);
    if file_name.to_str().is_none() {
        // The directory cannot be loaded under a name that is not UTF-8; its escaped identity keeps
        // it addressable instead of colliding with a sibling that renders the same way.
        return Some(PatternPackage {
            package_dir,
            manifest: None,
            fragment: None,
            content_hash: None,
            reason: Some(Unavailable::NotUtf8Name(file_name).to_string()),
        });
    }
    Some(read_package(directory, package_dir, ir_version))
}

/// Cursor identity of one directory name: valid UTF-8 verbatim, with `%` and every byte outside
/// valid UTF-8 percent-escaped.
///
/// A lossy rendering is not an identity — distinct names collapse to the same replacement characters,
/// so a cursor keyed on it cannot separate them. Escaping `%` as well keeps the mapping injective
/// across both branches: a directory literally named `a%FF` and one holding the byte `0xFF` stay
/// distinct.
fn package_dir_name(file_name: &OsStr) -> String {
    use std::fmt::Write as _;

    let bytes = file_name.as_encoded_bytes();
    let mut identity = String::with_capacity(bytes.len());
    for chunk in bytes.utf8_chunks() {
        identity.push_str(&chunk.valid().replace('%', "%25"));
        for byte in chunk.invalid() {
            write!(identity, "%{byte:02X}").expect("writing to a String cannot fail");
        }
    }
    identity
}

fn read_package(directory: &Path, package_dir: String, ir_version: u64) -> PatternPackage {
    let manifest: PatternManifest = match read_text(directory, MANIFEST_FILE)
        .and_then(|text| toml::from_str(&text).map_err(Unavailable::Manifest))
    {
        Ok(manifest) => manifest,
        Err(reason) => {
            return PatternPackage {
                package_dir,
                manifest: None,
                fragment: None,
                content_hash: None,
                reason: Some(reason.to_string()),
            };
        }
    };

    match load(directory, &manifest, &package_dir, ir_version) {
        Ok((fragment, content_hash)) => PatternPackage {
            package_dir,
            manifest: Some(manifest),
            fragment: Some(fragment),
            content_hash: Some(content_hash),
            reason: None,
        },
        Err(reason) => PatternPackage {
            package_dir,
            manifest: Some(manifest),
            fragment: None,
            content_hash: None,
            reason: Some(reason.to_string()),
        },
    }
}

fn load(
    directory: &Path,
    manifest: &PatternManifest,
    package_dir: &str,
    ir_version: u64,
) -> Result<(WorkflowDocument, String), Unavailable> {
    check_manifest(manifest, package_dir, ir_version)?;

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
    // The pin is the fragment document's own canonical hash, never the preset import path's
    // `content_hash` field (which is a constant `None` there).
    let content_hash = nexus_workflow_ir::content_hash(&fragment).map_err(Unavailable::Hash)?;
    Ok((fragment, content_hash))
}

/// Manifest rules the generated type cannot express: cross-field identity, IR compatibility and
/// per-package uniqueness. The manifest schema's `version` pattern is the single authority over the
/// `SemVer` 2.0.0 label; no second parser narrows it here.
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
    let path = directory.join(file);
    // Metadata is taken through symlinks, so a link to a FIFO or a device node is refused here too.
    // The gate is what keeps this synchronous scan from blocking on a member that never reaches EOF
    // or from loading a runaway file. A writer that swaps the member between this call and the open
    // below can still stall it; who may write the registry is undefined in P3, so the gate guards
    // the accidental member a scan actually meets, not a hostile one.
    let metadata = std::fs::metadata(&path).map_err(|source| Unavailable::Read { file, source })?;
    if !metadata.is_file() {
        return Err(Unavailable::NotRegularFile { file });
    }
    let bytes = metadata.len();
    if bytes > MAX_MEMBER_BYTES {
        return Err(Unavailable::Oversized {
            file,
            bytes,
            limit: MAX_MEMBER_BYTES,
        });
    }
    std::fs::read_to_string(&path).map_err(|source| Unavailable::Read { file, source })
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
    #[error("`{file}` is not a regular file")]
    NotRegularFile { file: &'static str },
    #[error("`{file}` is {bytes} bytes, above the {limit}-byte member ceiling")]
    Oversized {
        file: &'static str,
        bytes: u64,
        limit: u64,
    },
    #[error("`pattern.toml` is malformed: {0}")]
    Manifest(toml::de::Error),
    #[error("package directory name {0:?} is not UTF-8")]
    NotUtf8Name(OsString),
    #[error("`workflow.ir.json` is malformed: {0}")]
    Fragment(serde_json::Error),
    #[error("cannot compute the fragment content hash: {0}")]
    Hash(CanonicalHashError),
    #[error("manifest id `{id}` does not match package directory `{package_dir}`")]
    IdMismatch { id: String, package_dir: String },
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

#[cfg(all(test, unix))]
mod tests {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::path::Path;

    use super::package_entry;

    /// APFS rejects these names, so the classification is exercised without a filesystem.
    #[test]
    fn non_utf8_directory_names_stay_distinct_and_unavailable() {
        let entries: Vec<_> = [b"lossy-\xFF".to_vec(), b"lossy-\xFE".to_vec()]
            .into_iter()
            .map(|bytes| {
                package_entry(Path::new("unused"), OsString::from_vec(bytes), 0)
                    .expect("a non-UTF-8 name is reported, not skipped")
            })
            .collect();

        assert_eq!(entries[0].package_dir, "lossy-%FF");
        assert_eq!(entries[1].package_dir, "lossy-%FE");
        for entry in &entries {
            assert!(!entry.available());
            assert!(entry.manifest.is_none());
            assert!(
                matches!(entry.reason.as_deref(), Some(reason) if reason.contains("is not UTF-8")),
                "{:?}",
                entry.reason
            );
        }
        assert_ne!(entries[0].reason, entries[1].reason);
    }

    #[test]
    fn a_literal_percent_name_does_not_collide_with_an_escaped_one() {
        let literal = OsString::from("lossy-%FF");
        let escaped = OsString::from_vec(b"lossy-\xFF".to_vec());

        let literal = package_entry(Path::new("unused"), literal, 0).expect("a name is reported");
        let escaped = package_entry(Path::new("unused"), escaped, 0).expect("a name is reported");

        assert_eq!(literal.package_dir, "lossy-%25FF");
        assert_eq!(escaped.package_dir, "lossy-%FF");
        assert_ne!(literal.package_dir, escaped.package_dir);
    }

    #[test]
    fn hidden_non_utf8_directory_names_are_skipped() {
        for name in [b".hidden-\xFF".to_vec(), b"_hidden-\xFF".to_vec()] {
            assert!(package_entry(Path::new("unused"), OsString::from_vec(name), 0).is_none());
        }
    }
}
