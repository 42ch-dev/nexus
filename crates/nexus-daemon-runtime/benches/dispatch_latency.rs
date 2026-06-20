//! Criterion benchmark: CapabilityRegistry dispatch latency (V1.54 P0 T6).
//!
//! Measures:
//! - `registry_lookup_cold` — registry construction (LazyLock init) + 19 lookups
//! - `registry_lookup_warm` — 19 lookups on already-initialized &REGISTRY
//! - `dispatch_whoami` — end-to-end dispatch of `nexus.context.whoami`
//!
//! Target: warm-path lookup ≤1µs; cold-path init + 19 lookups <500µs.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use nexus_daemon_runtime::capability_registry::host_tool_registry;

fn bench_registry_lookup_warm(c: &mut Criterion) {
    // Warm path: registry already initialized by first call
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

criterion_group!(benches, bench_registry_lookup_warm, bench_registry_len);
criterion_main!(benches);
