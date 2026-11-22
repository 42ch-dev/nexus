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
//! The tree is built in a sibling staging directory and published by two renames — the
//! committed tree is retired to a sibling `.backup`, then the staged tree is landed —
//! so a failed, aborted or crashed run never prunes or partially regenerates the tree
//! in place. Publication is *recoverable*, not atomic: `std::fs` exposes no portable
//! directory exchange, so `out_root` is briefly absent between the two renames, and a
//! hard kill in that window leaves it absent until the next run restores the `.backup`.
//! Overlapping runs of this binary against the same output directory are refused by a
//! per-output advisory lock rather than allowed to share the staging/backup siblings.
//!
//! # Formatting pass + `$ref` dedupe
//!
//! `typify` hands back a `TokenStream`; its `to_string()` is a single-line token
//! dump. Every generated file is therefore parsed back into a `syn` AST and
//! re-printed with `prettyplease` before it is written — an in-process formatting
//! pass with no external `rustfmt` binary or pinned-toolchain coupling.
//!
//! The same AST pass removes the duplicate of a `$ref` target that `typify` inlines
//! into every referencing schema file (a standalone module type plus a wire-identical
//! copy, e.g. `WorkPoolEntry` inside `work_pool_list_response`). Each referenced type
//! keeps exactly one canonical home — the standalone module emitted for its own
//! schema — and referencing modules import it from there, so consumers no longer need
//! path-qualified workarounds. Names with no unique canonical home are left untouched.
//!
//! Orchestrator wiring (T3), clippy tuning (T4) and drift reconciliation (T5) are
//! handled by later tasks — the emitted struct/enum derives may still differ from
//! what consumers and the drift test expect.

use glob::glob;
use schemars::Schema;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
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
    "core/provider-host-event.schema.json",
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

/// typify 0.10 validates in `TryFrom<&str>`: `FromStr` delegates to it and
/// serde's `Deserialize` calls `TryFrom<String>` (which forwards to the `&str`
/// impl), so the trim guard belongs on `TryFrom<&str>` only — one authoritative
/// check covering parse, serde and direct conversion. Matching is AST-based, so
/// an equivalent `::core::convert::TryFrom` path still qualifies; a qualifying
/// struct with no such impl fails the codegen lane instead of silently losing
/// the trim contract.
fn inject_character_display_name_trim(file: &mut syn::File) -> Result<(), String> {
    let targets: BTreeSet<String> = file
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Struct(strukt)
                if is_character_display_name_type(&strukt.ident.to_string()) =>
            {
                Some(strukt.ident.to_string())
            }
            _ => None,
        })
        .collect();
    let mut guarded: BTreeSet<String> = BTreeSet::new();
    for item in &mut file.items {
        let syn::Item::Impl(block) = item else {
            continue;
        };
        if !try_from_str_impl(block) {
            continue;
        }
        let Some(name) = local_self_type(&block.self_ty) else {
            continue;
        };
        if !targets.contains(&name) {
            continue;
        }
        let Some(syn::ImplItem::Fn(function)) = block
            .items
            .iter_mut()
            .find(|item| matches!(item, syn::ImplItem::Fn(f) if f.sig.ident == "try_from"))
        else {
            continue;
        };
        if !function.block.stmts.first().is_some_and(is_trim_guard) {
            let guard = syn::parse_str::<syn::Stmt>(
                "if value.trim() != value { return Err(\"must be trimmed\".into()); }",
            )
            .map_err(|err| format!("build trim guard for {name}: {err}"))?;
            function.block.stmts.insert(0, guard);
        }
        guarded.insert(name);
    }
    match targets.difference(&guarded).next() {
        Some(missing) => Err(format!(
            "no `TryFrom<&str>` impl for Character display-name type {missing}"
        )),
        None => Ok(()),
    }
}

/// Schema extension marker (`x-nexus-tri-state: true`) for nullable PATCH
/// properties whose three wire states — omitted (keep), `null` (clear),
/// value (set) — must survive serde. A plain optional property deserializes
/// explicit `null` to `None`, which erases the clear state, so the generated
/// field additionally carries a presence-preserving deserializer.
const TRI_STATE_MARKER: &str = "x-nexus-tri-state";

