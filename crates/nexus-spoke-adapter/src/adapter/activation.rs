//! Lore activation engine — scans `WorldKB` entries against assembled moment
//! text and classifies them by `modules.activation` fire-conditions.
//!
//! **V1.149 P0 (DF-74): default-on lore activation engine.** The flag-gated
//! V1.146 spike is promoted to a consumer of the spoke `modules.activation`
//! dialect — the handbook field table is
//! `spoke/.mstar/specs/domain-profile-lore-activation.md` §"`modules.activation`
//! — portable subset" (product detail + acceptance:
//! `.mstar/iterations/v1.149/specs/fl-l-w4-activation.md` §1–§4). Nexus parses
//! the portable subset, never invents nexus-private portable fields, and keeps
//! all matching/scan logic product-local — no `spoke-operations` matchers
//! (spoke owns the dialect wire only).
//!
//! MCA calls `apply_activation` between `WorldKB` fetch and User Knowledge
//! assembly; the engine operates on the already-fetched entries — MCA stays
//! generic (spec Architecture Lock Decision 5).
//!
//! # Emit ordering (V1.149 P0 T3 — spec §4)
//!
//! After classify, `matched` is sorted in place: `constant` seeds first
//! (constant band), then non-constant; within each band `priority`
//! **descending** (higher wins, missing ⇒ 0) → `order` **ascending** (lower
//! first, missing ⇒ 0) → **stable original entry order**. Sort inputs are
//! captured during classify from the parsed config (no re-parse per
//! comparison). `unmatched` is never reordered (order is irrelevant there).
//!
//! Neutral entries (no activation module) carry default sort keys
//! (non-constant, priority 0, order 0), so an all-neutral set keeps its
//! original entry order — byte-identical to V1.146 flag-off (the neutral-only
//! ship guarantee, spec §1 / AC-I1 #2).
//!
//! # Trace reasons (DF-76 inspector surface)
//!
//! Every hit/miss `reason` carries the match mode + logic arm, e.g.
//! `primary-any (literal): matched key [king]` (hit) or
//! `and_all (whole_word): missing keys [horn, bell]` (miss) — human-readable
//! for the future DF-76 assembly inspector.
//!
//! # Logic truth table (handbook §2.1 — replaces the V1.146 primary-only spike)
//!
//! When `secondary_keys` is **absent or empty**, only the primary `keys`
//! participate and `logic` is ignored: the entry fires when **any** primary
//! key matches. When `secondary_keys` is present, `logic` combines both sets:
//!
//! | `logic`   | Entry fires when…                                                        |
//! |-----------|--------------------------------------------------------------------------|
//! | `and_any` | any primary **and** any secondary key match (handbook default)           |
//! | `and_all` | **all** primary **and** all secondary keys match                         |
//! | `not_any` | any primary matches **and** no secondary key matches                     |
//! | `not_all` | any primary matches **and** it is false that every secondary matches     |
//! | unknown   | treated as `and_any`; recorded in the trace                              |
//!
//! # Match modes
//!
//! - `literal` (default) — case-insensitive substring.
//! - `regex` — `regex::Regex` (workspace pin; linear-time by construction, so
//!   ReDoS-immune — F-001 P0 fix wave, replacing the backtracking engine on
//!   this hot path); key ≤ 256 chars (longer keys are skipped with
//!   a trace note), scan text capped at 64 KiB (defense-in-depth), compile
//!   failure → non-match + `"invalid regex"` trace note.
//! - `whole_word` — case-insensitive Unicode-aware word-boundary match.
//!
//! # Neutral entries (the byte-equivalence ship guarantee)
//!
//! Entries with no `modules` map, with `modules` but no `activation`, or with
//! an `activation` module whose `keys` are empty and `constant` is false are
//! **always** in `matched` — identical to V1.146 flag-off behavior. `constant:
//! true` entries are always-on seeds, sorted first by the engine (V1.149 P0 T3
//! ordering — constant band first, spec §4).
//!
//! # Relation hops (V1.149 P1 — spec §5; iteration spec:
//! `.mstar/iterations/v1.149/specs/fl-l-w4-activation.md` §5)
//!
//! When a primary-matched or `constant`-seed entry fires, the engine BFS-
//! expands **up to 2** graph hops over the world's confirmed relation graph
//! and pulls graph-adjacent entries into `matched` ([`apply_activation_with_hops`]).
//! Graph recursion, not string-mention recursion: an author no longer has to
//! keyword-tag every related entry.
//!
//! - **Edge source (`RelationPort` gap):** spoke 0.8.2 `RelationPort` is
//!   get/put only — there is no list-by-entity on the trait. The hop edge
//!   source is the storage list primitive `list_relationships_for_world`
//!   (confirmed graph only) loaded by the inherent adapter helper
//!   [`crate::adapter::relation_port::NexusAdapter::list_hop_edges_for_world`],
//!   mapped to engine-local [`HopEdge`]s. Not a `RelationPort` trait method.
//! - **Adjacency:** each edge is treated as **undirected** for neighbor
//!   discovery (both endpoints connect); the trace still records the stored
//!   `relation_type` / `relation_id`.
//! - **Seeds:** only primary-fired + `constant` entries (neutral entries never
//!   seed a hop — that is what keeps neutral-only Worlds byte-equivalent).
//! - **Dedup:** entries already accepted by the primary pass (neutral ones
//!   included) are pre-marked visited, so hop expansion never re-pulls them;
//!   hop-pulled entries are removed from `unmatched` (matched ∩ unmatched = ∅).
//! - **No re-fire:** hop-pulled entries never re-run [`apply_activation`]
//!   key evaluation (spec Q5) — they enter `matched` with a hop trace row.
//! - **Cycle safety:** `visited` on `entry_id` — A↔B and A→B→A terminate.
//! - **Budget (spec Q1):** when `HopConfig::max_hop_tokens` is set, the caller
//!   passes the cross-domain remainder after reserving personality (never
//!   truncated) and `world_state`/timeline (computed in MCA); the engine
//!   further subtracts the primary-matched KB estimate (chars/4 of
//!   summary-or-name) and stops before adding a hopped entry whose estimate
//!   would exceed the remainder. `None` ⇒ depth + cycle only.
//! - **Trace:** pulled entries carry `hop_origin_entry_id`, `hop_depth`,
//!   `source_relation_type`, `source_relation_id` on [`ActivationTraceEntry`]
//!   (skipped from serialization for primary-only rows).

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;
use serde::{Deserialize, Serialize};

/// Maximum regex key length (architect lock Q6) — chars.
const MAX_REGEX_KEY_CHARS: usize = 256;
/// Scan-text cap for the `regex` match path (architect lock Q6) — chars,
/// consistent with the chars/4 token heuristic used across MCA.
const MAX_REGEX_SCAN_CHARS: usize = 64 * 1024;

/// Token-budget accounting of an activation pass (V1.151 P0, DF-76 spec §2 H3).
///
/// Chars/4 estimates computed by the engine, exposed additively for the
/// inspector packet. No wire change: the values were already computed
/// internally (`apply_activation_with_hops` primary estimate + hop spend);
/// only exposure is new.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationBudget {
    /// Sum of chars/4 estimates over primary-matched entries.
    pub primary_tokens_est: usize,
    /// Sum of chars/4 estimates over hop-pulled entries (0 without hops).
    pub hop_tokens_est: usize,
    /// The caller-provided hop cap (`HopConfig::max_hop_tokens`); `None` ⇒
    /// depth + cycle only (no token stop).
    pub cap: Option<usize>,
    /// `cap` minus the primary + hop estimates (what the hop budget has
    /// left); `None` when no cap was set.
    pub remaining: Option<usize>,
}

/// Result of an activation pass over a set of `WorldKB` entries.
#[derive(Debug, Clone)]
pub struct ActivationResult {
    /// Entries that passed activation (or are `constant` seeds, or have no
    /// activation module — neutral entries are included as matched).
    pub matched: Vec<KnowledgeEntryRecord>,
    /// Entries that did not pass activation and are not constant seeds.
    pub unmatched: Vec<KnowledgeEntryRecord>,
    /// Per-entry fire/miss trace for diagnostics.
    pub trace: Vec<ActivationTraceEntry>,
    /// Token-budget accounting (spec §2 H3): `Some` whenever activation ran
    /// — primary estimate always; hop estimate + cap/remaining only on the
    /// with-hops path.
    pub budget: Option<ActivationBudget>,
}

/// Per-entry activation trace record.
#[derive(Debug, Clone, Serialize)]
pub struct ActivationTraceEntry {
    pub entry_id: String,
    pub canonical_name: String,
    /// Why this entry was matched or not.
    pub reason: String,
    /// Whether the entry ended up in `matched`.
    pub accepted: bool,
    /// V1.149 P1 (spec §5): entry the hop originated from — set only on
    /// hop-pulled rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hop_origin_entry_id: Option<String>,
    /// V1.149 P1: hop depth (1..=`max_hops`) — set only on hop-pulled rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hop_depth: Option<u32>,
    /// V1.149 P1: stored `relation_type` of the edge that first reached the
    /// entry — set only on hop-pulled rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_relation_type: Option<String>,
    /// V1.149 P1: stored `relation_id` of the edge that first reached the
    /// entry — set only on hop-pulled rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_relation_id: Option<String>,
}

/// Engine-local relation edge for hop expansion (spec §5) — **not** a spoke
/// wire type.
///
/// Produced by the adapter inherent loader
/// `NexusAdapter::list_hop_edges_for_world` (the `RelationPort` gap: spoke's
/// port is get/put only, so the storage list primitive is the edge source).
#[derive(Debug, Clone)]
pub struct HopEdge {
    pub relation_id: String,
    pub from_id: String,
    pub to_id: String,
    pub relation_type: String,
}

/// Hop-expansion knobs (spec Q1).
#[derive(Debug, Clone, Copy)]
pub struct HopConfig {
    /// Maximum BFS depth from each seed entry — architect lock: **2**.
    pub max_hops: u32,
    /// Hop token budget (chars/4) **after** the caller's personality +
    /// `world_state`/`timeline` reservations; the engine further subtracts
    /// the primary-matched KB estimate before the hop pass. `None` ⇒ depth +
    /// cycle only (no token stop).
    pub max_hop_tokens: Option<usize>,
}

impl Default for HopConfig {
    fn default() -> Self {
        Self {
            max_hops: 2,
            max_hop_tokens: None,
        }
    }
}

/// Result of [`expand_relation_hops`] — the BFS pull set + its trace rows.
#[derive(Debug, Clone)]
pub struct HopExpandResult {
    /// Entries pulled by the hop BFS, in discovery order (BFS level order).
    pub pulled: Vec<KnowledgeEntryRecord>,
    /// One accepted trace row per pulled entry, carrying the hop fields
    /// (`hop_origin_entry_id`, `hop_depth`, `source_relation_type`,
    /// `source_relation_id`).
    pub hop_trace: Vec<ActivationTraceEntry>,
    /// Sum of chars/4 estimates over pulled entries (V1.151 P0 — the hop
    /// spend, surfaced for the inspector budget section, spec §2 H3).
    pub tokens_consumed: usize,
}

/// Emit-order inputs captured during classify (V1.149 P0 T3 — spec §4).
///
/// Built once per entry from the parsed [`ActivationConfig`]; the sort never
/// re-parses config. Neutral entries (no activation module) get the default:
/// non-constant, priority 0, order 0 — so an all-neutral set keeps its
/// original entry order (byte-equivalence ship guarantee).
#[derive(Debug, Clone, Copy, Default)]
struct ActivationSortKey {
    /// Constant band flag — same seed predicate as [`evaluate_entry`].
    constant: bool,
    /// Primary sort key — higher wins (descending), missing ⇒ 0.
    priority: f64,
    /// Secondary sort key — lower first (ascending), missing ⇒ 0.
    order: f64,
}

impl ActivationSortKey {
    fn from_activation(
        activation: Option<&ActivationConfig>,
        entry_id: &str,
        caller_seed_ids: &[String],
    ) -> Self {
        activation.map_or_else(Self::default, |cfg| Self {
            constant: is_constant_seed(cfg, entry_id, caller_seed_ids),
            priority: cfg.priority,
            order: cfg.order,
        })
    }
}

