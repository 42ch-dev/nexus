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
//! # Scope
//!
//! Emits one `.rs` file per non-skipped schema, mirroring the schema tree under
//! `crates/nexus-contracts/src/generated/`. Also writes a barrel `mod.rs` for every
//! directory (nested `pub mod` + flat `pub use`, mirroring the sibling spoke
//! generator), and stamps `SCHEMA_VERSIONS` / `LATEST_SCHEMA_VERSION` into the root.
//!
//! Orchestrator wiring (T3), clippy tuning (T4) and drift reconciliation (T5) are
//! handled by later tasks — the emitted struct/enum derives may still differ from
//! what consumers and the drift test expect.

use glob::glob;
use schemars::schema::RootSchema;
use serde_json::Value;
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

/// Derive the canonical contract type name from a schema file name, mirroring the
/// TypeScript generator's `schemaToTypeName` (`tooling/codegen/src/utils.ts`) so
/// Rust and TypeScript agree on type names regardless of how the schema `title`
/// is phrased (Nexus titles carry a `"Nexus <Name>"` product prefix).
///
/// Rule (must match the TS side exactly): strip the trailing `.schema.json`, split
/// on `-`, capitalize the **first** character of each word (leaving the rest
/// unchanged), join. E.g. `work-summary.schema.json` → `WorkSummary`,
/// `fork-branch.schema.json` → `ForkBranch`, `world-membership.schema.json` →
/// `WorldMembership`, `context-assembly-v1.schema.json` → `ContextAssemblyV1`.
///
/// `typify` derives the root type name from the schema `title` (via `convert_case`'s
/// `to_pascal_case`, which is idempotent on these already-PascalCase names), so
/// overriding the title to this basename-derived name — before `add_root_schema` —
/// makes the emitted Rust struct/enum match the TS contract and the drift-test
/// `entry!` registrations. Scoped: only the in-memory schema passed to `typify`;
/// source schema files are never mutated.
fn schema_type_name(file_name: &str) -> String {
    let base = file_name.strip_suffix(".schema.json").unwrap_or(file_name);
    base.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// Decide whether a schema's barrel export should be a glob (`true`) or a specific
/// type re-export (`false`), mirroring the sibling spoke `export_mode_for_schema`
/// heuristic. Reads the **source** schema's `title` (via `$NEXUS_SRC_SCHEMAS_DIR`),
/// NOT the dereferenced tree: a single-word title (no spaces) → specific export;
/// otherwise → glob.
///
/// The caller resolves the specific type name via `schema_type_name(file_name)`
/// (basename-derived), NOT the raw source title: nexus titles carry a `"Nexus <Name>"`
/// product prefix and some single-word titles carry a `V1` suffix
/// (`"CreatorRuntimePolicyResponseV1"`) that diverges from the basename-derived
/// contract name (`CreatorRuntimePolicyResponse`) which is what `typify` emits
/// after T1's in-memory title override. Definitions-only / canonical-skip schemas
/// never reach here (they are filtered by `SKIP_SCHEMAS`).
fn export_mode_for_schema(src_schema_path: &Path) -> bool {
    let Ok(content) = fs::read_to_string(src_schema_path) else {
        // Source schema unreadable — default to the safe glob (re-exports everything).
        return true;
    };
    let Ok(raw) = serde_json::from_str::<Value>(&content) else {
        return true;
    };
    if let Some(title) = raw.get("title").and_then(Value::as_str) {
        if !title.contains(' ') {
            return false;
        }
    }
    true
}

/// Read the top-level `schema_version` integer from a **source** schema. The
/// dereferenced tree inlines this stamp into `properties.schema_version` (the
/// field definition), losing the top-level contract version, so it must be read
/// from `$NEXUS_SRC_SCHEMAS_DIR/<path>`. Used to build `SCHEMA_VERSIONS`.
fn read_schema_version(src_schema_path: &Path) -> Result<u32, String> {
    let content = fs::read_to_string(src_schema_path)
        .map_err(|err| format!("read schema_version {}: {err}", src_schema_path.display()))?;
    let raw: Value = serde_json::from_str(&content)
        .map_err(|err| format!("parse schema_version {}: {err}", src_schema_path.display()))?;
    raw.get("schema_version")
        .and_then(Value::as_u64)
        .map(|v| u32::try_from(v).unwrap_or(0))
        .ok_or_else(|| {
            format!(
                "missing/invalid top-level `schema_version` in {}",
                src_schema_path.display()
            )
        })
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
                    Component::Normal(os) => {
                        Some(PathBuf::from(to_rust_module_name(&os.to_string_lossy())))
                    }
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

    out_root
        .join(parent)
        .join(format!("{}.rs", to_rust_module_name(stem)))
}

/// typify's `maxLength`/`minLength` checks use UTF-8 `value.len()`. JSON Schema
/// draft-07 counts Unicode scalars, so rewrite every generated length check.
/// Trim rejection is not a JSON Schema keyword. Inject it only into Character
/// `display_name` newtypes — including copies typify emits when a response DTO
/// inlines Character — so every occurrence matches the domain parser. Other
/// length-bounded strings stay untrimmed.
fn is_character_display_name_type(type_name: &str) -> bool {
    type_name.contains("Character") && type_name.ends_with("DisplayName")
}

fn inject_character_display_name_trim(rust: &str) -> String {
    const IMPL: &str = "impl :: std :: str :: FromStr for ";
    const FROM_STR_OPEN: &str =
        "fn from_str (value : & str) -> :: std :: result :: Result < Self , self :: error :: ConversionError > { ";
    const TRIM_GUARD: &str =
        "if value . trim () != value { return Err (\"must be trimmed\" . into ()) ; } ";

    let mut out = String::with_capacity(rust.len() + 256);
    let mut cursor = 0;
    while let Some(rel) = rust[cursor..].find(IMPL) {
        let impl_at = cursor + rel;
        out.push_str(&rust[cursor..impl_at]);
        let name_start = impl_at + IMPL.len();
        let Some(name_len) = rust[name_start..].find(' ') else {
            out.push_str(&rust[impl_at..]);
            return out;
        };
        let name_end = name_start + name_len;
        let type_name = &rust[name_start..name_end];
        out.push_str(&rust[impl_at..name_end]);
        cursor = name_end;
        if !is_character_display_name_type(type_name) {
            continue;
        }
        if let Some(open_rel) = rust[cursor..].find(FROM_STR_OPEN) {
            let insert_at = cursor + open_rel + FROM_STR_OPEN.len();
            out.push_str(&rust[cursor..insert_at]);
            if !rust[insert_at..].starts_with(TRIM_GUARD) {
                out.push_str(TRIM_GUARD);
            }
            cursor = insert_at;
        }
    }
    out.push_str(&rust[cursor..]);
    out
}

fn rewrite_unicode_scalar_length_checks(rust: &str) -> String {
    inject_character_display_name_trim(&rust.replace("value . len ()", "value . chars () . count ()"))
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

/// Wire schemas whose serialized object-key order is part of a compatibility pin.
/// `typify` emits fields in BTreeMap (alphabetical) order; we restore the source
/// schema `properties` insertion order so omitted-optional JSON stays byte-stable.
const PRESERVE_PROPERTY_ORDER: &[&str] = &[
    "daemon-api/agent-host/session-response.schema.json",
    "daemon-api/agent-host/create-session-request.schema.json",
    "daemon-api/agent-host/session-viewpoint.schema.json",
];

/// Property names from a source schema object, in JSON insertion order
/// (`serde_json` `preserve_order`).
fn source_property_order(schema: &Value) -> Vec<String> {
    schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|props| props.keys().cloned().collect())
        .unwrap_or_default()
}

fn field_ident(field: &str) -> Option<String> {
    let idx = field.rfind(" pub ")?;
    let rest = field[idx + 5..].trim_start();
    let ident: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if ident.is_empty() {
        None
    } else {
        Some(ident)
    }
}

fn split_struct_fields(body: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut brace = 0i32;
    let mut angle = 0i32;
    let mut paren = 0i32;
    let mut bracket = 0i32;
    for ch in body.chars() {
        match ch {
            '{' => brace += 1,
            '}' => brace -= 1,
            '<' => angle += 1,
            '>' => angle -= 1,
            '(' => paren += 1,
            ')' => paren -= 1,
            '[' => bracket += 1,
            ']' => bracket -= 1,
            ',' if brace == 0 && angle == 0 && paren == 0 && bracket == 0 => {
                fields.push(std::mem::take(&mut current));
                current.push(ch);
                fields.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(ch);
    }
    if !current.is_empty() {
        fields.push(current);
    }
    fields
}

fn reorder_struct_body(body: &str, order: &[String]) -> String {
    let chunks = split_struct_fields(body);
    let mut named: Vec<(String, String)> = Vec::new();
    for chunk in &chunks {
        if let Some(name) = field_ident(chunk) {
            named.push((name, chunk.clone()));
        }
    }
    if named.is_empty() {
        return body.to_string();
    }
    let mut used = vec![false; named.len()];
    let mut ordered_fields: Vec<String> = Vec::new();
    for want in order {
        if let Some(slot) = named
            .iter()
            .enumerate()
            .find(|(i, (name, _))| !used[*i] && name == want)
            .map(|(i, _)| i)
        {
            used[slot] = true;
            ordered_fields.push(named[slot].1.clone());
        }
    }
    for (i, used_flag) in used.iter().enumerate() {
        if !*used_flag {
            ordered_fields.push(named[i].1.clone());
        }
    }
    let mut separators: Vec<String> = chunks
        .iter()
        .filter(|c| field_ident(c).is_none())
        .cloned()
        .collect();
    let mut out = String::new();
    for (i, field) in ordered_fields.iter().enumerate() {
        out.push_str(field);
        if i + 1 < ordered_fields.len() {
            if let Some(pos) = separators.iter().position(|s| s.contains(',')) {
                out.push_str(&separators.remove(pos));
            } else {
                out.push(',');
            }
        }
    }
    for sep in separators {
        out.push_str(&sep);
    }
    out
}

fn reorder_root_struct_fields(rust: &str, type_name: &str, order: &[String]) -> String {
    if order.is_empty() {
        return rust.to_string();
    }
    let needle = format!("pub struct {type_name} {{");
    let Some(start) = rust.find(&needle) else {
        return rust.to_string();
    };
    let body_start = start + needle.len();
    let bytes = rust.as_bytes();
    let mut depth = 1i32;
    let mut i = body_start;
    while i < rust.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' if depth == 1 => {
                let body = &rust[body_start..i];
                let reordered = reorder_struct_body(body, order);
                return format!("{}{}{}", &rust[..body_start], reordered, &rust[i..]);
            }
            b'}' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    rust.to_string()
}

/// Generate Rust source for a single schema via `typify` and write it to `out_path`.
///
/// `rel` is the schema's path relative to the schemas dir (POSIX or platform); it
/// is rendered verbatim into the file header as the canonical source pointer.
fn generate_schema_rust(
    schema_path: &Path,
    rel: &Path,
    out_path: &Path,
    src_schema_path: &Path,
) -> Result<(), String> {
    let content = fs::read_to_string(schema_path)
        .map_err(|err| format!("failed to read {}: {err}", schema_path.display()))?;
    let mut schema: RootSchema = serde_json::from_str(&content)
        .map_err(|err| format!("invalid JSON Schema in {}: {err}", schema_path.display()))?;

    // Override the schema `title` to the basename-derived PascalCase name BEFORE
    // handing it to `typify`. Without this, `typify` names the root type from the
    // raw title (e.g. `"Nexus World Entity"` → `NexusWorldEntity`, `"Nexus Delta"`
    // → `NexusDelta`) or emits no root type at all when the title is absent
    // (e.g. `work-summary`). Overriding aligns the emitted name with the TS contract
    // (`schemaToTypeName`) and the drift-test `entry!` registrations (`World`,
    // `WorkSummary`, `ForkBranch`, …). Only the in-memory `metadata.title` is touched;
    // other metadata (description, $id, …) is preserved.
    let file_name = rel
        .file_name()
        .and_then(|s| s.to_str())
        .expect("schema rel path has a file name");
    let type_name = schema_type_name(file_name);
    {
        let metadata = schema.schema.metadata.get_or_insert_with(Box::default);
        metadata.title = Some(type_name.clone());
    }

    let mut settings = TypeSpaceSettings::default();
    // Mirror the proven spoke setting; T4 may tune derives if clippy requires.
    settings.with_struct_builder(true);

    let mut type_space = TypeSpace::new(&settings);
    type_space
        .add_root_schema(schema)
        .map_err(|err| format!("typify failed for {}: {err}", schema_path.display()))?;

    let mut rust = rewrite_unicode_scalar_length_checks(&type_space.to_stream().to_string());
    if rust.trim().is_empty() {
        return Err(format!(
            "typify produced empty output for {}",
            schema_path.display()
        ));
    }
    let rel_posix_path = rel_posix(rel);
    if PRESERVE_PROPERTY_ORDER.contains(&rel_posix_path.as_str()) {
        if let Ok(src_raw) = fs::read_to_string(src_schema_path) {
            if let Ok(src_json) = serde_json::from_str::<Value>(&src_raw) {
                let order = source_property_order(&src_json);
                rust = reorder_root_struct_fields(&rust, &type_name, &order);
            }
        }
    }
    if rel_posix_path == "daemon-api/agent-host/session-response.schema.json" {
        let viewpoint_schema = src_schema_path
            .parent()
            .map(|dir| dir.join("session-viewpoint.schema.json"));
        if let Some(path) = viewpoint_schema {
            if let Ok(src_raw) = fs::read_to_string(path) {
                if let Ok(src_json) = serde_json::from_str::<Value>(&src_raw) {
                    let order = source_property_order(&src_json);
                    rust = reorder_root_struct_fields(&rust, "NexusSessionViewpoint", &order);
                }
            }
        }
    }

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create output directory {}: {err}",
                parent.display()
            )
        })?;
    }

    let header = format!(
        "//! AUTO-GENERATED FROM JSON SCHEMA - DO NOT MODIFY\n//! Source: {}\n//! Generated by: pnpm run codegen\n\n",
        rel.display()
    );

    fs::write(out_path, format!("{header}{rust}"))
        .map_err(|err| format!("failed to write {}: {err}", out_path.display()))?;
    Ok(())
}