/// Collect the marked property names of a source schema, in declaration order.
fn tri_state_fields(src_schema_path: &Path) -> Vec<String> {
    let Ok(content) = fs::read_to_string(src_schema_path) else {
        return Vec::new();
    };
    let Ok(raw) = serde_json::from_str::<Value>(&content) else {
        return Vec::new();
    };
    raw.get("properties")
        .and_then(Value::as_object)
        .map(|props| {
            props
                .iter()
                .filter(|(_, schema)| {
                    schema.get(TRI_STATE_MARKER).and_then(Value::as_bool) == Some(true)
                })
                .map(|(name, _)| name.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// Inject `#[serde(default, deserialize_with = …)]` into the marked fields of
/// the emitted root struct. The generated field type is already
/// `Option<serde_json::Value>` (the schema keeps the property unrestricted);
/// only the attribute is added, so every unmarked optional field keeps the
/// stock typify semantics. `default` covers absence (`None`); the helper maps
/// a present `null` to `Some(Value::Null)` instead of collapsing it.
///
/// The `default` half is supplied here, not by typify: 0.7 emitted
/// `#[serde(default, skip_serializing_if = …)]` for an optional `Option`
/// field, and 0.8 dropped the `default` (relying on serde's implicit
/// missing-field-to-`None` for `Option`). That implicit path is bypassed once
/// a field carries `deserialize_with` — serde then makes a missing field a
/// hard error — so the marked tri-state fields keep their author-facing
/// omission state only because the attribute is emitted here.
fn inject_tri_state_deserializers(rust: &str, type_name: &str, fields: &[String]) -> String {
    if fields.is_empty() {
        return rust.to_string();
    }
    let struct_header = format!("pub struct {type_name} ");
    let Some(start) = rust.find(&struct_header) else {
        return rust.to_string();
    };
    let Some(open) = rust[start..].find('{') else {
        return rust.to_string();
    };
    let open = start + open;
    let mut depth = 0i32;
    let mut close = open;
    for (offset, ch) in rust[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close = open + offset;
                    break;
                }
            }
            _ => {}
        }
    }
    let (head, body, tail) = (
        rust[..open].to_string(),
        rust[open..close].to_string(),
        rust[close..].to_string(),
    );
    let mut body = body;
    for field in fields {
        let needle = format!("pub {field} :");
        let Some(at) = body.find(&needle) else {
            continue;
        };
        let Some(attr_at) = body[..at].rfind("# [") else {
            continue;
        };
        // a field that carries `deserialize_with` makes serde reject omission,
        // so the presence-preserving attribute must carry `default` itself.
        let attribute = concat!(
            "# [serde (default , deserialize_with = ",
            "\"crate :: tristate :: deserialize_presence\")] ",
        );
        body.insert_str(attr_at, attribute);
    }
    format!("{head}{body}{tail}")
}
/// Schema extension marker for boolean fields whose accepted wire value is true.
const LITERAL_TRUE_MARKER: &str = "x-nexus-literal-true";

fn literal_true_fields(src_schema_path: &Path) -> Vec<String> {
    let Ok(content) = fs::read_to_string(src_schema_path) else {
        return Vec::new();
    };
    let Ok(raw) = serde_json::from_str::<Value>(&content) else {
        return Vec::new();
    };
    raw.get("properties")
        .and_then(Value::as_object)
        .map(|props| {
            props
                .iter()
                .filter(|(_, schema)| {
                    schema.get(LITERAL_TRUE_MARKER).and_then(Value::as_bool) == Some(true)
                })
                .map(|(name, _)| name.clone())
                .collect()
        })
        .unwrap_or_default()
}

fn inject_literal_true_serde_attributes(rust: &str, type_name: &str, fields: &[String]) -> String {
    if fields.is_empty() {
        return rust.to_string();
    }
    let struct_header = format!("pub struct {type_name} ");
    let Some(start) = rust.find(&struct_header) else {
        return rust.to_string();
    };
    let Some(open_rel) = rust[start..].find('{') else {
        return rust.to_string();
    };
    let open = start + open_rel;
    let mut depth = 0i32;
    let mut close = open;
    for (offset, ch) in rust[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close = open + offset;
                    break;
                }
            }
            _ => {}
        }
    }
    let (head, body, tail) = (
        rust[..open].to_string(),
        rust[open..close].to_string(),
        rust[close..].to_string(),
    );
    let mut body = body;
    for field in fields {
        let needle = format!("pub {field} :");
        let Some(at) = body.find(&needle) else {
            continue;
        };
        body.insert_str(
            at,
            concat!(
                "# [serde (deserialize_with = \"crate :: literal_true :: deserialize\" , ",
                "serialize_with = \"crate :: literal_true :: serialize\")] ",
            ),
        );
    }
    format!("{head}{body}{tail}")
}