/// Seed predicate shared by classify + ordering: handbook `constant: true`,
/// V1.146 spike `constant_seeds` self-id, or caller-supplied seed id
/// (spec §2.2 — the spike alias "treats as `constant: true`").
fn is_constant_seed(cfg: &ActivationConfig, entry_id: &str, caller_seed_ids: &[String]) -> bool {
    cfg.constant
        || cfg.constant_seeds.iter().any(|s| s == entry_id)
        || caller_seed_ids.iter().any(|s| s == entry_id)
}

/// The parsed `modules.activation` data from a single entry (handbook shape).
///
/// Deserialization goes through [`ActivationConfigRaw`], which applies the
/// V1.146 spike aliases (`key` → `keys`, `constant_seeds` self-id → `constant`)
/// and ignores unknown fields (consumer-only dialect — no `deny_unknown_fields`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "ActivationConfigRaw")]
struct ActivationConfig {
    /// Primary activation triggers (handbook `keys`).
    keys: Vec<String>,
    /// Secondary / selective triggers evaluated with `logic` (handbook).
    secondary_keys: Vec<String>,
    /// How primary + secondary combine; only meaningful when
    /// `secondary_keys` is non-empty (handbook §2.1).
    logic: String,
    /// Always-on **seed** candidate when `true` (handbook).
    constant: bool,
    /// Insertion / scan order hint — lower first (handbook; sorted by caller).
    order: f64,
    /// Tie-break / budget preference — higher wins (handbook; sorted by caller).
    priority: f64,
    /// Preferred placement (`before_defs`/`after_defs`/`depth`/`outlet`).
    /// Parsed + preserved; not actioned until DF-75.
    position_hint: Option<String>,
    /// Named injection outlet id paired with `position_hint: "outlet"`.
    /// Parsed + preserved; not actioned until DF-75.
    outlet: Option<String>,
    /// How key strings match scanned context (handbook `match`; default `literal`).
    #[serde(rename = "match")]
    match_mode: String,
    /// V1.146 spike alias carrier: `constant_seeds` self-id membership derives
    /// `constant` at evaluation time (spec §2.2). Never exported to the wire.
    #[serde(skip_serializing)]
    constant_seeds: Vec<String>,
}

/// Serde intermediate for `modules.activation` — handbook fields plus the
/// one-minor V1.146 spike aliases (`key`, `constant_seeds`). Unknown fields
/// are ignored (no `deny_unknown_fields`), so portable packs round-trip.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ActivationConfigRaw {
    #[serde(default)]
    keys: Option<Vec<String>>,
    /// Spike alias for `keys` — used only when `keys` is absent (spec §2.2).
    #[serde(default)]
    key: Option<Vec<String>>,
    #[serde(default)]
    secondary_keys: Vec<String>,
    #[serde(default = "default_logic")]
    logic: String,
    #[serde(default)]
    constant: bool,
    /// Spike alias carrier — per-entry self-id membership → `constant: true`.
    #[serde(default)]
    constant_seeds: Vec<String>,
    #[serde(default)]
    order: f64,
    #[serde(default)]
    priority: f64,
    #[serde(default)]
    position_hint: Option<String>,
    #[serde(default)]
    outlet: Option<String>,
    #[serde(default = "default_match_mode", rename = "match")]
    match_mode: String,
}

impl From<ActivationConfigRaw> for ActivationConfig {
    fn from(raw: ActivationConfigRaw) -> Self {
        // Spike alias: prefer `keys` when both are present, else fall back to
        // `key` (spec §2.2 "Prefer `keys` when both present").
        let keys = raw.keys.or(raw.key).unwrap_or_default();
        Self {
            keys,
            secondary_keys: raw.secondary_keys,
            logic: raw.logic,
            constant: raw.constant,
            order: raw.order,
            priority: raw.priority,
            position_hint: raw.position_hint,
            outlet: raw.outlet,
            match_mode: raw.match_mode,
            constant_seeds: raw.constant_seeds,
        }
    }
}

fn default_logic() -> String {
    "and_any".to_string()
}

fn default_match_mode() -> String {
    "literal".to_string()
}

/// Why a single key could not be evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MatchKeyError {
    /// Regex key exceeds the length cap (architect lock Q6) — skipped.
    OverlongKey,
    /// Pattern failed to compile — treated as a non-match.
    InvalidRegex,
}

/// Evaluate one activation key against the scan text under `mode`.
///
/// `scan_raw` is the unconverted scan text (regex path); `scan_lower` is the
/// lowercased form used by `literal` / `whole_word` (spec §3 match casing —
/// lowercasing applies only to those modes; regex case-folding is the
/// pattern's business). Unknown modes fall back to `literal` (handbook default).
///
/// Regex notes (F-001, P0 fix wave): the `regex` crate is linear-time, so a
/// catastrophic pattern like `(a+)+b` cannot hang the engine (the previous
/// backtracking engine allowed exponential worst-case time on this default-on
/// hot path). `regex` does not support lookaround/backreferences — such
/// patterns fail to compile and degrade to a traced non-match.
fn match_key(
    mode: &str,
    key: &str,
    scan_raw: &str,
    scan_lower: &str,
) -> Result<bool, MatchKeyError> {
    match mode.to_ascii_lowercase().as_str() {
        "regex" => {
            if key.chars().count() > MAX_REGEX_KEY_CHARS {
                return Err(MatchKeyError::OverlongKey);
            }
            let re = regex::Regex::new(key).map_err(|_| MatchKeyError::InvalidRegex)?;
            // Truncate with a stable prefix (architect lock Q6; kept as
            // defense-in-depth — the engine is linear-time regardless).
            let capped = truncate_chars(scan_raw, MAX_REGEX_SCAN_CHARS);
            Ok(re.is_match(capped))
        }
        "whole_word" => Ok(whole_word_match(key, scan_lower)),
        _ => Ok(literal_match(key, scan_lower)),
    }
}

fn literal_match(key: &str, scan_lower: &str) -> bool {
    scan_lower.contains(&key.to_lowercase())
}

/// Case-insensitive word-boundary match (Unicode-aware: boundary = not
/// alphanumeric and not `_`).
fn whole_word_match(key: &str, scan_lower: &str) -> bool {
    let key_lower = key.to_lowercase();
    if key_lower.is_empty() {
        return true;
    }
    let mut offset = 0;
    while let Some(rel) = scan_lower[offset..].find(&key_lower) {
        let start = offset + rel;
        let end = start + key_lower.len();
        let before_ok = start == 0 || !is_word_char(scan_lower[..start].chars().next_back());
        let after_ok = end == scan_lower.len() || !is_word_char(scan_lower[end..].chars().next());
        if before_ok && after_ok {
            return true;
        }
        // Advance past this occurrence by one full char (not one byte):
        // `start + 1` sliced mid-char for multi-byte keys and panicked
        // ("byte index is not a char boundary") — e.g. CJK keys. `start` is
        // always a char boundary and `key_lower` is non-empty, so this is
        // always `Some`; the `map_or` default is defensive only.
        offset = start + scan_lower[start..].chars().next().map_or(1, char::len_utf8);
    }
    false
}

fn is_word_char(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_alphanumeric() || c == '_')
}

/// First `max_chars` characters of `text` (stable prefix).
fn truncate_chars(text: &str, max_chars: usize) -> &str {
    text.char_indices()
        .nth(max_chars)
        .map_or(text, |(idx, _)| &text[..idx])
}

/// Evaluate a key list: returns `(matched keys, skip/error notes)`.
fn eval_keys<'a>(
    keys: &'a [String],
    mode: &str,
    scan_raw: &str,
    scan_lower: &str,
) -> (Vec<&'a str>, Vec<String>) {
    let mut hits = Vec::new();
    let mut notes = Vec::new();
    for key in keys {
        match match_key(mode, key, scan_raw, scan_lower) {
            Ok(true) => hits.push(key.as_str()),
            Ok(false) => {}
            Err(MatchKeyError::OverlongKey) => {
                notes.push(format!(
                    "regex key over {MAX_REGEX_KEY_CHARS} chars skipped"
                ));
            }
            Err(MatchKeyError::InvalidRegex) => notes.push("invalid regex".to_string()),
        }
    }
    (hits, notes)
}

