//! Generate Rust wire types from Nexus JSON Schemas using [`typify`].
//!
//! Consumes the **dereferenced** schema tree produced by `tooling/codegen`
//! stage 1 (`schema-prep.ts` → `.schemas-dereferenced/`). Dereferenced schemas
//! are self-contained (no cross-file `$ref`), which is exactly what `typify`
//! requires — unlike the sibling spoke generator, no per-schema `cwd` dance is
//! needed here because there are no bare-relative refs left to resolve.
//!
//! # Env contract (architect-locked)
//!
//! | Var | Default | Purpose |
//! |-----|---------|---------|
//! | `NEXUS_REPO_ROOT` | `CARGO_MANIFEST_DIR/../../..` | Repo root (drives output path + sibling-dir defaults) |
//! | `NEXUS_DEREF_SCHEMAS_DIR` | `<repo>/tooling/codegen/.schemas-dereferenced` | `typify` input tree |
//! | `NEXUS_SRC_SCHEMAS_DIR` | `<repo>/schemas` | Original schemas; used to log the canonical source root |
//!
//! # T1 scope
//!
//! Emits one `.rs` file per non-skipped schema, mirroring the schema tree under
//! `crates/nexus-contracts/src/generated/`. Barrel `mod.rs` files (T2),
//! orchestrator wiring (T3), clippy tuning (T4) and drift reconciliation (T5)
//! are handled by later tasks — the output here is expected to differ in naming
//! and derives from the current hand-tuned generator.

use glob::glob;
use schemars::schema::RootSchema;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process;
use typify::{TypeSpace, TypeSpaceSettings};

/// Schemas excluded from generation — matched against their path relative to the
/// schemas dir (POSIX slashes). Definitions-only files emit no root type, and
/// `bundle-refinement` is a canonical-skip reference schema.
const SKIP_SCHEMAS: &[&str] = &[
    "common/common.schema.json",
    "common/source-anchor.schema.json",
    "platform/sync/bundle-refinement.schema.json",
];