fn rewrite_unicode_scalar_length_checks(rust: &str) -> String {
    rust.replace("value . len ()", "value . chars () . count ()")
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
    "daemon-api/agent-host/session-list-response.schema.json",
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

/// Restore `NexusSessionViewpoint` field order from the source viewpoint schema
/// whenever typify inlines that type (create/single/list carriers).
fn apply_nested_session_viewpoint_order(rust: &str, src_schema_path: &Path) -> String {
    if !rust.contains("pub struct NexusSessionViewpoint {") {
        return rust.to_string();
    }
    let Some(dir) = src_schema_path.parent() else {
        return rust.to_string();
    };
    let viewpoint_schema = dir.join("session-viewpoint.schema.json");
    let Ok(src_raw) = fs::read_to_string(&viewpoint_schema) else {
        return rust.to_string();
    };
    let Ok(src_json) = serde_json::from_str::<Value>(&src_raw) else {
        return rust.to_string();
    };
    let order = source_property_order(&src_json);
    reorder_root_struct_fields(rust, "NexusSessionViewpoint", &order)
}

/// Mirror typify's convert_case Pascal naming for a schema `title`
/// (`"Nexus CharacterDetail"` → `NexusCharacterDetail`).
fn title_to_type_name(title: &str) -> String {
    title
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// Rename identifier-prefixed occurrences of `from` to `to`. A match counts
/// when the preceding char is not part of an identifier, so derived names
/// (`NexusXBuilder`, `NexusXNested`) shift coherently with the root rename.
fn rename_identifier_prefix(rust: &str, from: &str, to: &str) -> String {
    if from == to || from.is_empty() {
        return rust.to_string();
    }
    let mut out = String::with_capacity(rust.len());
    let mut rest = rust;
    while let Some(pos) = rest.find(from) {
        let bounded = rest[..pos]
            .chars()
            .next_back()
            .map(|c| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(true);
        out.push_str(&rest[..pos]);
        if bounded {
            out.push_str(to);
        } else {
            out.push_str(from);
        }
        rest = &rest[pos + from.len()..];
    }
    out.push_str(rest);
    out
}

/// Canonical home of a generated `$ref` target: the standalone module that owns
/// the name. `module` is relative to `crate::generated` (e.g.
/// `core::works::work_pool_entry`).
struct RefTargetEntry {
    /// Basename-derived contract name of the schema this module was generated for
    /// (`WorkPoolEntry`, `TimelineEventInfo`).
    root: String,
    /// Module path relative to `crate::generated`.
    module: String,
    /// Type names the module declares at its top level, as emitted.
    declared: BTreeSet<String>,
}

struct RefTargetIndex {
    entries: Vec<RefTargetEntry>,
}

impl RefTargetIndex {
    /// Resolve the owner module of a declared type name, or `None` when the name has
    /// no unique canonical home (it then stays exactly as `typify` emitted it).
    ///
    /// A name is owned when exactly one generated module both *declares* it and is
    /// its family home: the module whose root type *is* the name (`WorkPoolEntry`),
    /// or — for a nested type derived from an inlined `$ref` body — a module whose
    /// root type name prefixes it (`TimelineEventInfoStatus` →
    /// `daemon_api::timeline::timeline_event_info`). Declaring the name is required:
    /// a mere prefix match (`CharacterActorRefActorKind` vs the `Character` module,
    /// which does not declare it) must not invent a home. Ambiguous matches and the
    /// file's own module resolve to `None`.
    fn owner_of(&self, name: &str, self_module: &str) -> Option<&str> {
        let candidates: Vec<&str> = self
            .entries
            .iter()
            .filter(|entry| {
                entry.declared.contains(name)
                    && (entry.root == name || name.starts_with(entry.root.as_str()))
            })
            .map(|entry| entry.module.as_str())
            .collect();
        if candidates.len() == 1 && candidates[0] != self_module {
            Some(candidates[0])
        } else {
            None
        }
    }
}

/// Module path of a schema's generated file, relative to `crate::generated`
/// (`core/works/work-pool-entry.schema.json` → `core::works::work_pool_entry`).
fn generated_module_path(rel: &Path) -> String {
    let mut segments: Vec<String> = rel
        .components()
        .filter_map(|c| match c {
            Component::Normal(os) => Some(to_rust_module_name(&os.to_string_lossy())),
            _ => None,
        })
        .collect();
    if let Some(last) = segments.pop() {
        let stem = last.strip_suffix(".schema.json").unwrap_or(&last).to_string();
        segments.push(stem);
    }
    segments.join("::")
}

/// The identifier an item declares or implements for: `pub struct X` → `X`,
/// `impl X { … }` / `impl From<…> for X` → `X`.
fn item_type_ident(item: &syn::Item) -> Option<String> {
    match item {
        syn::Item::Struct(i) => Some(i.ident.to_string()),
        syn::Item::Enum(i) => Some(i.ident.to_string()),
        syn::Item::Union(i) => Some(i.ident.to_string()),
        syn::Item::Type(i) => Some(i.ident.to_string()),
        syn::Item::Impl(i) => match &*i.self_ty {
            syn::Type::Path(path) => path.path.segments.last().map(|s| s.ident.to_string()),
            _ => None,
        },
        _ => None,
    }
}

/// Type names a module declares at its top level, as emitted.
fn declared_type_names(items: &[syn::Item]) -> BTreeSet<String> {
    items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Struct(i) => Some(i.ident.to_string()),
            syn::Item::Enum(i) => Some(i.ident.to_string()),
            syn::Item::Union(i) => Some(i.ident.to_string()),
            syn::Item::Type(i) => Some(i.ident.to_string()),
            _ => None,
        })
        .collect()
}

/// Drop every declaration/impl of a duplicated name, recursing into the generated
/// submodules: `typify` emits the inline copy's builder (and any nested helper)
/// there too, so the copy must leave as one unit.
fn prune_duplicate_items(items: &mut Vec<syn::Item>, duplicates: &BTreeSet<String>) {
    items.retain(|item| !matches!(item_type_ident(item), Some(ident) if duplicates.contains(&ident)));
    for item in items.iter_mut() {
        if let syn::Item::Mod(module) = item {
            if let Some((_, nested)) = module.content.as_mut() {
                prune_duplicate_items(nested, duplicates);
            }
        }
    }
}

/// True when `haystack` mentions `ident` as a whole word.
fn mentions_ident(haystack: &str, ident: &str) -> bool {
    let bytes = haystack.as_bytes();
    let mut from = 0;
    while let Some(pos) = haystack[from..].find(ident) {
        let at = from + pos;
        let end = at + ident.len();
        let before_ok = at == 0 || !is_ident_byte(bytes[at - 1]);
        let after_ok = end >= bytes.len() || !is_ident_byte(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
        from = at + 1;
    }
    false
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Remove the inline duplicates of `$ref` targets from a generated module so the
/// standalone module stays the single canonical home, then import what the pruned
/// module still uses.
///
/// `typify` emits a standalone type for every referenced schema *and* a
/// wire-identical copy inside each referencing file. Only names with a unique
/// canonical home are touched; everything else (including genuinely local nested
/// types, and names that collide ambiguously) is left alone.
fn dedupe_ref_targets(
    file: &mut syn::File,
    self_module: &str,
    index: &RefTargetIndex,
) -> Result<(), String> {
    let mut duplicates: BTreeMap<String, String> = BTreeMap::new();
    for item in &file.items {
        if let Some(ident) = item_type_ident(item) {
            if let Some(owner) = index.owner_of(&ident, self_module) {
                duplicates.insert(ident, owner.to_string());
            }
        }
    }
    if duplicates.is_empty() {
        return Ok(());
    }

    let names: BTreeSet<String> = duplicates.keys().cloned().collect();
    prune_duplicate_items(&mut file.items, &names);

    // helper of the copy); import just the names the pruned module still uses.
    let pruned = prettyplease::unparse(file);
    let mut imports: Vec<syn::Item> = Vec::new();
    for (name, module) in &duplicates {
        if !mentions_ident(&pruned, name) {
            continue;
        }
        let path = format!("crate::generated::{module}::{name}");
        imports.push(syn::Item::Use(
            syn::parse_str::<syn::ItemUse>(&format!("use {path};"))
                .map_err(|err| format!("invalid generated import `{path}`: {err}"))?,
        ));
    }
    if !imports.is_empty() {
        imports.append(&mut file.items);
        file.items = imports;
    }
    Ok(())
}

/// True when `path` ends in the given trait/type segment.
fn path_ends_with(path: &syn::Path, segment: &str) -> bool {
    path.segments
        .last()
        .is_some_and(|last| last.ident == segment)
}

/// The local type name a generated impl targets: a single-segment path (`Pid`)
/// or an explicit `self::` prefix (`self::Pid`). Any other multi-segment path
/// names a different type and is rejected — never matched by suffix.
fn local_self_type(ty: &syn::Type) -> Option<String> {
    let syn::Type::Path(path) = ty else {
        return None;
    };
    if path.qself.is_some() || path.path.leading_colon.is_some() {
        return None;
    }
    let segments: Vec<_> = path.path.segments.iter().collect();
    match segments.as_slice() {
        [only] => Some(only.ident.to_string()),
        [prefix, only] if prefix.ident == "self" => Some(only.ident.to_string()),
        _ => None,
    }
}

/// True when `ty` is `&str`.
fn is_str_ref(ty: &syn::Type) -> bool {
    matches!(
        ty,
        syn::Type::Reference(reference)
            if matches!(&*reference.elem, syn::Type::Path(path) if path.path.is_ident("str"))
    )
}

/// True when `block` is `impl TryFrom<&str> for _` (any path spelling of the
/// trait, e.g. `::core::convert::TryFrom`).
fn try_from_str_impl(block: &syn::ItemImpl) -> bool {
    let Some((trait_path, _)) = block.trait_.as_ref() else {
        return false;
    };
    if !path_ends_with(trait_path, "TryFrom") {
        return false;
    }
    let Some(last) = trait_path.segments.last() else {
        return false;
    };
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return false;
    };
    matches!(args.args.first(), Some(syn::GenericArgument::Type(arg)) if is_str_ref(arg))
}

/// True when `stmt` is the injected `if value.trim() != value { return Err(..) }`
/// guard, so a second pass cannot stack a duplicate.
fn is_trim_guard(stmt: &syn::Stmt) -> bool {
    let syn::Stmt::Expr(syn::Expr::If(if_expr), _) = stmt else {
        return false;
    };
    let syn::Expr::Binary(binary) = &*if_expr.cond else {
        return false;
    };
    if !matches!(binary.op, syn::BinOp::Ne(_)) {
        return false;
    }
    let syn::Expr::MethodCall(call) = &*binary.left else {
        return false;
    };
    call.method == "trim"
        && matches!(&*binary.right, syn::Expr::Path(path) if path.path.is_ident("value"))
}

/// Primitive integers a generated newtype may wrap.
const PRIMITIVE_INTS: &[&str] = &[
    "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize",
];

/// `Some(primitive)` when `ty` is exactly one of [`PRIMITIVE_INTS`].
fn primitive_int(ty: &syn::Type) -> Option<&'static str> {
    let syn::Type::Path(path) = ty else {
        return None;
    };
    if path.qself.is_some() {
        return None;
    }
    PRIMITIVE_INTS
        .iter()
        .copied()
        .find(|primitive| path.path.is_ident(primitive))
}

/// Local single-field tuple structs over a primitive integer: name → primitive.
fn primitive_tuple_newtypes(items: &[syn::Item]) -> BTreeMap<String, &'static str> {
    let mut newtypes = BTreeMap::new();
    for item in items {
        let syn::Item::Struct(strukt) = item else {
            continue;
        };
        let syn::Fields::Unnamed(fields) = &strukt.fields else {
            continue;
        };
        if fields.unnamed.len() != 1 {
            continue;
        }
        let Some(primitive) = fields.unnamed.first().and_then(|field| primitive_int(&field.ty))
        else {
            continue;
        };
        newtypes.insert(strukt.ident.to_string(), primitive);
    }
    newtypes
}

