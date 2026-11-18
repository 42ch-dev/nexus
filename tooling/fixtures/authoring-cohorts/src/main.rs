//! Checker-owned REAL consumer for the authoring cohort (v1.249 A3).
//!
//! Not a product and not a core test target: one path dependency on
//! `nexus-core`, selecting the core path a consumer selects. `default =
//! ["execution"]` is the execution-only consumer; `--features authoring` adds
//! `nexus-core/strategy-authoring`. Every check is an assertion over real
//! behaviour, so an exit status of 0 means the selection behaved — never that a
//! call merely returned:
//!
//! - **reserved**: the canonical `nexus.strategy.generate` row is in the
//!   catalog in BOTH selections (the catalog compiles in every cohort);
//! - **runnable**: the registry binds the row, and the allowlist
//!   (`spine_resolves`) agrees with it, only where the authoring surface links;
//! - **reservation**: neither entry path asks the peer/user fallback lanes about
//!   the canonical id (a zero delta on the registry's lane-consultation
//!   counter, with a live-counter control), so the refusal below is an early
//!   canonical miss rather than a fall-through that found nothing;
//! - **dispatch**: without authoring the dispatch refuses with the
//!   canonical-miss `not_supported`; with authoring it reaches the real Generate
//!   kernel and commits candidate revision 1 with its `generate` provenance;
//! - **domain**: `nexus.context.whoami` — a domain row — answers in both
//!   selections, so the difference under test is the authoring surface alone.
//!
//! `tooling/check-graph-pins.sh --cohort authoring` states the other half: the
//! resolved core feature set each selection produces.

use std::path::Path;
use std::sync::Arc;

use nexus_contracts::ToolsApi;
use nexus_core::owner::capabilities::{
    execute_tool, host_tool_registry, ToolContext, ToolRuntimeFacts,
};
use nexus_core::tool_catalog::{lookup_host_tool, HostToolId};
use nexus_core::{CoreAccess, CoreOpenOptions, CoreService};
use serde_json::{json, Value};

const CREATOR: &str = "ctr_cohort";
const SLUG: &str = "default";
const STRATEGY: &str = "cohort-authoring";
const GENERATE_TOOL: &str = "nexus.strategy.generate";
const DOMAIN_TOOL: &str = "nexus.context.whoami";

/// The user bundle the strategy id addresses: the layout the product writes and
/// the core resolves a strategy through.
const PRESET_YAML: &str = "preset:
  id: cohort-authoring
  version: 1
  kind: creator
  description: \"authoring cohort fixture\"
  requires_capabilities: []
  run_intents: [work_init]
  initial: start
  terminal: done
states:
  - id: start
    enter: []
    exit_when: { kind: rule }
    next: done
  - id: done
    terminal: true
";

fn request(tool_name: &str, parameters: Value) -> ToolsApi {
    ToolsApi {
        caller_kind: None,
        parameters,
        request_id: None,
        session_id: None,
        tool_name: tool_name.to_string(),
    }
}

/// The same legal graph `crates/nexus-core/tests/generate_loop.rs` commits:
/// one outer state wired to its terminal, one run intent.
fn legal_request() -> Value {
    json!({
        "entry": "start",
        "terminal": ["done"],
        "nodes": [
            {"id": "start", "kind": "outer_state", "effect_class": "pure"},
            {"id": "done", "kind": "terminal", "effect_class": "pure"},
        ],
        "edges": [{"id": "start->done", "from": "start", "to": "done", "kind": "linear"}],
        "nests": [],
        "meta": {
            "description": "authoring cohort fixture",
            "kind": "creator",
            "run_intents": ["work_init"],
            "requires_capabilities": [],
        },
    })
}

/// The disposable home the core can open: the active-creator config and the
/// operational workspace directory `CoreService::open` expects.
fn prepare_home(home: &Path) {
    let nexus_home = home.join(".nexus42");
    std::fs::create_dir_all(&nexus_home).expect("mkdir nexus home");
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        home, CREATOR, SLUG,
    ))
    .expect("mkdir operational workspace");
    let bundle = nexus_home.join("presets").join(STRATEGY);
    std::fs::create_dir_all(&bundle).expect("mkdir preset bundle");
    std::fs::write(bundle.join("preset.yaml"), PRESET_YAML).expect("write preset.yaml");
    std::fs::write(
        nexus_home.join("config.toml"),
        format!(
            "active_creator_id = \"{CREATOR}\"\n\
             [active_workspace_slug_by_creator]\n\
             \"{CREATOR}\" = \"{SLUG}\"\n"
        ),
    )
    .expect("write config.toml");
}

