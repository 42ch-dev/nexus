//! Criterion benchmark: CapabilityRegistry dispatch latency (V1.54 P0 T6).
//!
//! Measures:
//! - `registry_lookup_cold` — fresh registry construction + 19 lookups
//! - `registry_lookup_warm` — 19 lookups on already-initialized `&REGISTRY`
//! - `registry_len` — `len()` call on initialized `&REGISTRY`
//!
//! Target: warm-path lookup ≤1µs; cold-path init + 19 lookups <500µs.
//!
//! Note: `dispatch_whoami` end-to-end dispatch is not benchmarked here
//! because it requires a live `WorkspaceState` (database + filesystem).
//! End-to-end dispatch latency is covered by the concurrent dispatch tests
//! in `host_tool_executor.rs`.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use nexus_daemon_runtime::capability_registry::host_tool_registry;

// `build_registry` is `pub(crate)`, accessible from benchmarks in the same crate.
// Note: it is NOT re-exported through the crate's public API, so the import
// must reference the internal module path — but since benches/ is part of the
// same crate, bench code can call `nexus_daemon_runtime::capability_registry::build_registry`.
use nexus_daemon_runtime::capability_registry::build_registry;

fn bench_registry_lookup_cold(c: &mut Criterion) {
    // Cold path: construct a fresh registry and measure init + lookups.
    // This measures what happens on the first `host_tool_registry()` call.
    c.bench_function("registry_lookup_cold_init_plus_19_lookups", |b| {
        b.iter(|| {
            let reg = build_registry();
            for tool_id in &[
                "nexus.context.whoami",
                "nexus.workspace.info",
                "nexus.work.get",
                "nexus.work.patch",
                "nexus.orchestration.schedule_status",
                "nexus.context.assemble",
                "nexus.world.snapshot.get",
                "nexus.timeline.recent.get",
                "nexus.kb_snapshot.read",
                "nexus.manuscript.chapter.get",
                "nexus.observability.daemon.health",
                "nexus.kb_snapshot.write",
                "nexus.manuscript.chapter.update",
                "nexus.world.configure",
                "nexus.work.schedule.set",
                "nexus.finding.resolve",
                "nexus.pool.entry.manage",
                "fs/read_text_file",
                "fs/write_text_file",
            ] {
                let _row = black_box(reg.lookup(tool_id));
            }
        })
    });
}

fn bench_registry_lookup_warm(c: &mut Criterion) {
    // Warm path: registry already initialized by first call.
    let _reg = host_tool_registry(); // force LazyLock init

    c.bench_function("registry_lookup_warm_19_tools", |b| {
        b.iter(|| {
            let reg = host_tool_registry();
            for tool_id in &[
                "nexus.context.whoami",
                "nexus.workspace.info",
                "nexus.work.get",
                "nexus.work.patch",
                "nexus.orchestration.schedule_status",
                "nexus.context.assemble",
                "nexus.world.snapshot.get",
                "nexus.timeline.recent.get",
                "nexus.kb_snapshot.read",
                "nexus.manuscript.chapter.get",
                "nexus.observability.daemon.health",
                "nexus.kb_snapshot.write",
                "nexus.manuscript.chapter.update",
                "nexus.world.configure",
                "nexus.work.schedule.set",
                "nexus.finding.resolve",
                "nexus.pool.entry.manage",
                "fs/read_text_file",
                "fs/write_text_file",
            ] {
                let _row = black_box(reg.lookup(tool_id));
            }
        })
    });
}

fn bench_registry_len(c: &mut Criterion) {
    c.bench_function("registry_len", |b| {
        b.iter(|| {
            let reg = host_tool_registry();
            black_box(reg.len())
        })
    });
}

criterion_group!(
    benches,
    bench_registry_lookup_cold,
    bench_registry_lookup_warm,
    bench_registry_len
);
criterion_main!(benches);