/// The primitive an integer-shaped newtype parses into, read from its
/// `type Err = <u64 as ::std::str::FromStr>::Err;` associated type. Requires the
/// qself trait to be `FromStr` and the qself type to be a primitive integer.
fn from_str_err_primitive(items: &[syn::ImplItem]) -> Option<&'static str> {
    items.iter().find_map(|item| {
        let syn::ImplItem::Type(assoc) = item else {
            return None;
        };
        if assoc.ident != "Err" {
            return None;
        }
        let syn::Type::Path(err_ty) = &assoc.ty else {
            return None;
        };
        if !path_ends_with(&err_ty.path, "Err") {
            return None;
        }
        let qself = err_ty.qself.as_ref()?;
        let trait_segment = err_ty
            .path
            .segments
            .iter()
            .take(qself.position)
            .next_back()?;
        if trait_segment.ident != "FromStr" {
            return None;
        }
        primitive_int(&qself.ty)
    })
}

/// `Some((name, borrowed))` for `impl TryFrom<&str> for <name>` (`borrowed`) and
/// `impl TryFrom<String> for <name>` (`owned`).
fn try_from_target(item: &syn::Item) -> Option<(String, bool)> {
    let syn::Item::Impl(block) = item else {
        return None;
    };
    let (trait_path, _) = block.trait_.as_ref()?;
    if !path_ends_with(trait_path, "TryFrom") {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &trait_path.segments.last()?.arguments else {
        return None;
    };
    let syn::GenericArgument::Type(arg) = args.args.first()? else {
        return None;
    };
    let borrowed = match arg {
        syn::Type::Reference(reference)
            if matches!(&*reference.elem, syn::Type::Path(path) if path.path.is_ident("str")) =>
        {
            true
        }
        syn::Type::Path(path) if path_ends_with(&path.path, "String") => false,
        _ => return None,
    };
    Some((local_self_type(&block.self_ty)?, borrowed))
}

/// typify 0.10 only emits `TryFrom<&str>` / `TryFrom<String>` for string
/// newtypes, which drops two public conversions from primitive newtypes (e.g.
/// `Pid(pub u64)`); restore whichever of the two is missing (borrowed and owned
/// checked independently) as delegations to the generated `FromStr` so the 0.8
/// conversion surface stays available to out-of-repo consumers.
fn restore_primitive_newtype_try_from(file: &mut syn::File) -> Result<(), String> {
    let newtypes = primitive_tuple_newtypes(&file.items);
    let mut insertions: Vec<(usize, Vec<syn::Item>)> = Vec::new();
    for (index, item) in file.items.iter().enumerate() {
        let syn::Item::Impl(block) = item else {
            continue;
        };
        let Some((trait_path, _)) = block.trait_.as_ref() else {
            continue;
        };
        if !path_ends_with(trait_path, "FromStr") {
            continue;
        }
        let Some(name) = local_self_type(&block.self_ty) else {
            continue;
        };
        // `FromStr` impl parses into qualifies; everything else is skipped.
        let Some(inner) = from_str_err_primitive(&block.items) else {
            continue;
        };
        if newtypes.get(&name) != Some(&inner) {
            continue;
        }
        let (has_borrowed, has_owned) = file
            .items
            .iter()
            .filter_map(try_from_target)
            .filter(|(target, _)| target == &name)
            .fold((false, false), |(borrowed, owned), (_, is_borrowed)| {
                (borrowed || is_borrowed, owned || !is_borrowed)
            });
        let missing: Vec<&str> = match (has_borrowed, has_owned) {
            (false, false) => vec!["&str", "String"],
            (false, true) => vec!["&str"],
            (true, false) => vec!["String"],
            (true, true) => continue,
        };
        let restored: Vec<syn::Item> = missing
            .into_iter()
            .map(|arg| {
                syn::parse_str(&format!(
                    "impl ::std::convert::TryFrom<{arg}> for {name} {{ \
                     type Error = <{inner} as ::std::str::FromStr>::Err; \
                     fn try_from(value: {arg}) -> ::std::result::Result<Self, Self::Error> {{ value.parse() }} }}"
                ))
                .map_err(|err| format!("restore TryFrom<{arg}> for {name}: {err}"))
            })
            .collect::<Result<_, _>>()?;
        insertions.push((index, restored));
    }
    for (index, items) in insertions.into_iter().rev() {
        file.items.splice(index + 1..index + 1, items);
    }
    Ok(())
}

/// Formatting pass: render a module as pretty Rust after removing any
/// `$ref`-target duplicate.
fn render_module_body(
    file: &mut syn::File,
    self_module: &str,
    index: &RefTargetIndex,
) -> Result<String, String> {
    inject_character_display_name_trim(file)?;
    restore_primitive_newtype_try_from(file)?;
    dedupe_ref_targets(file, self_module, index)?;
    Ok(prettyplease::unparse(file))
}

/// Generate Rust source for a single schema via `typify` and return the module
/// body (schema header excluded).
///
/// `rel` is the schema's path relative to the schemas dir (POSIX or platform); the
/// caller renders it verbatim into the file header as the canonical source pointer.
fn generate_schema_rust(
    schema_path: &Path,
    rel: &Path,
    src_schema_path: &Path,
) -> Result<String, String> {
    let content = fs::read_to_string(schema_path)
        .map_err(|err| format!("failed to read {}: {err}", schema_path.display()))?;
    let mut schema: Schema = serde_json::from_str(&content)
        .map_err(|err| format!("invalid JSON Schema in {}: {err}", schema_path.display()))?;

    // handing it to `typify`. Without this, `typify` names the root type from the
    // raw title (e.g. `"Nexus World Entity"` → `NexusWorldEntity`, `"Nexus Delta"`
    // → `NexusDelta`) or emits no root type at all when the title is absent
    // (e.g. `work-summary`). Overriding aligns the emitted name with the TS contract
    // (`schemaToTypeName`) and the drift-test `entry!` registrations (`World`,
    // `WorkSummary`, `ForkBranch`, …). Only the in-memory root `title` entry is
    // touched; other metadata (description, $id, …) is preserved. The schemars-0.8
    // `RootSchema` parser required an object root, so a non-object root stays a
    // hard error here rather than being silently coerced.
    let file_name = rel
        .file_name()
        .and_then(|s| s.to_str())
        .expect("schema rel path has a file name");
    let type_name = schema_type_name(file_name);
    schema
        .as_object_mut()
        .ok_or_else(|| format!("root schema must be an object: {}", schema_path.display()))?
        .insert("title".to_owned(), Value::String(type_name.clone()));

    // definition): the prep stage inlines the referenced schema verbatim, so
    // typify names the merged root type from the REFERENCED member's `title`
    // (e.g. `NexusCharacterDetail`), drifting from the basename-derived TS
    // contract name set above. Capture that member-derived name; after
    // generation the emitted identifiers are renamed to the contract name, and
    // derived helper names (builders, nested newtypes) shift coherently.
    let member_type_name: Option<String> = schema
        .as_object()
        .and_then(|obj| obj.get("allOf"))
        .and_then(|value| value.as_array())
        .and_then(|members| members.first())
        .and_then(|member| member.get("title"))
        .and_then(|title| title.as_str())
        .map(title_to_type_name);

    // own schema representation.
    let typify_schema = serde_json::from_value(schema.to_value())
        .map_err(|err| format!("invalid typify input in {}: {err}", schema_path.display()))?;
    let mut type_space = TypeSpace::new(
        TypeSpaceSettings::default().map_typespace_settings(|s| s.with_struct_builder(true)),
    );
    type_space
        .add_root_schema(typify_schema)
        .map_err(|err| format!("typify failed for {}: {err}", schema_path.display()))?;

    let mut rust = rewrite_unicode_scalar_length_checks(
        &type_space
            .to_stream()
            .map_err(|err| format!("typify failed for {}: {err}", schema_path.display()))?
            .to_string(),
    );
    if rust.trim().is_empty() {
        return Err(format!(
            "typify produced empty output for {}",
            schema_path.display()
        ));
    }
    if let Some(from) = member_type_name {
        if from != type_name {
            rust = rename_identifier_prefix(&rust, &from, &type_name);
        }
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
    rust = apply_nested_session_viewpoint_order(&rust, src_schema_path);

    let marked = tri_state_fields(src_schema_path);
    if !marked.is_empty() {
        rust = inject_tri_state_deserializers(&rust, &type_name, &marked);
    }
    let literal_true = literal_true_fields(src_schema_path);
    if !literal_true.is_empty() {
        rust = inject_literal_true_serde_attributes(&rust, &type_name, &literal_true);
    }

    Ok(rust)
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
    let mut file_dirs: BTreeSet<PathBuf> = BTreeSet::new();
    for m in modules {
        let abs = out_root.join(&m.out_rel);
        if let Some(parent) = abs.parent() {
            file_dirs.insert(parent.to_path_buf());
        }
    }

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

        let mut child_dirs: Vec<String> = all_dirs
            .iter()
            .filter(|other| other.parent() == Some(dir.as_path()))
            .filter_map(|other| other.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        child_dirs.sort();

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
    if let Err(err) = run() {
        eprintln!("error: {err}");
        process::exit(1);
    }
}

/// Resolve the lane inputs, then regenerate the Rust contract tree via stage-and-swap.
fn run() -> Result<(), String> {
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
        return Err(
            "dereferenced schema tree not found — run `pnpm run codegen` (stage 1) first, \
             or set NEXUS_DEREF_SCHEMAS_DIR"
                .to_string(),
        );
    }

    // committed tree (bespoke, stale or partial) only once every write succeeded. A
    // failed or aborted run therefore leaves the previous generated tree intact instead
    // of pruning it mid-window; stray files from a previous generator (e.g. a
    // definitions-only emit like `common_types.rs`) still disappear with the swap.
    stage_and_swap(&out_root, |stage_root| {
        generate_tree(&root, &deref_dir, &src_dir, &out_root, stage_root)
    })
}

/// Generate every schema module and barrel `mod.rs` into `stage_root`, keeping the
/// bookkeeping paths (`out_path` / `out_rel`) relative to the final `out_root` so the
/// swap stays a plain directory rename.
fn generate_tree(
    root: &Path,
    deref_dir: &Path,
    src_dir: &Path,
    out_root: &Path,
    stage_root: &Path,
) -> Result<(), String> {
    let pattern = deref_dir.join("**/*.schema.json");
    let mut schema_paths: Vec<PathBuf> = glob(pattern.to_str().expect("glob pattern is literal"))
        .expect("glob pattern compiles")
        .filter_map(Result::ok)
        .collect();
    schema_paths.sort();

    let skip_set: BTreeSet<&str> = SKIP_SCHEMAS.iter().copied().collect();

    let mut generated: BTreeSet<PathBuf> = BTreeSet::new();
    let mut generated_modules: Vec<GeneratedModule> = Vec::new();
    /// One generated module, held in memory until the whole tree generated
    /// cleanly: the formatting/dedupe pass runs before anything is written.
    struct PendingModule {
        out_path: PathBuf,
        rel: PathBuf,
        /// Basename-derived contract name of the schema (the module's root type).
        root_name: String,
        body: String,
    }
    let mut pending: Vec<PendingModule> = Vec::new();
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

        let file_name = rel
            .file_name()
            .and_then(|s| s.to_str())
            .expect("schema rel path has a file name");
        let type_name = schema_type_name(file_name);
        let out_path = output_path(&out_root, rel);
        match generate_schema_rust(schema_path, rel, &src_dir.join(rel)) {
            Ok(body) => pending.push(PendingModule {
                out_path: out_path.clone(),
                rel: rel.to_path_buf(),
                root_name: type_name.clone(),
                body,
            }),
            Err(err) => {
                failures.push(err);
                continue;
            }
        }

        // tree loses the top-level `schema_version` stamp and keeps the raw title).
        let src_path = src_dir.join(rel);
        let export_all = export_mode_for_schema(&src_path);
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
    }

    eprintln!("skipped {skipped} definition-only / canonical-skip schema(s)");

    if !failures.is_empty() {
        return Err(failures.join("\nerror: "));
    }

    if generated.is_empty() {
        return Err(format!(
            "generated 0 Rust schema files (deref dir: {})",
            deref_dir.display()
        ));
    }

    // first write, so the owner index sees the type names modules actually declare
    // and a render failure cannot leave a partially written tree.
    let mut parsed: Vec<(PathBuf, PathBuf, String, syn::File)> = Vec::new();
    let mut render_failures: Vec<String> = Vec::new();
    for module in &pending {
        match syn::parse_file(&module.body) {
            Ok(file) => parsed.push((
                module.out_path.clone(),
                module.rel.clone(),
                module.root_name.clone(),
                file,
            )),
            Err(err) => render_failures.push(format!(
                "{}: parse emitted body: {err}",
                module.rel.display()
            )),
        }
    }
    let ref_targets = RefTargetIndex {
        entries: parsed
            .iter()
            .map(|(_, rel, root, file)| RefTargetEntry {
                root: root.clone(),
                module: generated_module_path(rel),
                declared: declared_type_names(&file.items),
            })
            .collect(),
    };
    let mut rendered: Vec<(PathBuf, PathBuf, String)> = Vec::new();
    for (out_path, rel, _, mut file) in parsed {
        let self_module = generated_module_path(&rel);
        match render_module_body(&mut file, &self_module, &ref_targets) {
            Ok(body) => rendered.push((out_path, rel, body)),
            Err(err) => render_failures.push(format!("{}: {err}", rel.display())),
        }
    }
    if !render_failures.is_empty() {
        return Err(render_failures.join("\nerror: "));
    }

    for (out_path, rel, body) in &rendered {
        let dest = stage_root.join(
            out_path
                .strip_prefix(out_root)
                .expect("output path sits under the output root"),
        );
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("create generated schema output dir: {err}"))?;
        }
        let header = format!(
            "//! AUTO-GENERATED FROM JSON SCHEMA - DO NOT MODIFY\n//! Source: {}\n//! Generated by: pnpm run codegen\n\n",
            rel.display()
        );
        fs::write(&dest, format!("{header}{body}"))
            .map_err(|err| format!("failed to write {}: {err}", out_path.display()))?;
        println!(
            "wrote {}",
            out_path.strip_prefix(root).unwrap_or(out_path).display()
        );
    }

    let provider_host_event_src =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("provider_host_event.rs.template");
    let provider_host_event_out = out_root.join("core").join("provider_host_event.rs");
    if provider_host_event_src.exists() {
        let dest = stage_root.join("core").join("provider_host_event.rs");
        fs::create_dir_all(dest.parent().expect("core parent"))
            .map_err(|err| format!("create core output dir: {err}"))?;
        fs::copy(&provider_host_event_src, &dest)
            .map_err(|err| format!("copy provider_host_event template: {err}"))?;
        generated.insert(provider_host_event_out.clone());
        generated_modules.push(GeneratedModule {
            out_rel: PathBuf::from("core").join("provider_host_event.rs"),
            rust_mod: "provider_host_event".to_string(),
            export_all: false,
            export_type: Some("ProviderHostEvent".to_string()),
        });
        if let Ok(version) =
            read_schema_version(&src_dir.join("core/provider-host-event.schema.json"))
        {
            schema_versions.push(("ProviderHostEvent".to_string(), version));
        }
        println!(
            "wrote {}",
            provider_host_event_out
                .strip_prefix(&root)
                .unwrap_or(&provider_host_event_out)
                .display()
        );
    }

    // `#![allow(...)]` attr + `SCHEMA_VERSIONS` / `LATEST_SCHEMA_VERSION`).
    write_barrels(stage_root, &generated_modules, &schema_versions);

    let latest = schema_versions.iter().map(|(_, v)| *v).max().unwrap_or(0);
    println!(
        "Rust: generated {} schema file(s) + barrel mod.rs tree, {} schema version(s), latest v{latest}",
        generated.len(),
        schema_versions.len()
    );
    Ok(())
}