/// A generated `.rs` module, tracked so the barrel `mod.rs` files can declare and
/// re-export it. `out_rel` is relative to the output root.
struct GeneratedModule {
    /// Path relative to the output root, e.g. `daemon_api/canvas/outline/work_outline.rs`.
    out_rel: PathBuf,
    /// Snake_case Rust module name, e.g. `work_outline`.
    rust_mod: String,
    /// `true` → `pub use <rust_mod>::*;` (glob). `false` → specific type re-export.
    export_all: bool,
    /// When `export_all == false`, the basename-derived type name to re-export
    /// (matches the emitted Rust struct after T1's title override).
    export_type: Option<String>,
}

/// Write a single `mod.rs` barrel for `dir`. Mirrors the spoke `write_mod_rs` shape
/// (nested `pub mod` declarations followed by flat `pub use` re-exports), adapted
/// for nexus: the root additionally carries `#![allow(ambiguous_glob_reexports)]`
/// and the `SCHEMA_VERSIONS` / `LATEST_SCHEMA_VERSION` constants (consumers and the
/// drift test depend on this exact root shape).
fn write_mod_rs(
    dir: &Path,
    child_modules: &[String],
    dir_modules: &[&GeneratedModule],
    is_root: bool,
    schema_versions: &[(String, u32)],
) {
    let mut lines: Vec<String> = Vec::new();

    if is_root {
        lines.extend([
            "//! Nexus Wire Contracts - Generated Rust Types".into(),
            "//!".into(),
            "//! AUTO-GENERATED FROM JSON SCHEMA - DO NOT MODIFY MANUALLY".into(),
            "//! Source: schemas/*.schema.json".into(),
            "//! Generated by: `pnpm run codegen`".into(),
            "//!".into(),
            "//! The root flat-globs every scope (`pub use domain::*`, `pub use daemon_api::*`, \u{2026}).".into(),
            "//! Scope submodule names can collide (e.g. `domain::memory` vs `daemon_api::memory`);".into(),
            "//! the flat TYPE re-exports are all unique, so the module-name ambiguity is benign.".into(),
            "#![allow(ambiguous_glob_reexports)]".into(),
            // The clippy allows below are emitted into the generated file as
            // `//` *string* lines (not Rust comments here) so the justification
            // travels with the generated code. See plan v1.138 T4 (architect Q4).
            "// Clippy: typify-generated code is machine output. The allows below are the".into(),
            "// complete set that fires on typify 0.3 output under this workspace's pedantic +".into(),
            "// nursery groups (plan v1.138 T4, architect Q4 lock). They are not hand-tunable".into(),
            "// without forking typify. Scoped to this `generated` subtree ONLY — hand-written".into(),
            "// code under `src/local/` and `src/enum_conversions.rs` is NOT covered. Mirrors".into(),
            "// the `.rustfmt.toml` `ignore` precedent that exempts generated code.".into(),
            "#![allow(".into(),
            "    clippy::clone_on_copy,".into(),
            "    clippy::default_trait_access,".into(),
            "    clippy::derivable_impls,".into(),
            "    clippy::doc_markdown,".into(),
            "    clippy::len_zero,".into(),
            "    clippy::missing_const_for_fn,".into(),
            "    clippy::must_use_candidate,".into(),
            "    clippy::possible_missing_else,".into(),
            "    clippy::return_self_not_must_use,".into(),
            "    clippy::struct_excessive_bools,".into(),
            "    clippy::struct_field_names,".into(),
            "    clippy::too_long_first_doc_paragraph,".into(),
            "    clippy::uninlined_format_args,".into(),
            "    clippy::unreadable_literal,".into(),
            "    clippy::use_self,".into(),
            ")]".into(),
            String::new(),
        ]);
    } else {
        lines.extend([
            "//! AUTO-GENERATED FROM JSON SCHEMA - DO NOT MODIFY".into(),
            "//! Generated by: pnpm run codegen".into(),
            String::new(),
        ]);
    }

    for m in child_modules {
        lines.push(format!("pub mod {m};"));
    }
    for module in dir_modules {
        lines.push(format!("pub mod {};", module.rust_mod));
    }
    lines.push(String::new());

    for m in child_modules {
        lines.push(format!("pub use {m}::*;"));
    }
    for module in dir_modules {
        if module.export_all {
            lines.push(format!("pub use {}::*;", module.rust_mod));
        } else if let Some(type_name) = &module.export_type {
            lines.push(format!("pub use {}::{type_name};", module.rust_mod));
        }
    }

    if is_root {
        let latest = schema_versions.iter().map(|(_, v)| *v).max().unwrap_or(0);
        lines.push(String::new());
        lines.push("/// Schema version constants".into());
        lines.push("pub const SCHEMA_VERSIONS: &[(&str, u32)] = &[".into());
        for (name, version) in schema_versions {
            lines.push(format!("    (\"{name}\", {version}),"));
        }
        lines.push("];".into());
        lines.push(String::new());
        lines.push("/// Highest `schema_version` among emitted contract schemas".into());
        lines.push(format!("pub const LATEST_SCHEMA_VERSION: u32 = {latest};"));
    }

    lines.push(String::new());

    fs::write(dir.join("mod.rs"), lines.join("\n"))
        .unwrap_or_else(|err| panic!("write mod.rs {}: {err}", dir.display()));
}

