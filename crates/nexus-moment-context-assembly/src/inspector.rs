//! V1.151 P0 — enriched assembly inspector packet builder (DF-76, spec
//! `fl-l-w6-assembly-inspector.md` §2).
//!
//! Relocated from `apps/nexus42/src/commands/platform/context.rs` (V1.150
//! `build_inspector_packet`) into MCA so the daemon route (T2) and the CLI
//! `assemble-moment --inspect` / `--emit-packet` (T4) share one builder.
//!
//! Packet shape (spec §2): `modules` (unchanged spoke assemble-module
//! recipe — `placement` + `activation_trace`, AC-I3) plus three additive
//! **product-local** top-level sections:
//!
//! - `slot_map` — `entry_id` → slot id (`world.before` | `default` |
//!   `world.after` | `kb.outlet.<name>` | `style.post_history` |
//!   `moment.directive`), captured post stage-gate at assembly time.
//! - `budget` — chars/4 token accounting: `primary_tokens_est`,
//!   `hop_tokens_est`, `cap`, `remaining`.
//! - `moment_directive` — **status/metadata only** (`scope`, `scope_id`,
//!   `insert_depth`, `ttl_kind`, `ttl_remaining`, `clear_on_scene_change`,
//!   `status`). The directive **body never appears**: the builder reads
//!   only `ctx.moment_directive_meta`, never `ctx.moment_directive` — body
//!   exclusion is **by construction** (AC-I3).
//!
//! The packet is a **separate emission path** from `to_full_context()` — it
//! never changes assembled bytes (AC-I6).

use crate::moment::MomentContext;