/// Sibling staging directory for `out_root`: same parent, so the final swap is a single
/// rename within one filesystem.
fn staging_root(out_root: &Path) -> PathBuf {
    let name = out_root
        .file_name()
        .expect("generated tree root has a file name");
    out_root.with_file_name(format!("{}.staging", name.to_string_lossy()))
}

/// Sibling path holding the committed tree while it is retired for a swap: same parent,
/// so retiring it and dropping it are renames/removals within one filesystem.
fn backup_root(out_root: &Path) -> PathBuf {
    let name = out_root
        .file_name()
        .expect("generated tree root has a file name");
    out_root.with_file_name(format!("{}.backup", name.to_string_lossy()))
}

/// Sibling advisory lock file guarding one output directory's whole publication
/// lifecycle, so overlapping invocations cannot share its staging/backup siblings.
fn lock_root(out_root: &Path) -> PathBuf {
    let name = out_root
        .file_name()
        .expect("generated tree root has a file name");
    out_root.with_file_name(format!("{}.lock", name.to_string_lossy()))
}

/// Take the exclusive per-output-directory publication lock, or fail when another run
/// holds it. Backed by the OS advisory file lock (`std::fs::File::try_lock`), which the
/// OS releases when the holder exits — so a crashed run cannot wedge the next one. Only
/// the advisory lock is stateful; the zero-byte lock file is kept beside the output.
fn acquire_publication_lock(out_root: &Path) -> Result<fs::File, String> {
    let lock_path = lock_root(out_root);
    let file = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|err| format!("open publication lock {}: {err}", lock_path.display()))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(fs::TryLockError::WouldBlock) => Err(format!(
            "another codegen run is active for {}; refusing to share its staging tree",
            out_root.display()
        )),
        Err(fs::TryLockError::Error(err)) => Err(format!("lock {}: {err}", lock_path.display())),
    }
}