/// Resolve the repository root: `NEXUS_REPO_ROOT` env, else
/// `CARGO_MANIFEST_DIR/../../..` (the monorepo root, from `tooling/codegen/rust-gen/`).
fn repo_root() -> PathBuf {
    if let Ok(root) = env::var("NEXUS_REPO_ROOT") {
        return PathBuf::from(root);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("repo root: CARGO_MANIFEST_DIR/../../.. must resolve to the monorepo root")
}

/// Dereferenced schema tree consumed by `typify` (self-contained schemas).
fn deref_schemas_dir(root: &Path) -> PathBuf {
    env::var("NEXUS_DEREF_SCHEMAS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("tooling/codegen/.schemas-dereferenced"))
}

/// Original `schemas/` tree. The dereferenced tree is the `typify` input; this
/// is resolved only to log the canonical source root for the operator.
fn source_schemas_dir(root: &Path) -> PathBuf {
    env::var("NEXUS_SRC_SCHEMAS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("schemas"))
}

/// Convert a kebab-case path segment to a valid Rust module name (`-` → `_`).
fn to_rust_module_name(segment: &str) -> String {
    segment.replace('-', "_")
}

/// Build the output `.rs` path for a schema, mirroring its tree position with
/// Rust module naming. E.g. `daemon-api/canvas/outline/work-outline.schema.json`
/// → `…/daemon_api/canvas/outline/work_outline.rs`.
fn output_path(out_root: &Path, rel: &Path) -> PathBuf {
    let parent = rel
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| {
            p.components()
                .filter_map(|c| match c {
                    Component::Normal(os) => Some(PathBuf::from(to_rust_module_name(&os.to_string_lossy()))),
                    _ => None,
                })
                .collect::<PathBuf>()
        })
        .unwrap_or_default();

    let stem = rel
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.strip_suffix(".schema").unwrap_or(s))
        .expect("schema file has a stem");

    out_root.join(parent).join(format!("{}.rs", to_rust_module_name(stem)))
}

/// A path relative to the schemas dir, rendered with POSIX separators, for
/// skip-list matching (independent of platform path separators).
fn rel_posix(path: &Path) -> String {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(os) => Some(os.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Generate Rust source for a single schema via `typify` and write it to `out_path`.
///
/// `rel` is the schema's path relative to the schemas dir (POSIX or platform); it
/// is rendered verbatim into the file header as the canonical source pointer.
fn generate_schema_rust(schema_path: &Path, rel: &Path, out_path: &Path) -> Result<(), String> {
    let content = fs::read_to_string(schema_path)
        .map_err(|err| format!("failed to read {}: {err}", schema_path.display()))?;
    let schema: RootSchema = serde_json::from_str(&content)
        .map_err(|err| format!("invalid JSON Schema in {}: {err}", schema_path.display()))?;

    let mut settings = TypeSpaceSettings::default();
    // Mirror the proven spoke setting; T4 may tune derives if clippy requires.
    settings.with_struct_builder(true);

    let mut type_space = TypeSpace::new(&settings);
    type_space
        .add_root_schema(schema)
        .map_err(|err| format!("typify failed for {}: {err}", schema_path.display()))?;

    let rust = type_space.to_stream().to_string();
    if rust.trim().is_empty() {
        return Err(format!(
            "typify produced empty output for {}",
            schema_path.display()
        ));
    }

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create output directory {}: {err}", parent.display()))?;
    }

    let header = format!(
        "//! AUTO-GENERATED FROM JSON SCHEMA - DO NOT MODIFY\n//! Source: {}\n//! Generated by: pnpm run codegen\n\n",
        rel.display()
    );

    fs::write(out_path, format!("{header}{rust}"))
        .map_err(|err| format!("failed to write {}: {err}", out_path.display()))?;
    Ok(())
}

fn main() {
    let root = repo_root();
    let deref_dir = deref_schemas_dir(&root);
    let src_dir = source_schemas_dir(&root);
    let out_root = root.join("crates/nexus-contracts/src/generated");

    eprintln!("nexus-rust-gen");
    eprintln!("  repo:  {}", root.display());
    eprintln!("  src:   {}", src_dir.display());
    eprintln!("  deref: {}", deref_dir.display());
    eprintln!("  out:   {}", out_root.display());

    if !deref_dir.is_dir() {
        eprintln!(
            "error: dereferenced schema tree not found — run `pnpm run codegen` (stage 1) first, \
             or set NEXUS_DEREF_SCHEMAS_DIR"
        );
        process::exit(1);
    }

    let pattern = deref_dir.join("**/*.schema.json");
    let mut schema_paths: Vec<PathBuf> = glob(pattern.to_str().expect("glob pattern is literal"))
        .expect("glob pattern compiles")
        .filter_map(Result::ok)
        .collect();
    schema_paths.sort();

    let skip_set: BTreeSet<&str> = SKIP_SCHEMAS.iter().copied().collect();

    let mut generated: BTreeSet<PathBuf> = BTreeSet::new();
    let mut failures: Vec<String> = Vec::new();
    let mut skipped = 0usize;

    for schema_path in &schema_paths {
        let rel = schema_path
            .strip_prefix(&deref_dir)
            .expect("globbed path sits under the deref dir");
        if skip_set.contains(rel_posix(rel).as_str()) {
            skipped += 1;
            continue;
        }

        let out_path = output_path(&out_root, rel);
        if let Err(err) = generate_schema_rust(schema_path, rel, &out_path) {
            failures.push(err);
            continue;
        }
        generated.insert(out_path.clone());
        println!(
            "wrote {}",
            out_root
                .strip_prefix(&root)
                .ok()
                .and_then(|_| out_path.strip_prefix(&root).ok())
                .unwrap_or(&out_path)
                .display()
        );
    }

    eprintln!("skipped {skipped} definition-only / canonical-skip schema(s)");

    if !failures.is_empty() {
        for err in &failures {
            eprintln!("error: {err}");
        }
        process::exit(1);
    }

    if generated.is_empty() {
        eprintln!(
            "error: generated 0 Rust schema files (deref dir: {})",
            deref_dir.display()
        );
        process::exit(1);
    }

    println!("Rust: generated {} file(s)", generated.len());
}