/// Write a `mod.rs` barrel for every directory in the generated tree. A directory
/// gets a barrel iff it directly contains `.rs` schema files or a subdirectory that
/// (transitively) does — every ancestor of a generated file up to the output root.
fn write_barrels(out_root: &Path, modules: &[GeneratedModule], schema_versions: &[(String, u32)]) {
    // Directories that directly contain a generated `.rs` file.
    let mut file_dirs: BTreeSet<PathBuf> = BTreeSet::new();
    for m in modules {
        let abs = out_root.join(&m.out_rel);
        if let Some(parent) = abs.parent() {
            file_dirs.insert(parent.to_path_buf());
        }
    }

    // Every ancestor up to (and including) the output root needs a barrel too.
    let mut all_dirs: BTreeSet<PathBuf> = BTreeSet::new();
    all_dirs.insert(out_root.to_path_buf());
    for dir in &file_dirs {
        let mut cur = dir.as_path();
        loop {
            all_dirs.insert(cur.to_path_buf());
            match cur.parent() {
                Some(p) if p.starts_with(out_root) => cur = p,
                _ => break,
            }
        }
    }

    for dir in &all_dirs {
        let is_root = dir == out_root;

        // Immediate subdirectories of `dir` that participate in the tree.
        let mut child_dirs: Vec<String> = all_dirs
            .iter()
            .filter(|other| other.parent() == Some(dir.as_path()))
            .filter_map(|other| other.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        child_dirs.sort();

        // Modules directly inside `dir`.
        let mut dir_modules: Vec<&GeneratedModule> = modules
            .iter()
            .filter(|m| out_root.join(&m.out_rel).parent() == Some(dir.as_path()))
            .collect();
        dir_modules.sort_by(|a, b| a.rust_mod.cmp(&b.rust_mod));

        write_mod_rs(
            dir,
            &child_dirs,
            &dir_modules,
            is_root,
            if is_root { schema_versions } else { &[] },
        );
    }
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

    // Replace any prior generated tree (bespoke or stale) with a clean typify output,
    // mirroring the sibling spoke generator. Stray files from a previous generator
    // (e.g. a definitions-only emit like `common_types.rs`) would otherwise linger
    // as unmounted orphans on disk.
    if out_root.exists() {
        fs::remove_dir_all(&out_root).expect("remove stale generated tree");
    }
    fs::create_dir_all(&out_root).expect("create generated tree root");

    let pattern = deref_dir.join("**/*.schema.json");
    let mut schema_paths: Vec<PathBuf> = glob(pattern.to_str().expect("glob pattern is literal"))
        .expect("glob pattern compiles")
        .filter_map(Result::ok)
        .collect();
    schema_paths.sort();

    let skip_set: BTreeSet<&str> = SKIP_SCHEMAS.iter().copied().collect();

    let mut generated: BTreeSet<PathBuf> = BTreeSet::new();
    let mut generated_modules: Vec<GeneratedModule> = Vec::new();
    // `(type_name, schema_version)` pairs in filesystem-walk order (the loop iterates
    // sorted schema paths), matching the committed `SCHEMA_VERSIONS` ordering.
    let mut schema_versions: Vec<(String, u32)> = Vec::new();
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
        if let Err(err) = generate_schema_rust(schema_path, rel, &out_path, &src_dir.join(rel)) {
            failures.push(err);
            continue;
        }

        // Export mode + schema_version are read from the SOURCE schema (the deref
        // tree loses the top-level `schema_version` stamp and keeps the raw title).
        let src_path = src_dir.join(rel);
        let export_all = export_mode_for_schema(&src_path);
        let file_name = rel
            .file_name()
            .and_then(|s| s.to_str())
            .expect("schema rel path has a file name");
        let type_name = schema_type_name(file_name);
        let export_type = if export_all {
            None
        } else {
            Some(type_name.clone())
        };

        let rust_mod = out_path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(String::from)
            .expect("output path has a stem");
        let out_rel = out_path
            .strip_prefix(&out_root)
            .expect("output path sits under the output root")
            .to_path_buf();

        match read_schema_version(&src_path) {
            Ok(version) => schema_versions.push((type_name, version)),
            Err(err) => failures.push(err),
        }

        generated.insert(out_path.clone());
        generated_modules.push(GeneratedModule {
            out_rel,
            rust_mod,
            export_all,
            export_type,
        });
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

    // Write the barrel `mod.rs` for every directory in the tree (root gets the
    // `#![allow(...)]` attr + `SCHEMA_VERSIONS` / `LATEST_SCHEMA_VERSION`).
    write_barrels(&out_root, &generated_modules, &schema_versions);

    let latest = schema_versions.iter().map(|(_, v)| *v).max().unwrap_or(0);
    println!(
        "Rust: generated {} schema file(s) + barrel mod.rs tree, {} schema version(s), latest v{latest}",
        generated.len(),
        schema_versions.len()
    );
}