/// Build the enriched inspector packet JSON from an assembled
/// [`MomentContext`].
///
/// `modules.*` keeps the spoke assemble-module recipe (unchanged, AC-I3);
/// `slot_map` / `budget` / `moment_directive` are additive product-local
/// sections (spec §2). All sections are always present with nullable/empty
/// values so consumers can rely on a fixed shape.
// The Some/None arms are two full `json!` object literals (the None arm
// mirrors the Some shape with nulls); a `match` reads clearer than a
// `map_or_else` closure pair here.
#[allow(clippy::option_if_let_else)]
#[must_use]
pub fn build_inspector_packet(ctx: &MomentContext) -> serde_json::Value {
    let trace = ctx.activation_trace.as_deref().unwrap_or(&[]);

    // modules.placement: entries that passed activation (accepted == true).
    let placement: Vec<serde_json::Value> = trace
        .iter()
        .filter(|t| t.accepted)
        .map(|t| {
            serde_json::json!({
                "entry_id": t.entry_id,
                "canonical_name": t.canonical_name,
                "reason": t.reason,
            })
        })
        .collect();

    // modules.activation_trace: full per-entry fire/miss trace.
    let trace_json: Vec<serde_json::Value> = trace
        .iter()
        .map(|t| {
            serde_json::json!({
                "entry_id": t.entry_id,
                "canonical_name": t.canonical_name,
                "reason": t.reason,
                "accepted": t.accepted,
            })
        })
        .collect();

    // slot_map: entry_id → slot id (spec §2 H2) — captured post stage-gate
    // at assembly time, so it reflects what actually rendered.
    let slot_map: Vec<serde_json::Value> = ctx
        .slot_map
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|e| {
            serde_json::json!({
                "entry_id": e.entry_id,
                "slot": e.slot,
            })
        })
        .collect();

    // budget: chars/4 token accounting (spec §2 H3). `None` budget (no
    // activation ran) renders zeros with null cap/remaining.
    let budget = serde_json::json!({
        "primary_tokens_est": ctx.activation_budget.as_ref().map_or(0, |b| b.primary_tokens_est),
        "hop_tokens_est": ctx.activation_budget.as_ref().map_or(0, |b| b.hop_tokens_est),
        "cap": ctx.activation_budget.as_ref().and_then(|b| b.cap),
        "remaining": ctx.activation_budget.as_ref().and_then(|b| b.remaining),
    });

    // moment_directive: status/metadata only (spec §2 H6). Reads ONLY
    // `ctx.moment_directive_meta` — the directive body is excluded **by
    // construction** (AC-I3). `"none"` + nulls when no directive injected.
    let moment_directive = match ctx.moment_directive_meta.as_ref() {
        Some(meta) => serde_json::json!({
            "scope": meta.scope.kind,
            "scope_id": meta.scope.id,
            "insert_depth": meta.insert_depth.as_str(),
            "ttl_kind": meta.ttl_kind.as_str(),
            "ttl_remaining": meta.ttl_remaining,
            "clear_on_scene_change": meta.clear_on_scene_change,
            "status": meta.status,
        }),
        None => serde_json::json!({
            "scope": serde_json::Value::Null,
            "scope_id": serde_json::Value::Null,
            "insert_depth": serde_json::Value::Null,
            "ttl_kind": serde_json::Value::Null,
            "ttl_remaining": serde_json::Value::Null,
            "clear_on_scene_change": false,
            "status": "none",
        }),
    };

    serde_json::json!({
        "modules": {
            "placement": placement,
            "activation_trace": trace_json,
        },
        "slot_map": slot_map,
        "budget": budget,
        "moment_directive": moment_directive,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::directive::{DirectiveDepth, DirectiveTtlKind, MomentDirectiveScope};
    use nexus_spoke_adapter::adapter::activation::{ActivationBudget, ActivationTraceEntry};

    fn trace_entry(entry_id: &str, accepted: bool) -> ActivationTraceEntry {
        ActivationTraceEntry {
            entry_id: entry_id.to_string(),
            canonical_name: entry_id.to_string(),
            reason: "keyword match".to_string(),
            accepted,
            hop_origin_entry_id: None,
            hop_depth: None,
            source_relation_type: None,
            source_relation_id: None,
        }
    }

    /// V1.151 P0 (spec §2 + AC-I6): the enriched packet over a seeded
    /// `MomentContext` (slots + hops budget + active directive) carries the
    /// three product-local sections with the exact captured values, and
    /// `modules.*` is unchanged from the trace-derived shape.
    #[test]
    fn enriched_packet_carries_slot_map_budget_and_directive_meta() {
        let trace = vec![
            trace_entry("kb_hero", true),
            trace_entry("kb_castle", true),
            trace_entry("kb_dragon", false),
        ];
        let ctx = MomentContext {
            stage0_context: "stage0".to_string(),
            activation_trace: Some(trace),
            slot_map: Some(vec![
                crate::slots::SlotMapEntry {
                    entry_id: "kb_hero".to_string(),
                    slot: "world.before".to_string(),
                },
                crate::slots::SlotMapEntry {
                    entry_id: "kb_castle".to_string(),
                    slot: "default".to_string(),
                },
                crate::slots::SlotMapEntry {
                    entry_id: "dir_1".to_string(),
                    slot: "moment.directive".to_string(),
                },
            ]),
            activation_budget: Some(ActivationBudget {
                primary_tokens_est: 12,
                hop_tokens_est: 3,
                cap: Some(100),
                remaining: Some(85),
            }),
            moment_directive_meta: Some(crate::directive::MomentDirectiveStatus {
                scope: MomentDirectiveScope {
                    kind: "work".to_string(),
                    id: "wrk_1".to_string(),
                },
                insert_depth: DirectiveDepth::Tail,
                ttl_kind: DirectiveTtlKind::Generations,
                ttl_remaining: Some(2),
                clear_on_scene_change: true,
                status: "active".to_string(),
            }),
            ..MomentContext::default()
        };

        let packet = build_inspector_packet(&ctx);

        // slot_map section: entry_id → slot, exact captured values.
        assert_eq!(
            packet["slot_map"],
            serde_json::json!([
                { "entry_id": "kb_hero", "slot": "world.before" },
                { "entry_id": "kb_castle", "slot": "default" },
                { "entry_id": "dir_1", "slot": "moment.directive" },
            ]),
            "slot_map must carry the captured post-gate slot assignments"
        );

        // budget section: chars/4 accounting.
        assert_eq!(
            packet["budget"],
            serde_json::json!({
                "primary_tokens_est": 12,
                "hop_tokens_est": 3,
                "cap": 100,
                "remaining": 85,
            }),
            "budget must carry the captured token accounting"
        );

        // moment_directive section: status/metadata only.
        assert_eq!(
            packet["moment_directive"],
            serde_json::json!({
                "scope": "work",
                "scope_id": "wrk_1",
                "insert_depth": "tail",
                "ttl_kind": "generations",
                "ttl_remaining": 2,
                "clear_on_scene_change": true,
                "status": "active",
            }),
            "moment_directive must carry status/metadata only"
        );

        // modules.* unchanged: derived solely from the activation trace.
        assert_eq!(
            packet["modules"]["placement"],
            serde_json::json!([
                { "entry_id": "kb_hero", "canonical_name": "kb_hero", "reason": "keyword match" },
                { "entry_id": "kb_castle", "canonical_name": "kb_castle", "reason": "keyword match" },
            ]),
            "modules.placement unchanged — accepted entries only"
        );
        assert_eq!(
            packet["modules"]["activation_trace"],
            serde_json::json!([
                { "entry_id": "kb_hero", "canonical_name": "kb_hero", "reason": "keyword match", "accepted": true },
                { "entry_id": "kb_castle", "canonical_name": "kb_castle", "reason": "keyword match", "accepted": true },
                { "entry_id": "kb_dragon", "canonical_name": "kb_dragon", "reason": "keyword match", "accepted": false },
            ]),
            "modules.activation_trace unchanged — full fire/miss trace"
        );
    }

    /// V1.151 P0 (spec §2): a neutral context (no slots, no budget, no
    /// directive) still yields the fixed four-section shape — empty arrays
    /// and nulls — so consumers can rely on a stable packet.
    #[test]
    fn neutral_context_yields_fixed_packet_shape() {
        let ctx = MomentContext::default();
        let packet = build_inspector_packet(&ctx);

        assert_eq!(packet["slot_map"], serde_json::json!([]));
        assert_eq!(
            packet["budget"],
            serde_json::json!({
                "primary_tokens_est": 0,
                "hop_tokens_est": 0,
                "cap": serde_json::Value::Null,
                "remaining": serde_json::Value::Null,
            })
        );
        assert_eq!(
            packet["moment_directive"],
            serde_json::json!({
                "scope": serde_json::Value::Null,
                "scope_id": serde_json::Value::Null,
                "insert_depth": serde_json::Value::Null,
                "ttl_kind": serde_json::Value::Null,
                "ttl_remaining": serde_json::Value::Null,
                "clear_on_scene_change": false,
                "status": "none",
            })
        );
        assert_eq!(
            packet["modules"],
            serde_json::json!({ "placement": [], "activation_trace": [] })
        );
    }
}