/// Generate into a sibling staging tree and swap it into place only after `build`
/// succeeds, so a failed, aborted or crashed run cannot prune the existing tree.
///
/// Replacement is *recoverable*, not atomic — `std::fs` exposes no portable directory
/// exchange, so this is two operations with a documented window. The committed tree is
/// retired to the sibling `.backup` path via rename (never a recursive delete of the
/// only copy), the staging tree is renamed into place, and the retired copy is dropped
/// last. A hard kill therefore leaves `out_root` in one of three states, all
/// recoverable on the next run: before the swap → `out_root` absent, the committed tree
/// at `.backup`, restored by `recover_interrupted_publication`; after the swap → the new
/// tree committed, the stale backup dropped; mid-swap → the backup restored in place.
/// `out_root` is thus briefly absent during a swap, but the tree is never observably
/// pruned or partially deleted.
///
/// The whole recover → build → publish lifecycle is serialized per `out_root` by
/// [`acquire_publication_lock`], so a second run targeting the same output fails before
/// it can clear this run's staging tree.
fn stage_and_swap<F>(out_root: &Path, build: F) -> Result<(), String>
where
    F: FnOnce(&Path) -> Result<(), String>,
{
    let _lock = acquire_publication_lock(out_root)?;
    let stage_root = staging_root(out_root);
    // this run could overwrite the recovery material.
    recover_interrupted_publication(out_root)?;
    if stage_root.exists() {
        fs::remove_dir_all(&stage_root)
            .map_err(|err| format!("clear staging tree {}: {err}", stage_root.display()))?;
    }
    fs::create_dir_all(&stage_root)
        .map_err(|err| format!("create staging tree {}: {err}", stage_root.display()))?;

    if let Err(err) = build(&stage_root) {
        let _ = fs::remove_dir_all(&stage_root);
        return Err(err);
    }

    publish(&stage_root, out_root)
}