#[tokio::main]
async fn main() {
    let home = std::env::temp_dir().join(format!("nexus-authoring-cohort-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    prepare_home(&home);
    // The engine-owned core and its store: the tool spine writes its audit row
    // through a guarded connection, and the workspace gate admits exactly the
    // engine/migration writer, so this consumer opens the same product access
    // level the daemon's owner does instead of a direct writer that would be
    // fenced at the first audit insert.
    let db_path = nexus_home_layout::workspace_state_db_path(&home, CREATOR, SLUG);
    let core = CoreService::open(CoreOpenOptions {
        user_home: home.clone(),
        access: CoreAccess::EngineOwner,
        memory: None,
    })
    .await
    .expect("open the core domain");
    let store = nexus_local_db::writer_protocol::join_live_engine_pool(
        &db_path,
        nexus_local_db::writer_protocol::GuardedPoolOptions::default(),
    )
    .await
    .expect("join the engine admission")
    .expect("the open core holds a live engine admission");
    let pool = store.clone_pool();
    let context = ToolContext::new(
        pool.clone(),
        home.join(".nexus42"),
        Some(
            nexus_home_layout::operational_workspace_dir(&home, CREATOR, SLUG)
                .display()
                .to_string(),
        ),
        ToolRuntimeFacts {
            is_initialized: true,
            lifecycle_state: "Running".to_string(),
            ..ToolRuntimeFacts::default()
        },
        Some(Arc::new(core)),
        None,
    );

    // Reserved: the catalog compiles in every cohort, so the row exists whether
    // or not this build binds a handler for it. The absent-id arm keeps the
    // reservation check from passing on "a lookup returned something".
    let catalog_row = lookup_host_tool(GENERATE_TOOL).expect("the canonical row stays reserved");
    assert_eq!(catalog_row.id, HostToolId::StrategyGenerate);
    assert!(
        lookup_host_tool("nexus.not.a.canonical.tool").is_none(),
        "an id outside the catalog must never read as reserved"
    );

    // Runnable: the binding is the feature's, and the caller-facing allowlist
    // must agree with it — a reserved id may never read as reachable while
    // dispatch refuses it.
    let authoring = cfg!(feature = "authoring");
    let registry = host_tool_registry();
    let bound = registry.lookup(GENERATE_TOOL).is_some();
    assert_eq!(
        bound, authoring,
        "the Generate row is bound iff strategy-authoring is linked"
    );
    assert_eq!(
        registry.ids().any(|id| id == GENERATE_TOOL),
        bound,
        "the runnable id list must match the binding"
    );

    // RESERVATION, not just the refusal. `not_supported` alone cannot tell an
    // early canonical-miss refusal from a fall-through that asked the peer and
    // user lanes and found nothing there — both return the same code. The
    // registry's lane-consultation counter distinguishes them, so both entry
    // paths (allowlist and dispatch) are pinned to a ZERO delta for the
    // canonical id.
    //
    // The counter is compiled in by core's `test-hooks`, selected by this
    // checker's own `observable` feature (tooling-only, `publish = false`,
    // never a shipped selection). No other test-hooks seam is armed.
    let consulted = || host_tool_registry().fallback_consultations();
    let lanes_before = consulted();
    assert_eq!(
        registry.spine_resolves(&context, GENERATE_TOOL),
        bound,
        "the allowlist decision must match the binding"
    );

    // Dispatch: the reserved-but-unbound row refuses through the canonical-miss
    // decision; the bound row reaches the real Generate kernel.
    let generate = execute_tool(
        &context,
        &request(
            GENERATE_TOOL,
            json!({
                "strategy_id": STRATEGY,
                "base_revision": 0,
                "request": legal_request(),
            }),
        ),
    )
    .await;

    assert_eq!(
        consulted(),
        lanes_before,
        "a canonical id must never reach the peer/user fallback lanes (allowlist + dispatch)"
    );

    // CONTROL: a non-canonical, unbound id DOES consult the lanes, so a counter
    // that never moved would prove nothing. Without this arm the zero delta
    // above would also pass on a build where the counter is dead.
    assert!(
        !registry.spine_resolves(&context, "tools.not-a-real-peer-tool"),
        "an unregistered peer-grammar id must not resolve"
    );
    assert_eq!(
        consulted(),
        lanes_before + 1,
        "the lane counter must observe exactly the one real consultation"
    );

    #[cfg(feature = "authoring")]
    let dispatch = {
        let payload =
            generate.expect("the authoring build binds the row, so dispatch must reach the kernel");
        assert_eq!(
            payload["new_revision"],
            json!(1),
            "a first candidate commits at revision 1: {payload}"
        );
        assert_eq!(payload["document"]["id"], json!(STRATEGY), "{payload}");
        assert_eq!(payload["document"]["revision"], json!(1), "{payload}");
        assert_eq!(
            payload["document"]["state"],
            json!("candidate"),
            "{payload}"
        );
        assert_eq!(
            payload["document"]["provenance"]["source"],
            json!("generate"),
            "only the Generate kernel writes this provenance: {payload}"
        );
        assert!(payload["warnings"].is_array(), "{payload}");
        "committed@1".to_string()
    };

    #[cfg(not(feature = "authoring"))]
    let dispatch = {
        let error =
            generate.expect_err("the reservation must refuse a row this build does not bind");
        assert!(
            matches!(&error, nexus_core::CoreError::Coded { code, .. } if code == "not_supported"),
            "expected the canonical-miss refusal, got {error:?}"
        );
        "not_supported".to_string()
    };

    // Domain: a non-authoring row answers in BOTH selections, so the only
    // difference this runner observes is the authoring surface itself.
    let whoami = execute_tool(&context, &request(DOMAIN_TOOL, json!({})))
        .await
        .expect("the domain row must stay runnable in every selection");
    assert_eq!(whoami["creator_id"], json!(CREATOR), "{whoami}");
    assert_eq!(whoami["workspace_slug"], json!(SLUG), "{whoami}");

    println!(
        "authoring-cohort OK selection={} reserved=1 runnable={} dispatch={} domain=whoami@{CREATOR}",
        if authoring { "authoring" } else { "execution-only" },
        u8::from(bound),
        dispatch,
    );

    drop(context);
    pool.close().await;
    drop(store);
    let _ = std::fs::remove_dir_all(&home);
}