/// Keys in `keys` that are not present in `hits`.
fn missing_keys<'a>(keys: &'a [String], hits: &[&'a str]) -> Vec<&'a str> {
    keys.iter()
        .filter(|k| !hits.contains(&k.as_str()))
        .map(String::as_str)
        .collect()
}

fn miss_reason(arm: &str, mode: &str, scanned: usize, notes: &[String]) -> String {
    if notes.is_empty() {
        format!("{arm} ({mode}): no key matched ({scanned} keys scanned)")
    } else {
        format!(
            "{arm} ({mode}): no key matched ({scanned} keys scanned; {})",
            notes.join("; ")
        )
    }
}

/// Apply lore activation to a set of `WorldKB` entries.
///
/// For each entry:
/// 1. No `modules.activation` → "neutral", included in `matched` (byte-
///    equivalence guarantee — identical to V1.146 flag-off).
/// 2. `constant: true` (or V1.146 `constant_seeds` self-id, or caller-supplied
///    seed id) → always-on seed, included in `matched`.
/// 3. `keys` empty (and not constant) → neutral, included in `matched`.
/// 4. Otherwise evaluate the handbook truth table over `keys` +
///    `secondary_keys` under `match` mode, with per-entry self-match text
///    (`canonical_name` + `body.summary`) appended to the scan (spec §3 #4).
///
/// # Arguments
///
/// * `entries` — Slice of `WorldKB` entries to classify.
/// * `scan_text` — The assembled scan text (Stage-0 + outline beats as wired
///   by MCA; per-entry self-match is appended internally).
/// * `constant_seed_ids` — Caller-supplied entry IDs that are always included
///   (tests/config; merged with per-entry `constant`/`constant_seeds`).
#[must_use]
pub fn apply_activation(
    entries: &[KnowledgeEntryRecord],
    scan_text: &str,
    constant_seed_ids: &[String],
) -> ActivationResult {
    let mut pass = run_primary_pass(entries, scan_text, constant_seed_ids);
    sort_matched(&mut pass.matched);
    // V1.151 P0 (spec §2 H3): no-hops path — primary estimate only,
    // `cap`/`remaining` = `None` (no hop budget active).
    let primary_tokens_est: usize = pass
        .matched
        .iter()
        .map(|(entry, _)| estimate_tokens(entry))
        .sum();
    ActivationResult {
        matched: pass.matched.into_iter().map(|(entry, _)| entry).collect(),
        unmatched: pass.unmatched,
        trace: pass.trace,
        budget: Some(ActivationBudget {
            primary_tokens_est,
            hop_tokens_est: 0,
            cap: None,
            remaining: None,
        }),
    }
}

/// Apply lore activation **and** relation-hop expansion (V1.149 P1, spec §5).
///
/// Runs the same primary classification pass as [`apply_activation`], then —
/// when `edges` is non-empty and `hop.max_hops > 0` — BFS-expands up to
/// `hop.max_hops` hops from every **seed** entry (primary-fired or `constant`
/// only; neutral entries never seed) and pulls graph-adjacent entries into
/// `matched`. Pulled entries are **not** re-evaluated against
/// `modules.activation` keys (spec Q5) and carry hop fields on their trace
/// rows. Entries already accepted by the primary pass (neutral ones included)
/// are pre-marked visited, so hop expansion never pulls (or duplicates) an
/// entry already in `matched`; a hop-pulled entry leaves `unmatched`.
///
/// Emit ordering after merge is the spec §4 sort (constant band first,
/// priority desc → order asc → stable): pulled entries join the non-constant
/// band with their own `priority`/`order` keys; on exact sort-key ties the
/// stable sort keeps primary-pass entries before pulled ones.
///
/// # Budget contract (spec Q1 — split between caller and engine)
///
/// `hop.max_hop_tokens` is the caller-provided cap: MCA passes the
/// cross-domain remainder **after** reserving personality (never truncated),
/// `world_state` and `timeline` at chars/4. The engine then subtracts the
/// primary-matched KB estimate (same chars/4 estimator) and stops before
/// adding a hopped entry whose estimate would exceed the remainder. `None`
/// ⇒ depth + cycle only.
///
/// # Arguments
///
/// * `entries` — Slice of `WorldKB` entries to classify (the full candidate
///   pool — hop expansion only reaches entries present here; an edge endpoint
///   outside the pool is skipped).
/// * `scan_text` — See [`apply_activation`].
/// * `constant_seed_ids` — See [`apply_activation`].
/// * `edges` — Confirmed relation edges of the world (engine-local
///   [`HopEdge`]s; loaded by the adapter helper, never by MCA itself).
/// * `hop` — Hop knobs ([`HopConfig`]).
#[must_use]
pub fn apply_activation_with_hops(
    entries: &[KnowledgeEntryRecord],
    scan_text: &str,
    constant_seed_ids: &[String],
    edges: &[HopEdge],
    hop: &HopConfig,
) -> ActivationResult {
    let mut primary = run_primary_pass(entries, scan_text, constant_seed_ids);
    // V1.151 P0 (spec §2 H3): record the already-computed primary estimate
    // for the inspector budget section.
    let primary_tokens_est: usize = primary
        .matched
        .iter()
        .map(|(entry, _)| estimate_tokens(entry))
        .sum();
    let mut hop_tokens_est: usize = 0;

    if !edges.is_empty() && hop.max_hops > 0 {
        // Spec Q1: the caller already reserved personality/world_state/
        // timeline; the engine reserves the primary-matched KB estimate.
        let budget = hop
            .max_hop_tokens
            .map(|b| b.saturating_sub(primary_tokens_est));
        let config = HopConfig {
            max_hops: hop.max_hops,
            max_hop_tokens: budget,
        };
        let pool: HashMap<String, KnowledgeEntryRecord> = entries
            .iter()
            .map(|entry| (entry.entry_id.clone(), entry.clone()))
            .collect();
        // QC C-001: every primary-matched entry (neutral ones included) is
        // pre-marked visited, so hop expansion never re-pulls an entry that
        // is already in `matched` — and never traverses through it.
        let pre_visited: Vec<String> = primary
            .matched
            .iter()
            .map(|(entry, _)| entry.entry_id.clone())
            .collect();
        let expanded = expand_relation_hops(&primary.seed_ids, &pre_visited, &pool, edges, &config);
        hop_tokens_est = expanded.tokens_consumed;

        // QC2 F-001: a primary-missed entry that the hop pass pulled must not
        // also stay in `unmatched` — matched ∩ unmatched = ∅ after this.
        let pulled_ids: HashSet<String> = expanded
            .pulled
            .iter()
            .map(|entry| entry.entry_id.clone())
            .collect();
        primary
            .unmatched
            .retain(|entry| !pulled_ids.contains(&entry.entry_id));

        primary
            .matched
            .extend(expanded.pulled.into_iter().map(|entry| {
                let sort_key = ActivationSortKey::from_activation(
                    parse_activation(&entry).as_ref(),
                    &entry.entry_id,
                    constant_seed_ids,
                );
                (entry, sort_key)
            }));
        primary.trace.extend(expanded.hop_trace);
    }

    sort_matched(&mut primary.matched);
    ActivationResult {
        matched: primary
            .matched
            .into_iter()
            .map(|(entry, _)| entry)
            .collect(),
        unmatched: primary.unmatched,
        trace: primary.trace,
        // V1.151 P0 (spec §2 H3): `cap` is the caller's hop cap; `remaining`
        // is what the hop budget has left after primary + hop spends.
        budget: Some(ActivationBudget {
            primary_tokens_est,
            hop_tokens_est,
            cap: hop.max_hop_tokens,
            remaining: hop.max_hop_tokens.map(|cap| {
                cap.saturating_sub(primary_tokens_est)
                    .saturating_sub(hop_tokens_est)
            }),
        }),
    }
}

/// BFS/level hop expansion (V1.149 P1 — spec §5).
///
/// From `seeds` (primary-fired/`constant` entry ids), walks the **undirected**
/// adjacency of `edges` level by level, up to `config.max_hops`, pulling
/// entries that exist in `entry_pool` into `pulled` with a hop trace row
/// (`hop_origin_entry_id` = the origin entry of the edge that first reached
/// it, `hop_depth`, `source_relation_type`, `source_relation_id`).
///
/// `pre_visited` carries entry ids that are **already accepted** by the caller
/// (the primary `matched` set, neutral entries included). They are pre-marked
/// `visited` so they are never pulled a second time and never act as BFS
/// waypoints — only `seeds` start the traversal (spec §5 seed rule).
///
/// Guarantees:
/// - **Cycle-safe:** `visited` on `entry_id` (seeds and `pre_visited`
///   pre-marked; A↔B and A→B→A terminate; self-loops are no-ops).
/// - **No re-fire:** pulled entries are never evaluated against
///   `modules.activation` keys (spec Q5).
/// - **Budget:** when `config.max_hop_tokens` is `Some`, an entry whose
///   estimate (`chars/4` of summary-or-name) would exceed the remaining
///   budget is skipped (and marked visited — the budget is monotonic, so it
///   can never fit later); over-budget skips are silent (no trace row).
///   `None` ⇒ depth + cycle only.
/// - **Deterministic:** seeds are processed in order, edges in slice order;
///   the first edge to reach an entry wins its trace.
///
/// An edge endpoint that is not a candidate entry (outside `entry_pool`, e.g.
/// a key block not in the fetched set) is skipped silently — hop expansion
/// only reaches entries present in the candidate pool.
// plan-locked signature (plan T1): engine-internal `HashMap<String, KnowledgeEntryRecord>`
// with the default hasher — generalizing over hashers would be dead flexibility.
#[allow(clippy::implicit_hasher)]
#[must_use]
pub fn expand_relation_hops(
    seeds: &[String],
    pre_visited: &[String],
    entry_pool: &HashMap<String, KnowledgeEntryRecord>,
    edges: &[HopEdge],
    config: &HopConfig,
) -> HopExpandResult {
    // Undirected adjacency index: every edge connects both endpoints (spec
    // §5 adjacency rule), so both endpoints carry the edge.
    let mut adjacency: HashMap<&str, Vec<&HopEdge>> = HashMap::new();
    for edge in edges {
        adjacency
            .entry(edge.from_id.as_str())
            .or_default()
            .push(edge);
        adjacency.entry(edge.to_id.as_str()).or_default().push(edge);
    }

    let mut visited: HashSet<String> = seeds.iter().chain(pre_visited).cloned().collect();
    let mut pulled: Vec<KnowledgeEntryRecord> = Vec::new();
    let mut hop_trace: Vec<ActivationTraceEntry> = Vec::new();
    let mut remaining = config.max_hop_tokens.unwrap_or(usize::MAX);
    // V1.151 P0 (spec §2 H3): hop spend = sum of estimates over pulled
    // entries — surfaced on the result for the inspector budget section.
    let mut tokens_consumed: usize = 0;

    let mut frontier: Vec<String> = seeds.to_vec();
    for depth in 1..=config.max_hops {
        let mut next_level: Vec<String> = Vec::new();
        for origin_id in &frontier {
            let Some(origin_edges) = adjacency.get(origin_id.as_str()) else {
                continue;
            };
            for edge in origin_edges {
                let neighbor_id = if edge.from_id == *origin_id {
                    &edge.to_id
                } else {
                    &edge.from_id
                };
                if !visited.insert(neighbor_id.clone()) {
                    continue;
                }
                let Some(entry) = entry_pool.get(neighbor_id) else {
                    continue;
                };
                let estimate = estimate_tokens(entry);
                if estimate > remaining {
                    continue;
                }
                remaining -= estimate;
                tokens_consumed += estimate;
                hop_trace.push(ActivationTraceEntry {
                    entry_id: neighbor_id.clone(),
                    canonical_name: entry.canonical_name.clone(),
                    reason: format!(
                        "relation hop (depth {depth}): {} from {origin_id}",
                        edge.relation_type
                    ),
                    accepted: true,
                    hop_origin_entry_id: Some(origin_id.clone()),
                    hop_depth: Some(depth),
                    source_relation_type: Some(edge.relation_type.clone()),
                    source_relation_id: Some(edge.relation_id.clone()),
                });
                pulled.push(entry.clone());
                next_level.push(neighbor_id.clone());
            }
        }
        frontier = next_level;
    }

    HopExpandResult {
        pulled,
        hop_trace,
        tokens_consumed,
    }
}

/// Token estimate for one entry: `chars/4` of summary-or-name (plan T1; the
/// same estimator reserves the primary-matched KB cost and gates each hop).
fn estimate_tokens(entry: &KnowledgeEntryRecord) -> usize {
    let text = entry
        .body
        .as_ref()
        .and_then(|body| body.summary.as_ref())
        .map_or(entry.canonical_name.as_str(), String::as_str);
    text.chars().count() / 4
}

/// The primary classification pass shared by [`apply_activation`] and
/// [`apply_activation_with_hops`].
///
/// In addition to the P0 output (`matched` with sort keys, `unmatched`,
/// `trace`), it derives `seed_ids` — the ids of accepted entries that
/// primary-fired or are `constant` seeds (neutral entries never seed a hop,
/// which keeps neutral-only Worlds byte-equivalent under hops).
struct PrimaryPassOutput {
    matched: Vec<(KnowledgeEntryRecord, ActivationSortKey)>,
    unmatched: Vec<KnowledgeEntryRecord>,
    trace: Vec<ActivationTraceEntry>,
    seed_ids: Vec<String>,
}

fn run_primary_pass(
    entries: &[KnowledgeEntryRecord],
    scan_text: &str,
    constant_seed_ids: &[String],
) -> PrimaryPassOutput {
    let base_lower = scan_text.to_lowercase();
    let mut matched: Vec<(KnowledgeEntryRecord, ActivationSortKey)> = Vec::new();
    let mut unmatched = Vec::new();
    let mut trace = Vec::new();
    let mut seed_ids = Vec::new();

    for entry in entries {
        let entry_id = entry.entry_id.clone();
        let canonical_name = entry.canonical_name.clone();

        // Per-entry self-match text (V1.146 behavior, spec §3 source 4):
        // canonical_name + body.summary appended to the external scan.
        let mut entry_raw = canonical_name.clone();
        if let Some(ref body) = entry.body {
            if let Some(ref summary) = body.summary {
                entry_raw.push(' ');
                entry_raw.push_str(summary);
            }
        }
        let scan_raw = format!("{scan_text}\n{entry_raw}");
        let scan_lower = format!("{base_lower}\n{}", entry_raw.to_lowercase());

        let activation = parse_activation(entry);

        let (accepted, reason) = activation.as_ref().map_or_else(
            || {
                // No activation module → neutral, always included.
                (true, "no activation module".to_string())
            },
            |cfg| evaluate_entry(cfg, &entry_id, &scan_raw, &scan_lower, constant_seed_ids),
        );

        trace.push(ActivationTraceEntry {
            entry_id: entry_id.clone(),
            canonical_name: canonical_name.clone(),
            reason: reason.clone(),
            accepted,
            hop_origin_entry_id: None,
            hop_depth: None,
            source_relation_type: None,
            source_relation_id: None,
        });

        // V1.149 P1: hop seeds = accepted entries that primary-fired or are
        // `constant` — NOT neutral entries (spec §5 seed rule).
        if accepted
            && activation.as_ref().is_some_and(|cfg| {
                is_constant_seed(cfg, &entry_id, constant_seed_ids) || !cfg.keys.is_empty()
            })
        {
            seed_ids.push(entry_id.clone());
        }

        if accepted {
            matched.push((
                entry.clone(),
                ActivationSortKey::from_activation(
                    activation.as_ref(),
                    &entry_id,
                    constant_seed_ids,
                ),
            ));
        } else {
            unmatched.push(entry.clone());
        }
    }

    PrimaryPassOutput {
        matched,
        unmatched,
        trace,
        seed_ids,
    }
}

/// Parse the `modules.activation` config of an entry (`None` when absent or
/// malformed — malformed configs classify as neutral).
fn parse_activation(entry: &KnowledgeEntryRecord) -> Option<ActivationConfig> {
    entry
        .modules
        .as_ref()
        .and_then(|modules| modules.get("activation"))
        .and_then(|value| serde_json::from_value::<ActivationConfig>(value.clone()).ok())
}

/// Spec §4 emit ordering comparator: constant band first, `priority`
/// descending (higher wins), `order` ascending (lower first).
fn compare_sort_keys(a: &ActivationSortKey, b: &ActivationSortKey) -> Ordering {
    b.constant
        .cmp(&a.constant)
        .then_with(|| b.priority.total_cmp(&a.priority))
        .then_with(|| a.order.total_cmp(&b.order))
}

/// Sort the matched pairs in place (spec §4). `sort_by` is stable, so equal
/// sort keys keep their original entry order — an all-neutral set sorts to
/// its original order and stays byte-identical to V1.146 flag-off.
fn sort_matched(matched: &mut [(KnowledgeEntryRecord, ActivationSortKey)]) {
    matched.sort_by(|(_, a), (_, b)| compare_sort_keys(a, b));
}

/// Evaluate one entry against the handbook logic truth table (spec §2.1).
///
/// Returns `(accepted, reason)`. The exhaustive table keeps this over the
/// pedantic line budget; splitting would obscure the truth-table shape.
#[allow(clippy::too_many_lines)] // exhaustive handbook logic truth table
fn evaluate_entry(
    cfg: &ActivationConfig,
    entry_id: &str,
    scan_raw: &str,
    scan_lower: &str,
    caller_seed_ids: &[String],
) -> (bool, String) {
    // Always-on seeds: handbook `constant: true`, V1.146 spike
    // `constant_seeds` self-id, or caller-supplied seed ids.
    if is_constant_seed(cfg, entry_id, caller_seed_ids) {
        let seed_source = if cfg.constant {
            "constant"
        } else if cfg.constant_seeds.iter().any(|s| s == entry_id) {
            "self-declared"
        } else {
            "caller-supplied"
        };
        return (true, format!("constant seed ({seed_source})"));
    }

    // Empty primary keys + no constant → neutral (spec §1 neutral-only:
    // "activation with empty keys and no constant").
    if cfg.keys.is_empty() {
        return (true, "no activation keys (neutral)".to_string());
    }

    let mode = &cfg.match_mode;
    let primary = eval_keys(&cfg.keys, mode, scan_raw, scan_lower);
    let primary_hit = !primary.0.is_empty();

    // Handbook §2.1: secondary absent/empty → primary-only, logic ignored.
    if cfg.secondary_keys.is_empty() {
        return if primary_hit {
            (
                true,
                format!(
                    "primary-any ({mode}): matched key [{}]",
                    primary.0.join(", ")
                ),
            )
        } else {
            (
                false,
                miss_reason("primary-any", mode, cfg.keys.len(), &primary.1),
            )
        };
    }

    let secondary = eval_keys(&cfg.secondary_keys, mode, scan_raw, scan_lower);
    let secondary_hit = !secondary.0.is_empty();

    match cfg.logic.as_str() {
        "and_any" => {
            if primary_hit && secondary_hit {
                (
                    true,
                    format!(
                        "and_any ({mode}): primary [{}] + secondary [{}] matched",
                        primary.0.join(", "),
                        secondary.0.join(", ")
                    ),
                )
            } else {
                (
                    false,
                    format!(
                        "and_any ({mode}): no full match — primary matched: {}, secondary matched: {}",
                        primary.0.len(),
                        secondary.0.len()
                    ),
                )
            }
        }
        "and_all" => {
            let primary_all = primary.0.len() == cfg.keys.len();
            let secondary_all = secondary.0.len() == cfg.secondary_keys.len();
            if primary_all && secondary_all {
                (
                    true,
                    format!(
                        "and_all ({mode}): all {} primary + {} secondary keys matched",
                        cfg.keys.len(),
                        cfg.secondary_keys.len()
                    ),
                )
            } else {
                let missing: Vec<&str> = missing_keys(&cfg.keys, &primary.0)
                    .into_iter()
                    .chain(missing_keys(&cfg.secondary_keys, &secondary.0))
                    .collect();
                (
                    false,
                    format!(
                        "and_all ({mode}): missing keys [{missing}]",
                        missing = missing.join(", ")
                    ),
                )
            }
        }
        "not_any" => {
            if primary_hit && !secondary_hit {
                (
                    true,
                    format!("not_any ({mode}): primary matched, no secondary matched"),
                )
            } else if !primary_hit {
                (
                    false,
                    format!(
                        "not_any ({mode}): no key matched ({} primary keys scanned)",
                        cfg.keys.len()
                    ),
                )
            } else {
                (
                    false,
                    format!(
                        "not_any ({mode}): secondary exclusion matched [{secondary}]",
                        secondary = secondary.0.join(", ")
                    ),
                )
            }
        }
        "not_all" => {
            let secondary_all = secondary.0.len() == cfg.secondary_keys.len();
            if primary_hit && !secondary_all {
                (
                    true,
                    format!("not_all ({mode}): primary matched, not all secondary matched"),
                )
            } else if !primary_hit {
                (
                    false,
                    format!(
                        "not_all ({mode}): no key matched ({} primary keys scanned)",
                        cfg.keys.len()
                    ),
                )
            } else {
                (
                    false,
                    format!("not_all ({mode}): all secondary matched — excluded"),
                )
            }
        }
        other => {
            // Unknown logic → handbook default `and_any`; recorded in trace (spec §2.1).
            if primary_hit && secondary_hit {
                (
                    true,
                    format!(
                        "unknown logic '{other}' — treated as and_any ({mode}): primary [{}] + secondary [{}] matched",
                        primary.0.join(", "),
                        secondary.0.join(", ")
                    ),
                )
            } else {
                (
                    false,
                    format!(
                        "unknown logic '{other}' — treated as and_any ({mode}): no key matched ({} primary + {} secondary keys scanned)",
                        cfg.keys.len(),
                        cfg.secondary_keys.len()
                    ),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_contracts::BlockType;
    use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryBody;
    use serde_json::json;

    /// Helper: build a `KnowledgeEntryRecord` with modules.activation.
    fn entry_with_activation(
        id: &str,
        name: &str,
        summary: &str,
        modules_val: Option<serde_json::Value>,
    ) -> KnowledgeEntryRecord {
        let mut entry = KnowledgeEntryRecord::new("wld_test", BlockType::Character, name);
        // Override the random entry_id with a predictable one for tests
        entry.entry_id = id.to_string();
        entry.body = Some(KnowledgeEntryBody {
            summary: Some(summary.to_string()),
            ..Default::default()
        });
        entry.modules = modules_val;
        entry
    }

    /// Helper: build activation JSON with handbook `keys` (+ optional secondary).
    fn activation_json(keys: &[&str], secondary_keys: &[&str], logic: &str) -> serde_json::Value {
        json!({
            "activation": {
                "keys": keys,
                "secondary_keys": secondary_keys,
                "logic": logic,
            }
        })
    }

    /// Shortcut for the common no-secondary case.
    fn activation_primary(keys: &[&str], logic: &str) -> serde_json::Value {
        activation_json(keys, &[], logic)
    }

    fn run(entries: &[KnowledgeEntryRecord], scan: &str) -> ActivationResult {
        apply_activation(entries, scan, &[])
    }

    // ── primary-only (secondary absent) — logic IGNORED ──────────────

    #[test]
    fn test_secondary_absent_and_any_fires_on_any_primary() {
        let entries = vec![entry_with_activation(
            "kb_1",
            "Old King",
            "The elderly ruler of the northern kingdom",
            Some(activation_primary(&["king", "dragon"], "and_any")),
        )];
        let result = run(
            &entries,
            "The story takes place in a northern kingdom ruled by an old king.",
        );
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].reason.contains("matched key [king]"));
    }

    #[test]
    fn test_secondary_absent_ignores_logic_and_all_any_hit_fires() {
        // CRITICAL truth-table check: with no secondary_keys, `logic: and_all`
        // must NOT require all primary keys — any primary hit fires.
        let entries = vec![entry_with_activation(
            "kb_2",
            "Royal Guard",
            "Protector of the throne",
            Some(activation_primary(&["king", "dragon"], "and_all")),
        )];
        let result = run(&entries, "The king sat on the throne.");
        assert_eq!(
            result.matched.len(),
            1,
            "and_all with no secondary = primary-any"
        );
        assert!(result.trace[0].reason.contains("primary-any (literal)"));
    }

    #[test]
    fn test_secondary_absent_ignores_logic_not_any_any_hit_fires() {
        // `not_any` with no secondary is NOT an exclusion list anymore —
        // primary-any fires (handbook: logic ignored).
        let entries = vec![entry_with_activation(
            "kb_3",
            "Orc Warlord",
            "A brutal commander",
            Some(activation_primary(&["orc", "warlord"], "not_any")),
        )];
        let result = run(&entries, "The orc army marched forward.");
        assert_eq!(
            result.matched.len(),
            1,
            "not_any with no secondary = primary-any"
        );
        assert!(result.trace[0].reason.contains("primary-any (literal)"));
    }

    #[test]
    fn test_secondary_absent_no_primary_hit_misses() {
        let entries = vec![entry_with_activation(
            "kb_4",
            "Dragon",
            "A fearsome dragon",
            Some(activation_primary(&["elf", "wizard"], "and_any")),
        )];
        let result = run(&entries, "The story takes place in a northern kingdom.");
        assert_eq!(result.matched.len(), 0);
        assert_eq!(result.unmatched.len(), 1);
        assert!(result.trace[0].reason.contains("no key matched"));
        assert!(result.trace[0].reason.contains("primary-any"));
    }

    // ── and_any + secondary ─────────────────────────────────────────

    #[test]
    fn test_and_any_with_secondary_requires_both() {
        let entries = vec![entry_with_activation(
            "kb_5",
            "Harbor",
            "The dawn dock district",
            Some(activation_json(&["harbor"], &["chapter 1"], "and_any")),
        )];
        let result = run(&entries, "The harbor gates open.");
        assert_eq!(
            result.matched.len(),
            0,
            "secondary 'chapter 1' missing → no fire"
        );
        assert!(result.trace[0].reason.contains("and_any"));
        assert!(result.trace[0].reason.contains("no full match"));

        let result2 = run(&entries, "The harbor gates open in chapter 1.");
        assert_eq!(
            result2.matched.len(),
            1,
            "primary + secondary both present → fire"
        );
        assert!(result2.trace[0]
            .reason
            .contains("primary [harbor] + secondary [chapter 1]"));
    }

    // ── and_all + secondary ──────────────────────────────────────────

    #[test]
    fn test_and_all_with_secondary_requires_all_primary_and_all_secondary() {
        // Secondary keys chosen to avoid the entry's own self-match text
        // ("Royal Guard" / "Protector of the throne").
        let entries = vec![entry_with_activation(
            "kb_6",
            "Royal Guard",
            "Protector of the throne",
            Some(activation_json(
                &["king", "throne"],
                &["horn", "bell"],
                "and_all",
            )),
        )];
        let result = run(&entries, "The king sat on the throne.");
        assert_eq!(result.matched.len(), 0, "secondary missing → no fire");
        assert!(result.trace[0].reason.contains("missing keys [horn, bell]"));

        let result2 = run(
            &entries,
            "The king sat on the throne while a horn and a bell rang.",
        );
        assert_eq!(
            result2.matched.len(),
            1,
            "all primary + all secondary → fire"
        );
        assert!(result2.trace[0]
            .reason
            .contains("all 2 primary + 2 secondary keys matched"));
    }

    #[test]
    fn test_and_all_with_secondary_missing_primary_fails() {
        let entries = vec![entry_with_activation(
            "kb_7",
            "Royal Guard",
            "Protector of the throne",
            Some(activation_json(&["king", "dragon"], &["guard"], "and_all")),
        )];
        let result = run(&entries, "The king and the guard stood watch.");
        assert_eq!(
            result.matched.len(),
            0,
            "primary 'dragon' missing → no fire"
        );
        assert!(result.trace[0].reason.contains("missing keys [dragon]"));
    }

    // ── not_any + secondary ──────────────────────────────────────────

    #[test]
    fn test_not_any_with_secondary_excludes_when_secondary_hits() {
        let entries = vec![entry_with_activation(
            "kb_8",
            "Orc Warlord",
            "A brutal commander",
            Some(activation_json(&["orc"], &["ally"], "not_any")),
        )];
        let result = run(&entries, "The orc army marched forward.");
        assert_eq!(result.matched.len(), 1, "primary hit, no secondary → fire");

        let result2 = run(&entries, "The orc and his ally marched forward.");
        assert_eq!(
            result2.matched.len(),
            0,
            "secondary 'ally' matches → excluded"
        );
        assert!(result2.trace[0]
            .reason
            .contains("secondary exclusion matched [ally]"));
    }

    #[test]
    fn test_not_any_with_secondary_no_primary_hit_fails() {
        let entries = vec![entry_with_activation(
            "kb_9",
            "Peaceful Elf",
            "A gentle forest dweller",
            Some(activation_json(&["orc"], &["ally"], "not_any")),
        )];
        let result = run(&entries, "The elves sang in the forest.");
        assert_eq!(result.matched.len(), 0, "primary missing → no fire");
        assert!(result.trace[0].reason.contains("no key matched"));
    }

    // ── not_all + secondary ──────────────────────────────────────────

    #[test]
    fn test_not_all_with_secondary_fires_when_not_every_secondary_matches() {
        let entries = vec![entry_with_activation(
            "kb_10",
            "Dark Wizard",
            "A powerful spellcaster",
            Some(activation_json(
                &["dark"],
                &["wizard", "necromancer"],
                "not_all",
            )),
        )];
        let result = run(&entries, "The dark wizard cast a spell.");
        assert_eq!(
            result.matched.len(),
            1,
            "'wizard' hits, 'necromancer' misses → fire"
        );

        let result2 = run(&entries, "The dark wizard necromancer cast a spell.");
        assert_eq!(
            result2.matched.len(),
            0,
            "every secondary matches → excluded"
        );
        assert!(result2.trace[0].reason.contains("all secondary matched"));
    }

    // ── match modes ──────────────────────────────────────────────────

    #[test]
    fn test_literal_is_case_insensitive_substring() {
        let entries = vec![entry_with_activation(
            "kb_11",
            "King Arthur",
            "Ruler of Camelot",
            Some(json!({"activation": {"keys": ["KING"]}})),
        )];
        let result = run(&entries, "The king ruled wisely.");
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].accepted);
    }

    #[test]
    fn test_whole_word_requires_boundary() {
        // Entry self-text must NOT contain the key, so matches come from the
        // scan text alone (self-match would defeat the boundary check).
        let entries = vec![entry_with_activation(
            "kb_12",
            "The Monarch",
            "Ruler of the realm",
            Some(json!({"activation": {"keys": ["king"], "match": "whole_word"}})),
        )];
        // "king" inside "kings" must NOT match; standalone "king" must.
        let result = run(&entries, "Two kings ruled.");
        assert_eq!(
            result.matched.len(),
            0,
            "'king' inside 'kings' is not a whole word"
        );

        let result2 = run(&entries, "A king ruled.");
        assert_eq!(result2.matched.len(), 1);
    }

    #[test]
    fn test_whole_word_cjk_char_boundary_safe_advance() {
        // Regression (QC F1): the advance after a failed boundary check used
        // `start + 1` bytes, slicing mid-char for multi-byte keys — this
        // panicked with "byte index 1 is not a char boundary" for
        // `whole_word_match("王", "王宫")`. "王" inside the CJK word "王宫" is
        // not a whole word → must not match, and must not panic.
        assert!(!whole_word_match("王", "王宫"));

        // A CJK key at a true boundary still matches; the scan must continue
        // past the failed boundary and find the later standalone occurrence.
        assert!(whole_word_match("王", "王宫。王"));

        // A CJK key inside a longer CJK word must not match mid-word.
        assert!(!whole_word_match("宫", "王宫"));
        assert!(!whole_word_match("王宫", "大秦王宫"));
    }

    #[test]
    fn test_regex_matches_pattern() {
        let entries = vec![entry_with_activation(
            "kb_13",
            "Draconic Beasts",
            "Fire breathers",
            Some(json!({"activation": {"keys": ["drag[ou]n"], "match": "regex"}})),
        )];
        let result = run(&entries, "A dragon and a dragoon met.");
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].accepted);
    }

    #[test]
    fn test_regex_invalid_pattern_is_non_match_with_trace() {
        let entries = vec![entry_with_activation(
            "kb_14",
            "Broken Pattern",
            "Unparseable regex",
            Some(json!({"activation": {"keys": ["(unclosed"], "match": "regex"}})),
        )];
        let result = run(&entries, "Any text.");
        assert_eq!(result.matched.len(), 0);
        assert!(result.trace[0].reason.contains("invalid regex"));
    }

    #[test]
    fn test_regex_overlong_key_skipped_with_trace() {
        let long_key = "x".repeat(MAX_REGEX_KEY_CHARS + 1);
        let entries = vec![entry_with_activation(
            "kb_15",
            "Overlong Key",
            "Key too long to compile",
            Some(json!({"activation": {"keys": [long_key], "match": "regex"}})),
        )];
        let result = run(&entries, "Any text.");
        assert_eq!(result.matched.len(), 0);
        assert!(result.trace[0]
            .reason
            .contains("key over 256 chars skipped"));
    }

    #[test]
    fn test_regex_scan_text_truncated_to_64k() {
        // A key beyond the 64 KiB cap must still match when present near the
        // head (stable prefix) — and must not panic on huge scan text.
        let scan = format!("harbor {}", "y".repeat(70 * 1024));
        let entries = vec![entry_with_activation(
            "kb_16",
            "Harbor",
            "Dawn dock",
            Some(json!({"activation": {"keys": ["harbor"], "match": "regex"}})),
        )];
        let result = run(&entries, &scan);
        assert_eq!(result.matched.len(), 1, "key at stable prefix head matches");
    }

    #[test]
    fn test_regex_catastrophic_pattern_completes_in_bounded_time() {
        // F-001 ReDoS regression (qc2 F-001, Critical): `(a+)+b` against a
        // non-matching run of 'a's drives a backtracking engine to
        // exponential worst-case time — qc2 measured >1.5s on 28 chars. The
        // `regex` crate is linear-time by construction, so this must return
        // promptly; the test reaching its assertions at all is the regression
        // guard (a backtracking engine would hang on the 5k 'a' scan below).
        let scan = format!("{}!", "a".repeat(5000));
        let entries = vec![entry_with_activation(
            "kb_redos",
            "ReDoS Guard",
            "Must not match the pattern",
            Some(json!({"activation": {"keys": ["(a+)+b"], "match": "regex"}})),
        )];
        let result = run(&entries, &scan);
        assert_eq!(
            result.matched.len(),
            0,
            "catastrophic pattern must not match a non-matching scan"
        );
        assert!(result.trace[0].reason.contains("no key matched"));
    }

    #[test]
    fn test_unknown_match_mode_falls_back_to_literal() {
        let entries = vec![entry_with_activation(
            "kb_17",
            "Fallback",
            "Unknown mode",
            Some(json!({"activation": {"keys": ["KING"], "match": "fuzzy"}})),
        )];
        let result = run(&entries, "The king ruled.");
        assert_eq!(
            result.matched.len(),
            1,
            "unknown mode → literal (CI substring)"
        );
    }

    // ── constant seeds ───────────────────────────────────────────────

    #[test]
    fn test_constant_true_is_always_matched() {
        let entries = vec![entry_with_activation(
            "kb_c1",
            "World Rule",
            "Always-on seed",
            Some(json!({"activation": {"keys": [], "constant": true, "priority": 100}})),
        )];
        let result = run(&entries, "Nothing matches.");
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].reason.contains("constant seed (constant)"));
    }

    #[test]
    fn test_caller_supplied_seed_always_matches() {
        let entries = vec![entry_with_activation(
            "kb_cs_1",
            "Seed Entry",
            "This should always be included",
            Some(activation_primary(&["nonexistent"], "and_all")),
        )];
        let result = apply_activation(
            &entries,
            "This story has nothing matching.",
            &["kb_cs_1".to_string()],
        );
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0]
            .reason
            .contains("constant seed (caller-supplied)"));
    }

    #[test]
    fn test_self_declared_constant_seeds_spike_alias() {
        // V1.146 spike alias: constant_seeds self-id → constant.
        let mut activation = activation_primary(&["nonexistent"], "and_all");
        activation["activation"]["constant_seeds"] = json!(["kb_self_1"]);
        let entries = vec![entry_with_activation(
            "kb_self_1",
            "Self Seed",
            "Always included",
            Some(activation),
        )];
        let result = run(&entries, "Nothing matches.");
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0]
            .reason
            .contains("constant seed (self-declared)"));
    }

    // ── ordering (V1.149 P0 T3 — spec §4 / AC-I1 #7) ────────────────

    fn matched_ids(result: &ActivationResult) -> Vec<&str> {
        result.matched.iter().map(|e| e.entry_id.as_str()).collect()
    }

    #[test]
    fn test_ordering_priority_desc_higher_first() {
        let entries = vec![
            entry_with_activation(
                "kb_o1",
                "Low",
                "p",
                Some(json!({"activation": {"keys": ["king"], "priority": 1}})),
            ),
            entry_with_activation(
                "kb_o2",
                "High",
                "p",
                Some(json!({"activation": {"keys": ["king"], "priority": 5}})),
            ),
            entry_with_activation(
                "kb_o3",
                "Mid",
                "p",
                Some(json!({"activation": {"keys": ["king"], "priority": 3}})),
            ),
        ];
        let result = run(&entries, "The king ruled.");
        assert_eq!(
            matched_ids(&result),
            ["kb_o2", "kb_o3", "kb_o1"],
            "priority desc: 5 > 3 > 1"
        );
    }

    #[test]
    fn test_ordering_order_asc_within_same_priority() {
        let entries = vec![
            entry_with_activation(
                "kb_o11",
                "A",
                "p",
                Some(json!({"activation": {"keys": ["king"], "priority": 2, "order": 10}})),
            ),
            entry_with_activation(
                "kb_o12",
                "B",
                "p",
                Some(json!({"activation": {"keys": ["king"], "priority": 2, "order": 1}})),
            ),
            entry_with_activation(
                "kb_o13",
                "C",
                "p",
                Some(json!({"activation": {"keys": ["king"], "priority": 2, "order": 5}})),
            ),
        ];
        let result = run(&entries, "The king ruled.");
        assert_eq!(
            matched_ids(&result),
            ["kb_o12", "kb_o13", "kb_o11"],
            "order asc within equal priority: 1 < 5 < 10"
        );
    }

    #[test]
    fn test_ordering_constant_band_first_even_at_lower_priority() {
        // Constant seed (priority 0) must precede a non-constant priority 100.
        let entries = vec![
            entry_with_activation(
                "kb_o21",
                "Hot",
                "p",
                Some(json!({"activation": {"keys": ["king"], "priority": 100}})),
            ),
            entry_with_activation(
                "kb_o22",
                "Seed",
                "p",
                Some(json!({"activation": {"keys": [], "constant": true}})),
            ),
        ];
        let result = run(&entries, "The king ruled.");
        assert_eq!(
            matched_ids(&result),
            ["kb_o22", "kb_o21"],
            "constant band sorts before non-constant regardless of priority"
        );
    }

    #[test]
    fn test_ordering_constant_band_internal_priority() {
        // Within the constant band, priority still applies (spec §4).
        let entries = vec![
            entry_with_activation(
                "kb_o31",
                "Seed A",
                "p",
                Some(json!({"activation": {"constant": true, "priority": 1}})),
            ),
            entry_with_activation(
                "kb_o32",
                "Seed B",
                "p",
                Some(json!({"activation": {"constant": true, "priority": 9}})),
            ),
        ];
        let result = run(&entries, "Nothing matches.");
        assert_eq!(matched_ids(&result), ["kb_o32", "kb_o31"]);
    }

    #[test]
    fn test_ordering_stable_tiebreak_keeps_original_order() {
        // Equal priority + order + band → original entry order (stable sort).
        let entries = vec![
            entry_with_activation(
                "kb_o41",
                "First",
                "p",
                Some(json!({"activation": {"keys": ["king"], "priority": 3, "order": 2}})),
            ),
            entry_with_activation(
                "kb_o42",
                "Second",
                "p",
                Some(json!({"activation": {"keys": ["king"], "priority": 3, "order": 2}})),
            ),
            entry_with_activation(
                "kb_o43",
                "Third",
                "p",
                Some(json!({"activation": {"keys": ["king"], "priority": 3, "order": 2}})),
            ),
        ];
        let result = run(&entries, "The king ruled.");
        assert_eq!(
            matched_ids(&result),
            ["kb_o41", "kb_o42", "kb_o43"],
            "equal priority+order keeps original entry order"
        );
    }

    #[test]
    fn test_ordering_missing_priority_order_default_to_zero() {
        // Missing priority/order ⇒ 0 (spec §4): a default-0 entry sits between
        // explicit positive and negative priorities.
        let entries = vec![
            entry_with_activation(
                "kb_o51",
                "Default",
                "p",
                Some(json!({"activation": {"keys": ["king"]}})),
            ),
            entry_with_activation(
                "kb_o52",
                "Negative",
                "p",
                Some(json!({"activation": {"keys": ["king"], "priority": -5}})),
            ),
            entry_with_activation(
                "kb_o53",
                "Positive",
                "p",
                Some(json!({"activation": {"keys": ["king"], "priority": 5}})),
            ),
        ];
        let result = run(&entries, "The king ruled.");
        assert_eq!(
            matched_ids(&result),
            ["kb_o53", "kb_o51", "kb_o52"],
            "missing priority ⇒ 0, sorted desc: 5 > 0 > -5"
        );
    }

    #[test]
    fn test_ordering_neutral_only_keeps_original_order() {
        // All-neutral set: every entry carries default sort keys → stable sort
        // returns the original order — the engine-level byte-equivalence
        // guarantee (spec §1 / §4).
        let entries = vec![
            entry_with_activation("kb_nA", "Neutral A", "No modules", None),
            entry_with_activation(
                "kb_nB",
                "Neutral B",
                "Modules, no activation",
                Some(json!({"pack": {"version": 1}})),
            ),
            entry_with_activation(
                "kb_nC",
                "Neutral C",
                "Empty keys",
                Some(activation_primary(&[], "and_any")),
            ),
        ];
        let result = run(&entries, "Any text.");
        assert_eq!(
            matched_ids(&result),
            ["kb_nA", "kb_nB", "kb_nC"],
            "all-neutral matched set keeps original entry order"
        );
        assert!(result.unmatched.is_empty());
    }

    #[test]
    fn test_ordering_neutral_mixed_with_priority_entries() {
        // Neutral entries act as default-key members of the non-constant band.
        let entries = vec![
            entry_with_activation("kb_mx1", "Neutral", "No activation", None),
            entry_with_activation(
                "kb_mx2",
                "Matched High",
                "p",
                Some(json!({"activation": {"keys": ["king"], "priority": 10}})),
            ),
            entry_with_activation(
                "kb_mx3",
                "Matched Low",
                "p",
                Some(json!({"activation": {"keys": ["king"], "priority": -10}})),
            ),
        ];
        let result = run(&entries, "The king ruled.");
        assert_eq!(
            matched_ids(&result),
            ["kb_mx2", "kb_mx1", "kb_mx3"],
            "priority 10 > neutral default 0 > priority -10"
        );
    }

    #[test]
    fn test_trace_reason_includes_match_mode_and_logic_arm() {
        // T3 verify: hit + miss reasons carry match mode + logic arm for the
        // future DF-76 inspector (spec §1 #5).
        let hit_entries = vec![entry_with_activation(
            "kb_tr1",
            "Hit",
            "p",
            Some(json!({"activation": {
                "keys": ["king"],
                "secondary_keys": ["throne"],
                "logic": "and_any",
                "match": "whole_word"
            }})),
        )];
        let result = run(&hit_entries, "The king sits on a throne.");
        assert!(result.trace[0].accepted);
        assert!(
            result.trace[0].reason.contains("and_any (whole_word)"),
            "hit reason must carry arm + mode: {}",
            result.trace[0].reason
        );

        let miss_entries = vec![entry_with_activation(
            "kb_tr2",
            "Miss",
            "p",
            Some(json!({"activation": {
                "keys": ["king", "queen"],
                "secondary_keys": ["throne"],
                "logic": "and_all",
                "match": "whole_word"
            }})),
        )];
        let result = run(&miss_entries, "The queen sits on a throne.");
        assert!(!result.trace[0].accepted);
        assert!(
            result.trace[0].reason.contains("and_all (whole_word)"),
            "miss reason must carry arm + mode: {}",
            result.trace[0].reason
        );
    }

    // ── spike aliases: `key` ─────────────────────────────────────────

    #[test]
    fn test_spike_key_alias_when_keys_absent() {
        let entries = vec![entry_with_activation(
            "kb_k1",
            "Aliased",
            "Spike key field",
            Some(json!({"activation": {"key": ["king"], "logic": "and_any"}})),
        )];
        let result = run(&entries, "The king ruled.");
        assert_eq!(result.matched.len(), 1, "spike `key` accepted as `keys`");
    }

    #[test]
    fn test_spike_keys_preferred_when_both_present() {
        // Both `keys` and `key`: `keys` wins (spec §2.2).
        let entries = vec![entry_with_activation(
            "kb_k2",
            "Preferred",
            "Keys wins over key",
            Some(json!({"activation": {"keys": ["wizard"], "key": ["king"], "logic": "and_any"}})),
        )];
        let result = run(&entries, "The king ruled.");
        assert_eq!(
            result.matched.len(),
            0,
            "`keys` wins — 'king' from spike `key` must not fire"
        );
        let result2 = run(&entries, "A wizard appeared.");
        assert_eq!(result2.matched.len(), 1);
    }

    // ── neutral entries (byte-equivalence guarantee) ─────────────────

    #[test]
    fn test_neutral_entry_no_modules() {
        let entries = vec![entry_with_activation(
            "kb_n1",
            "Neutral",
            "No modules",
            None,
        )];
        let result = run(&entries, "Any text.");
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].reason.contains("no activation module"));
    }

    #[test]
    fn test_neutral_entry_modules_without_activation() {
        let entries = vec![entry_with_activation(
            "kb_n2",
            "Other Module",
            "Has modules but not activation",
            Some(json!({"pack": {"version": 1}})),
        )];
        let result = run(&entries, "Any text.");
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].reason.contains("no activation module"));
    }

    #[test]
    fn test_empty_activation_keys_neutral() {
        let entries = vec![entry_with_activation(
            "kb_ek",
            "Empty Keys",
            "No keys defined",
            Some(activation_primary(&[], "and_any")),
        )];
        let result = run(&entries, "Any text.");
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].reason.contains("no activation keys"));
    }

    #[test]
    fn test_null_modules() {
        let entries = vec![entry_with_activation(
            "kb_null",
            "Null Modules",
            "No modules",
            Some(json!(null)),
        )];
        let result = run(&entries, "Text.");
        // modules is Some(null) → get("activation") on null returns None → neutral.
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].reason.contains("no activation module"));
    }

    #[test]
    fn test_unknown_logic_treated_as_and_any_with_secondary() {
        // Unknown logic + secondary present → and_any fallback (spec §2.1).
        // Secondary key "chapter" must not appear in the entry self-text.
        let entries = vec![entry_with_activation(
            "kb_u1",
            "Matched",
            "Contains king",
            Some(activation_json(&["king"], &["chapter"], "fuzzy")),
        )];
        let result = run(&entries, "The king ruled in chapter 5.");
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].reason.contains("treated as and_any"));

        let result2 = run(&entries, "The king ruled alone.");
        assert_eq!(result2.matched.len(), 0);
        assert!(result2.trace[0].reason.contains("treated as and_any"));
    }

    // ── mixed classification ─────────────────────────────────────────

    #[test]
    fn test_mixed_matched_unmatched_and_neutral() {
        let entries = vec![
            entry_with_activation(
                "kb_m1",
                "Match",
                "Contains king",
                Some(activation_primary(&["king"], "and_any")),
            ),
            entry_with_activation(
                "kb_m2",
                "NoMatch",
                "Nothing",
                Some(activation_primary(&["dragon"], "and_any")),
            ),
            entry_with_activation("kb_m3", "Neutral", "No activation", None),
        ];
        let result = run(&entries, "The king ruled.");
        assert_eq!(result.matched.len(), 2); // kb_m1 (matched) + kb_m3 (neutral)
        assert_eq!(result.unmatched.len(), 1); // kb_m2
    }

    // ── unknown-field round-trip + module preservation ───────────────

    #[test]
    fn test_activation_unknown_fields_ignored() {
        // Consumer-only dialect: nexus-private unknown fields inside
        // `activation` are ignored, not rejected.
        let entries = vec![entry_with_activation(
            "kb_uf",
            "Unknown Fields",
            "Portable subset with extras",
            Some(json!({
                "activation": {
                    "keys": ["king"],
                    "nexus_private_flag": "must-not-break-parse",
                    "outlet": "lore-main",
                }
            })),
        )];
        let result = run(&entries, "The king ruled.");
        assert_eq!(result.matched.len(), 1);
    }

    #[test]
    fn test_modules_activation_survives_spoke_roundtrip() {
        use crate::conversion::{knowledge_record_to_spoke, spoke_to_knowledge_record};

        let mut entry = KnowledgeEntryRecord::new("wld_test", BlockType::Character, "Hero");
        entry.entry_id = "kb_rt1".to_string();
        entry.body = Some(KnowledgeEntryBody {
            summary: Some("The hero of the story".to_string()),
            ..Default::default()
        });
        entry.modules = Some(json!({
            "activation": {
                "keys": ["hero", "protagonist"],
                "secondary_keys": ["chapter 1"],
                "logic": "and_any",
                "constant": true,
                "order": 10,
                "priority": 5,
                "position_hint": "before_defs",
                "match": "whole_word"
            },
            "other_module": {"version": 1}
        }));

        // Forward → spoke, reverse → nexus.
        let spoke = knowledge_record_to_spoke(&entry);
        let roundtripped = spoke_to_knowledge_record(spoke).unwrap();

        let modules = roundtripped
            .modules
            .as_ref()
            .expect("modules survive the spoke round-trip");
        let activation = &modules["activation"];
        assert_eq!(activation["keys"], json!(["hero", "protagonist"]));
        assert_eq!(activation["secondary_keys"], json!(["chapter 1"]));
        assert_eq!(activation["logic"], "and_any");
        assert_eq!(activation["constant"], true);
        assert_eq!(activation["order"], 10);
        assert_eq!(activation["priority"], 5);
        assert_eq!(activation["position_hint"], "before_defs");
        assert_eq!(activation["match"], "whole_word");
        // Unknown module namespace preserved.
        assert_eq!(modules["other_module"]["version"], 1);
    }

    // ── V1.149 P1: relation-hop expansion (pure engine, fixture edges) ──

    /// Build a fixture `HopEdge` (deterministic id derived from endpoints).
    fn edge(from_id: &str, to_id: &str, relation_type: &str) -> HopEdge {
        HopEdge {
            relation_id: format!("rel_{from_id}_{to_id}"),
            from_id: from_id.to_string(),
            to_id: to_id.to_string(),
            relation_type: relation_type.to_string(),
        }
    }

    fn hop_ids(result: &HopExpandResult) -> Vec<&str> {
        result.pulled.iter().map(|e| e.entry_id.as_str()).collect()
    }

    #[test]
    fn test_expand_relation_hops_pure_api_pulls_chain_with_trace() {
        // Direct exercise of the pure BFS API (plan T1 signature).
        let mut pool = HashMap::new();
        for id in ["kb_a", "kb_b", "kb_c"] {
            let entry = entry_with_activation(id, id, "summary", None);
            pool.insert(entry.entry_id.clone(), entry);
        }
        let seeds = vec!["kb_a".to_string()];
        let edges = vec![
            edge("kb_a", "kb_b", "located_in"),
            edge("kb_b", "kb_c", "member_of"),
        ];
        let config = HopConfig::default(); // max_hops 2, no budget

        let result = expand_relation_hops(&seeds, &[], &pool, &edges, &config);
        assert_eq!(
            hop_ids(&result),
            ["kb_b", "kb_c"],
            "BFS pulls level 1 then level 2"
        );
        assert_eq!(result.hop_trace.len(), 2);
        // Level-1 row: origin = seed, depth 1, first edge wins.
        let row1 = &result.hop_trace[0];
        assert_eq!(row1.entry_id, "kb_b");
        assert_eq!(row1.hop_origin_entry_id.as_deref(), Some("kb_a"));
        assert_eq!(row1.hop_depth, Some(1));
        assert_eq!(row1.source_relation_type.as_deref(), Some("located_in"));
        assert_eq!(row1.source_relation_id.as_deref(), Some("rel_kb_a_kb_b"));
        assert!(row1.accepted);
        // Level-2 row: origin = the level-1 entry that reached it.
        let row2 = &result.hop_trace[1];
        assert_eq!(row2.entry_id, "kb_c");
        assert_eq!(row2.hop_origin_entry_id.as_deref(), Some("kb_b"));
        assert_eq!(row2.hop_depth, Some(2));
        assert_eq!(row2.source_relation_type.as_deref(), Some("member_of"));
        assert_eq!(row2.source_relation_id.as_deref(), Some("rel_kb_b_kb_c"));
    }

    #[test]
    fn test_expand_relation_hops_pre_visited_blocks_pull_and_traversal() {
        // QC C-001 engine level: pre-visited ids (already in `matched`) are
        // never pulled, and the BFS does not traverse through them.
        let mut pool = HashMap::new();
        for id in ["kb_a", "kb_b", "kb_c"] {
            let entry = entry_with_activation(id, id, "summary", None);
            pool.insert(entry.entry_id.clone(), entry);
        }
        let seeds = vec!["kb_a".to_string()];
        let pre_visited = vec!["kb_b".to_string()];
        let edges = vec![
            edge("kb_a", "kb_b", "located_in"),
            edge("kb_b", "kb_c", "member_of"),
        ];
        let config = HopConfig::default();

        let result = expand_relation_hops(&seeds, &pre_visited, &pool, &edges, &config);
        assert!(
            hop_ids(&result).is_empty(),
            "pre-visited neighbor not pulled; chain not traversed through it"
        );
        assert!(result.hop_trace.is_empty());
    }

    #[test]
    fn test_apply_activation_with_hops_one_hop_pulls_neighbor() {
        let entries = vec![
            entry_with_activation(
                "kb_a",
                "Harbor",
                "The harbor gates.",
                Some(activation_primary(&["king"], "and_any")),
            ),
            entry_with_activation(
                "kb_b",
                "Dawn Dock",
                "A quiet dock.",
                Some(activation_primary(&["dragon"], "and_any")),
            ),
        ];
        let edges = vec![edge("kb_a", "kb_b", "located_in")];
        let result = apply_activation_with_hops(
            &entries,
            "The king ruled.",
            &[],
            &edges,
            &HopConfig::default(),
        );

        assert_eq!(
            matched_ids(&result),
            ["kb_a", "kb_b"],
            "neighbor pulled into matched"
        );
        // B never fires on the primary pass…
        let b_primary = result
            .trace
            .iter()
            .find(|t| t.entry_id == "kb_b" && !t.accepted);
        assert!(b_primary.is_some(), "B has a primary-pass miss row");
        // …and is accepted only via the hop row.
        let b_hop = result
            .trace
            .iter()
            .find(|t| t.entry_id == "kb_b" && t.accepted)
            .expect("B has a hop row");
        assert_eq!(b_hop.hop_origin_entry_id.as_deref(), Some("kb_a"));
        assert_eq!(b_hop.hop_depth, Some(1));
        assert!(b_hop.reason.contains("relation hop (depth 1): located_in"));
    }

    #[test]
    fn test_apply_activation_with_hops_two_hops_pulls_chain() {
        let entries = vec![
            entry_with_activation(
                "kb_a",
                "Harbor",
                "The harbor gates.",
                Some(activation_primary(&["king"], "and_any")),
            ),
            entry_with_activation(
                "kb_b",
                "Dawn Dock",
                "A quiet dock.",
                Some(activation_primary(&["dragon"], "and_any")),
            ),
            entry_with_activation(
                "kb_c",
                "Harbor Guild",
                "The guild hall.",
                Some(activation_primary(&["elf"], "and_any")),
            ),
        ];
        let edges = vec![
            edge("kb_a", "kb_b", "located_in"),
            edge("kb_b", "kb_c", "member_of"),
        ];
        let result = apply_activation_with_hops(
            &entries,
            "The king ruled.",
            &[],
            &edges,
            &HopConfig::default(),
        );

        assert_eq!(
            matched_ids(&result),
            ["kb_a", "kb_b", "kb_c"],
            "2-hop chain fully pulled"
        );
        let c_hop = result
            .trace
            .iter()
            .find(|t| t.entry_id == "kb_c" && t.accepted)
            .expect("C has a hop row");
        assert_eq!(c_hop.hop_depth, Some(2));
        assert_eq!(c_hop.hop_origin_entry_id.as_deref(), Some("kb_b"));
        assert_eq!(c_hop.source_relation_type.as_deref(), Some("member_of"));
    }

    #[test]
    fn test_apply_activation_with_hops_a_b_cycle_terminates() {
        // A↔B: the reverse edge must not duplicate B or loop forever.
        let entries = vec![
            entry_with_activation(
                "kb_a",
                "Harbor",
                "The harbor gates.",
                Some(activation_primary(&["king"], "and_any")),
            ),
            entry_with_activation(
                "kb_b",
                "Dawn Dock",
                "A quiet dock.",
                Some(activation_primary(&["dragon"], "and_any")),
            ),
        ];
        let edges = vec![
            edge("kb_a", "kb_b", "located_in"),
            edge("kb_b", "kb_a", "contains"),
        ];
        let result = apply_activation_with_hops(
            &entries,
            "The king ruled.",
            &[],
            &edges,
            &HopConfig::default(),
        );

        assert_eq!(
            matched_ids(&result),
            ["kb_a", "kb_b"],
            "A↔B pulls B once, no dup, terminates"
        );
        let b_rows: Vec<_> = result
            .trace
            .iter()
            .filter(|t| t.entry_id == "kb_b")
            .collect();
        assert_eq!(b_rows.len(), 2, "B: one primary miss + one hop row");
        assert_eq!(
            b_rows[1].source_relation_type.as_deref(),
            Some("located_in"),
            "first edge in slice order wins the trace"
        );
    }

    #[test]
    fn test_apply_activation_with_hops_a_to_b_to_a_terminates() {
        // A→B→A: A is already visited (seed), so the hop back is a no-op.
        let entries = vec![
            entry_with_activation(
                "kb_a",
                "Harbor",
                "The harbor gates.",
                Some(activation_primary(&["king"], "and_any")),
            ),
            entry_with_activation(
                "kb_b",
                "Dawn Dock",
                "A quiet dock.",
                Some(activation_primary(&["dragon"], "and_any")),
            ),
        ];
        let edges = vec![
            edge("kb_a", "kb_b", "located_in"),
            edge("kb_b", "kb_a", "located_in"),
        ];
        let result = apply_activation_with_hops(
            &entries,
            "The king ruled.",
            &[],
            &edges,
            &HopConfig::default(),
        );

        assert_eq!(
            matched_ids(&result),
            ["kb_a", "kb_b"],
            "A→B→A terminates with no re-pull"
        );
        assert_eq!(
            result.trace.iter().filter(|t| t.entry_id == "kb_a").count(),
            1,
            "A has only its primary row (never hop-pulled)"
        );
    }

    #[test]
    fn test_apply_activation_with_hops_max_hops_cap_blocks_depth_3() {
        let entries = vec![
            entry_with_activation(
                "kb_a",
                "A",
                "The harbor gates.",
                Some(activation_primary(&["king"], "and_any")),
            ),
            entry_with_activation(
                "kb_b",
                "B",
                "B.",
                Some(activation_primary(&["b1"], "and_any")),
            ),
            entry_with_activation(
                "kb_c",
                "C",
                "C.",
                Some(activation_primary(&["c1"], "and_any")),
            ),
            entry_with_activation(
                "kb_d",
                "D",
                "D.",
                Some(activation_primary(&["d1"], "and_any")),
            ),
        ];
        let edges = vec![
            edge("kb_a", "kb_b", "t1"),
            edge("kb_b", "kb_c", "t2"),
            edge("kb_c", "kb_d", "t3"),
        ];
        let result = apply_activation_with_hops(
            &entries,
            "The king ruled.",
            &[],
            &edges,
            &HopConfig::default(),
        );

        assert_eq!(
            matched_ids(&result),
            ["kb_a", "kb_b", "kb_c"],
            "max_hops 2 pulls depths 1..=2 only — D (depth 3) stays unmatched"
        );
        assert!(result.unmatched.iter().any(|e| e.entry_id == "kb_d"));
    }

    #[test]
    fn test_apply_activation_with_hops_budget_stops_mid_expansion() {
        // max_hop_tokens = 10; primary KB (A, 9 chars → 2 tokens) reserves 2 →
        // remaining 8. B (60 chars → 15 tokens) exceeds → skipped; C
        // (8 chars → 2 tokens) fits → pulled. Per-entry gate, not level-stop:
        // the over-budget edge comes FIRST and C is still pulled.
        let entries = vec![
            entry_with_activation(
                "kb_a",
                "A",
                "The king.",
                Some(activation_primary(&["king"], "and_any")),
            ),
            entry_with_activation(
                "kb_b",
                "B",
                &"x".repeat(60),
                Some(activation_primary(&["dragon"], "and_any")),
            ),
            entry_with_activation(
                "kb_c",
                "C",
                &"y".repeat(8),
                Some(activation_primary(&["elf"], "and_any")),
            ),
        ];
        let edges = vec![edge("kb_a", "kb_b", "big"), edge("kb_a", "kb_c", "small")];
        let config = HopConfig {
            max_hops: 2,
            max_hop_tokens: Some(10),
        };
        let result = apply_activation_with_hops(&entries, "The king ruled.", &[], &edges, &config);

        assert_eq!(
            matched_ids(&result),
            ["kb_a", "kb_c"],
            "over-budget B skipped, in-budget C pulled"
        );
        assert!(result.unmatched.iter().any(|e| e.entry_id == "kb_b"));
        assert!(result
            .trace
            .iter()
            .all(|t| t.entry_id != "kb_b" || !t.accepted));
    }

    #[test]
    fn test_apply_activation_with_hops_budget_exhaustion_blocks_deeper_levels() {
        // max_hop_tokens = 10; primary A (9 chars → 2) → remaining 8.
        // Level 1: B (8 chars → 2) pulled → 6; C (24 chars → 6) pulled → 0.
        // Level 2: D/E (8 chars → 2 each) exceed the exhausted budget → skipped.
        let entries = vec![
            entry_with_activation(
                "kb_a",
                "A",
                "The king.",
                Some(activation_primary(&["king"], "and_any")),
            ),
            entry_with_activation(
                "kb_b",
                "B",
                &"b".repeat(8),
                Some(activation_primary(&["dragon"], "and_any")),
            ),
            entry_with_activation(
                "kb_c",
                "C",
                &"c".repeat(24),
                Some(activation_primary(&["elf"], "and_any")),
            ),
            entry_with_activation(
                "kb_d",
                "D",
                &"d".repeat(8),
                Some(activation_primary(&["d1"], "and_any")),
            ),
            entry_with_activation(
                "kb_e",
                "E",
                &"e".repeat(8),
                Some(activation_primary(&["e1"], "and_any")),
            ),
        ];
        let edges = vec![
            edge("kb_a", "kb_b", "t1"),
            edge("kb_a", "kb_c", "t1"),
            edge("kb_b", "kb_d", "t2"),
            edge("kb_c", "kb_e", "t2"),
        ];
        let config = HopConfig {
            max_hops: 2,
            max_hop_tokens: Some(10),
        };
        let result = apply_activation_with_hops(&entries, "The king ruled.", &[], &edges, &config);

        assert_eq!(matched_ids(&result), ["kb_a", "kb_b", "kb_c"]);
        assert!(result.unmatched.iter().any(|e| e.entry_id == "kb_d"));
        assert!(result.unmatched.iter().any(|e| e.entry_id == "kb_e"));
    }

    #[test]
    fn test_apply_activation_with_hops_no_keyword_refire_on_pulled() {
        // B's keys would NOT match the scan; B enters matched ONLY via the
        // graph hop. The accepted trace row for B is the hop row (graph
        // reason), not a key-match reason — proving pulled entries are never
        // re-evaluated against modules.activation (spec Q5).
        let entries = vec![
            entry_with_activation(
                "kb_a",
                "Harbor",
                "The harbor gates.",
                Some(activation_primary(&["king"], "and_any")),
            ),
            entry_with_activation(
                "kb_b",
                "Dawn Dock",
                "A quiet dock.",
                Some(activation_primary(&["dragon"], "and_any")),
            ),
        ];
        let edges = vec![edge("kb_a", "kb_b", "located_in")];
        let result = apply_activation_with_hops(
            &entries,
            "The king ruled.",
            &[],
            &edges,
            &HopConfig::default(),
        );

        let b_rows: Vec<_> = result
            .trace
            .iter()
            .filter(|t| t.entry_id == "kb_b")
            .collect();
        assert_eq!(b_rows.len(), 2, "B: primary miss + hop pull");
        let b_hop = b_rows
            .iter()
            .find(|t| t.accepted)
            .expect("B hop row accepted");
        assert!(
            b_hop.reason.starts_with("relation hop"),
            "accepted row reason must be the graph reason, got: {}",
            b_hop.reason
        );
        assert_eq!(matched_ids(&result), ["kb_a", "kb_b"]);
        assert_eq!(
            result
                .matched
                .iter()
                .filter(|e| e.entry_id == "kb_b")
                .count(),
            1,
            "B present in matched exactly once"
        );
    }

    #[test]
    fn test_apply_activation_with_hops_constant_seed_expands() {
        // A constant seed (empty keys) is a hop seed: B pulled from it.
        let entries = vec![
            entry_with_activation(
                "kb_a",
                "World Rule",
                "Always-on seed.",
                Some(json!({"activation": {"keys": [], "constant": true}})),
            ),
            entry_with_activation(
                "kb_b",
                "Dawn Dock",
                "A quiet dock.",
                Some(activation_primary(&["dragon"], "and_any")),
            ),
        ];
        let edges = vec![edge("kb_a", "kb_b", "located_in")];
        let result = apply_activation_with_hops(
            &entries,
            "Nothing matches.",
            &[],
            &edges,
            &HopConfig::default(),
        );

        assert_eq!(
            matched_ids(&result),
            ["kb_a", "kb_b"],
            "constant seed + hop pull"
        );
        let b_hop = result
            .trace
            .iter()
            .find(|t| t.entry_id == "kb_b" && t.accepted)
            .expect("B hop row");
        assert_eq!(b_hop.hop_origin_entry_id.as_deref(), Some("kb_a"));
    }

    #[test]
    fn test_apply_activation_with_hops_pulled_sorts_in_non_constant_band() {
        // Spec §4: hop-expanded entries sort in the non-constant band by the
        // same keys (not automatically demoted). B (pulled, priority 100)
        // outranks A (primary hit, priority 5).
        let entries = vec![
            entry_with_activation(
                "kb_a",
                "Harbor",
                "The harbor gates.",
                Some(json!({"activation": {"keys": ["king"], "priority": 5}})),
            ),
            entry_with_activation(
                "kb_b",
                "Dawn Dock",
                "A quiet dock.",
                Some(json!({"activation": {"keys": ["dragon"], "priority": 100}})),
            ),
        ];
        let edges = vec![edge("kb_a", "kb_b", "located_in")];
        let result = apply_activation_with_hops(
            &entries,
            "The king ruled.",
            &[],
            &edges,
            &HopConfig::default(),
        );

        assert_eq!(
            matched_ids(&result),
            ["kb_b", "kb_a"],
            "priority desc across pulled + primary"
        );
    }

    #[test]
    fn test_apply_activation_with_hops_self_loop_safe() {
        let entries = vec![entry_with_activation(
            "kb_a",
            "Harbor",
            "The harbor gates.",
            Some(activation_primary(&["king"], "and_any")),
        )];
        let edges = vec![edge("kb_a", "kb_a", "self_loop")];
        let result = apply_activation_with_hops(
            &entries,
            "The king ruled.",
            &[],
            &edges,
            &HopConfig::default(),
        );

        assert_eq!(matched_ids(&result), ["kb_a"]);
        assert_eq!(result.trace.len(), 1, "self-loop produces no extra rows");
    }

    #[test]
    fn test_apply_activation_with_hops_endpoint_outside_pool_skipped() {
        // Edge A→X where X is not a candidate entry: skipped silently.
        let entries = vec![
            entry_with_activation(
                "kb_a",
                "Harbor",
                "The harbor gates.",
                Some(activation_primary(&["king"], "and_any")),
            ),
            entry_with_activation(
                "kb_b",
                "Dawn Dock",
                "A quiet dock.",
                Some(activation_primary(&["dragon"], "and_any")),
            ),
        ];
        let edges = vec![
            edge("kb_a", "kb_x", "located_in"),
            edge("kb_a", "kb_b", "located_in"),
        ];
        let result = apply_activation_with_hops(
            &entries,
            "The king ruled.",
            &[],
            &edges,
            &HopConfig::default(),
        );

        assert_eq!(
            matched_ids(&result),
            ["kb_a", "kb_b"],
            "pool-missing endpoint skipped, B pulled"
        );
        assert!(result.trace.iter().all(|t| t.entry_id != "kb_x"));
    }

    #[test]
    fn test_apply_activation_with_hops_first_edge_wins_trace() {
        // Two parallel edges A→B: the first in slice order supplies the trace.
        let entries = vec![
            entry_with_activation(
                "kb_a",
                "Harbor",
                "The harbor gates.",
                Some(activation_primary(&["king"], "and_any")),
            ),
            entry_with_activation(
                "kb_b",
                "Dawn Dock",
                "A quiet dock.",
                Some(activation_primary(&["dragon"], "and_any")),
            ),
        ];
        let edges = vec![
            HopEdge {
                relation_id: "rel_first".to_string(),
                from_id: "kb_a".to_string(),
                to_id: "kb_b".to_string(),
                relation_type: "first_edge".to_string(),
            },
            HopEdge {
                relation_id: "rel_second".to_string(),
                from_id: "kb_a".to_string(),
                to_id: "kb_b".to_string(),
                relation_type: "second_edge".to_string(),
            },
        ];
        let result = apply_activation_with_hops(
            &entries,
            "The king ruled.",
            &[],
            &edges,
            &HopConfig::default(),
        );

        assert_eq!(
            matched_ids(&result),
            ["kb_a", "kb_b"],
            "B pulled once despite two edges"
        );
        let b_rows: Vec<_> = result
            .trace
            .iter()
            .filter(|t| t.entry_id == "kb_b")
            .collect();
        assert_eq!(b_rows.len(), 2, "B: primary miss + one hop row");
        let b_hop = b_rows.iter().find(|t| t.accepted).expect("B hop row");
        assert_eq!(b_hop.source_relation_id.as_deref(), Some("rel_first"));
        assert_eq!(b_hop.source_relation_type.as_deref(), Some("first_edge"));
    }

    #[test]
    fn test_apply_activation_with_hops_no_edges_equals_apply_activation() {
        // P0-parity: empty edges ⇒ hop pass skipped ⇒ byte-identical result.
        let entries = vec![
            entry_with_activation(
                "kb_a",
                "Harbor",
                "The harbor gates.",
                Some(activation_primary(&["king"], "and_any")),
            ),
            entry_with_activation(
                "kb_b",
                "Dawn Dock",
                "A quiet dock.",
                Some(activation_primary(&["dragon"], "and_any")),
            ),
            entry_with_activation("kb_c", "Forest", "A forest.", None),
        ];
        let base = apply_activation(&entries, "The king ruled.", &[]);
        let with_hops = apply_activation_with_hops(
            &entries,
            "The king ruled.",
            &[],
            &[],
            &HopConfig::default(),
        );

        assert_eq!(matched_ids(&base), matched_ids(&with_hops));
        assert_eq!(
            serde_json::to_value(&base.trace).unwrap(),
            serde_json::to_value(&with_hops.trace).unwrap(),
            "no edges ⇒ identical trace (no hop rows)"
        );
        assert_eq!(base.unmatched.len(), with_hops.unmatched.len());
    }

    #[test]
    fn test_apply_activation_with_hops_neutral_only_no_seeds_no_pulls() {
        // Engine-level golden: a neutral-only world has no seeds ⇒ no hop
        // pull ⇒ identical to the P0 activation result (byte-equivalence
        // guarantee holds under hops; spec §1).
        let entries = vec![
            entry_with_activation("kb_n1", "Neutral A", "No modules.", None),
            entry_with_activation("kb_n2", "Neutral B", "No modules.", None),
        ];
        let edges = vec![edge("kb_n1", "kb_n2", "located_in")];
        let base = apply_activation(&entries, "Any text.", &[]);
        let with_hops =
            apply_activation_with_hops(&entries, "Any text.", &[], &edges, &HopConfig::default());

        assert_eq!(matched_ids(&base), matched_ids(&with_hops));
        assert_eq!(
            with_hops.trace.len(),
            2,
            "no hop rows for neutral-only world"
        );
        assert!(with_hops.trace.iter().all(|t| t.hop_depth.is_none()));
    }

    #[test]
    fn test_apply_activation_with_hops_neutral_matched_not_pulled_again() {
        // QC C-001 regression: a seed fires; a neutral entry (no
        // `modules.activation`) is graph-adjacent to it AND already in
        // `matched` from the primary pass (neutral ⇒ always included). Hop
        // expansion must not pull it a second time — every entry_id appears
        // exactly once in `matched`.
        let entries = vec![
            entry_with_activation(
                "kb_a",
                "Harbor",
                "The harbor gates.",
                Some(activation_primary(&["king"], "and_any")),
            ),
            entry_with_activation("kb_n", "Dawn Dock", "A quiet dock.", None),
        ];
        let edges = vec![edge("kb_a", "kb_n", "located_in")];
        let result = apply_activation_with_hops(
            &entries,
            "The king ruled.",
            &[],
            &edges,
            &HopConfig::default(),
        );

        assert_eq!(
            matched_ids(&result),
            ["kb_a", "kb_n"],
            "neutral already-matched neighbor is not re-pulled"
        );
        let mut seen: HashSet<&str> = HashSet::new();
        for entry in &result.matched {
            assert!(
                seen.insert(entry.entry_id.as_str()),
                "entry '{}' appears more than once in matched",
                entry.entry_id
            );
        }
        assert_eq!(result.trace.len(), 2, "no hop rows for the neutral entry");
        assert!(result.trace.iter().all(|t| t.hop_depth.is_none()));
        // qc2 F-001 partition: matched ∩ unmatched = ∅.
        for entry in &result.matched {
            assert!(
                !result
                    .unmatched
                    .iter()
                    .any(|u| u.entry_id == entry.entry_id),
                "entry '{}' in both matched and unmatched",
                entry.entry_id
            );
        }
    }

    #[test]
    fn test_apply_activation_with_hops_pulled_entry_removed_from_unmatched() {
        // qc2 F-001: a primary-missed entry pulled by the hop pass must not
        // also remain in `unmatched` — matched ∩ unmatched = ∅.
        let entries = vec![
            entry_with_activation(
                "kb_a",
                "Harbor",
                "The harbor gates.",
                Some(activation_primary(&["king"], "and_any")),
            ),
            entry_with_activation(
                "kb_b",
                "Dawn Dock",
                "A quiet dock.",
                Some(activation_primary(&["dragon"], "and_any")),
            ),
            entry_with_activation(
                "kb_c",
                "Harbor Guild",
                "The guild hall.",
                Some(activation_primary(&["elf"], "and_any")),
            ),
        ];
        // kb_b and kb_c both miss the scan; only kb_b is pulled (kb_c is not
        // adjacent) → kb_c legitimately stays unmatched.
        let edges = vec![edge("kb_a", "kb_b", "located_in")];
        let result = apply_activation_with_hops(
            &entries,
            "The king ruled.",
            &[],
            &edges,
            &HopConfig::default(),
        );

        assert_eq!(matched_ids(&result), ["kb_a", "kb_b"]);
        assert!(
            !result.unmatched.iter().any(|e| e.entry_id == "kb_b"),
            "hop-pulled entry removed from unmatched"
        );
        assert!(
            result.unmatched.iter().any(|e| e.entry_id == "kb_c"),
            "non-adjacent primary miss stays unmatched"
        );
        for entry in &result.matched {
            assert!(
                !result
                    .unmatched
                    .iter()
                    .any(|u| u.entry_id == entry.entry_id),
                "entry '{}' in both matched and unmatched",
                entry.entry_id
            );
        }
    }
}