/// Restore or clear the recovery material a previous interrupted publication left, so a
/// later run never removes it unrecovered. No-op when no backup exists.
fn recover_interrupted_publication(out_root: &Path) -> Result<(), String> {
    let backup_root = backup_root(out_root);
    if !backup_root.exists() {
        return Ok(());
    }
    if out_root.exists() {
        return fs::remove_dir_all(&backup_root)
            .map_err(|err| format!("remove retired generated tree: {err}"));
    }
    fs::rename(&backup_root, out_root).map_err(|err| {
        format!(
            "restore previous generated tree from {}: {err}",
            backup_root.display()
        )
    })
}

/// Retire the committed tree, land the staged one, then drop the retired copy. A failed
/// swap restores the retired tree before returning the error.
fn publish(stage_root: &Path, out_root: &Path) -> Result<(), String> {
    let backup_root = backup_root(out_root);
    if out_root.exists() {
        fs::rename(out_root, &backup_root).map_err(|err| {
            format!(
                "retire previous generated tree to {}: {err}",
                backup_root.display()
            )
        })?;
    }
    if let Err(err) = fs::rename(stage_root, out_root) {
        if backup_root.exists() {
            let _ = fs::rename(&backup_root, out_root);
        }
        return Err(format!("swap staging tree into place: {err}"));
    }
    if backup_root.exists() {
        fs::remove_dir_all(&backup_root)
            .map_err(|err| format!("remove retired generated tree: {err}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scratch dir under the system temp dir — same filesystem as the staged tree, so
    /// the swap is a real rename.
    fn scratch(tag: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("nexus-rust-gen-{tag}-{}", process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    #[test]
    fn failed_build_keeps_the_existing_tree() {
        let sandbox = scratch("abort");
        let out_root = sandbox.join("generated");
        fs::create_dir_all(&out_root).unwrap();
        fs::write(out_root.join("kept.rs"), "// committed").unwrap();

        let err = stage_and_swap(&out_root, |stage_root| {
            fs::write(stage_root.join("half.rs"), "// partial").unwrap();
            Err("boom".to_string())
        })
        .unwrap_err();

        assert_eq!(err, "boom");
        assert!(
            out_root.join("kept.rs").exists(),
            "existing tree was pruned"
        );
        assert!(!out_root.join("half.rs").exists());
        assert!(
            !staging_root(&out_root).exists(),
            "staging tree left behind"
        );
        let _ = fs::remove_dir_all(&sandbox);
    }

    #[test]
    fn successful_build_replaces_the_tree() {
        let sandbox = scratch("swap");
        let out_root = sandbox.join("generated");
        fs::create_dir_all(&out_root).unwrap();
        fs::write(out_root.join("stale.rs"), "// stale").unwrap();

        stage_and_swap(&out_root, |stage_root| {
            fs::write(stage_root.join("fresh.rs"), "// fresh").unwrap();
            Ok(())
        })
        .unwrap();

        assert!(
            !out_root.join("stale.rs").exists(),
            "stale file survived the swap"
        );
        assert!(out_root.join("fresh.rs").exists());
        let _ = fs::remove_dir_all(&sandbox);
    }

    #[test]
    fn failed_publish_restores_the_previous_tree() {
        let sandbox = scratch("publish");
        let out_root = sandbox.join("generated");
        fs::create_dir_all(&out_root).unwrap();
        fs::write(out_root.join("kept.rs"), "// committed").unwrap();

        let err = publish(&staging_root(&out_root), &out_root).unwrap_err();
        assert!(
            err.contains("swap staging tree into place"),
            "unexpected error: {err}"
        );
        assert!(
            out_root.join("kept.rs").exists(),
            "previous tree was not restored"
        );
        assert!(!backup_root(&out_root).exists());
        let _ = fs::remove_dir_all(&sandbox);
    }

    #[test]
    fn recovers_an_interrupted_publication() {
        let sandbox = scratch("recover");
        let out_root = sandbox.join("generated");
        fs::create_dir_all(backup_root(&out_root)).unwrap();
        fs::write(backup_root(&out_root).join("kept.rs"), "// committed").unwrap();

        let err = stage_and_swap(&out_root, |_| Err("boom".to_string())).unwrap_err();

        assert_eq!(err, "boom");
        assert!(
            out_root.join("kept.rs").exists(),
            "recovery material was not restored"
        );
        assert!(!backup_root(&out_root).exists());
        let _ = fs::remove_dir_all(&sandbox);
    }

    #[test]
    fn overlapping_runs_refuse_to_share_the_output() {
        let sandbox = scratch("overlap");
        let out_root = sandbox.join("generated");
        fs::create_dir_all(&out_root).unwrap();

        let (locked_tx, locked_rx) = std::sync::mpsc::channel::<()>();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let holder_root = out_root.clone();
        let holder = std::thread::spawn(move || {
            stage_and_swap(&holder_root, |stage_root| {
                fs::write(stage_root.join("first.rs"), "// first").unwrap();
                locked_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(())
            })
        });

        // rather than clear the staging tree out from under it.
        locked_rx.recv().unwrap();
        let err = stage_and_swap(&out_root, |_| Ok(())).unwrap_err();
        assert!(
            err.contains("another codegen run is active"),
            "overlap must fail visibly, got: {err}"
        );

        release_tx.send(()).unwrap();
        holder.join().unwrap().unwrap();
        assert!(out_root.join("first.rs").exists());

        stage_and_swap(&out_root, |stage_root| {
            fs::write(stage_root.join("second.rs"), "// second").unwrap();
            Ok(())
        })
        .unwrap();
        assert!(out_root.join("second.rs").exists());
        assert!(!out_root.join("first.rs").exists());
        let _ = fs::remove_dir_all(&sandbox);
    }
}
