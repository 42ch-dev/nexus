//! V1.58 P0 T14 (R-V156P1-L005) — registry.refresh latency benchmark.
//!
//! Measures the `RegistryRefresh` capability invocation latency:
//! - `registry_refresh_cold` — construct a fresh capability + run (measures
//!   allocation + first-call overhead; the synthetic path exercises the
//!   embedded snapshot and the metrics/tracing fast-paths).
//! - `registry_refresh_warm` — reuse an already-constructed capability across
//!   iterations (measures steady-state `run()` cost).
//!
//! Target: warm-path `run()` (synthetic) should be sub-millisecond. The
//! CDN/network path is intentionally NOT benchmarked here because it depends
//! on external I/O; a sustained-stream hammer test for the body-size cap is
//! covered in the unit tests.
//!
//! Run with: `cargo bench -p nexus-orchestration --bench registry_refresh_latency`

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use nexus_orchestration::capability::{builtins::RegistryRefresh, Capability};

fn bench_registry_refresh_cold(c: &mut Criterion) {
    // Cold path: construct + run. Measures allocation + first-call overhead.
    c.bench_function("registry_refresh_cold_construct_and_run", |b| {
        b.iter(|| {
            let cap = RegistryRefresh::new();
            let out = tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(cap.run(black_box(serde_json::json!({}))));
            black_box(out)
        });
    });
}

fn bench_registry_refresh_warm(c: &mut Criterion) {
    // Warm path: reuse a pre-constructed capability. Measures steady-state
    // run() cost (embedded snapshot + metrics + tracing).
    let rt = tokio::runtime::Runtime::new().unwrap();
    c.bench_function("registry_refresh_warm_run", |b| {
        let cap = RegistryRefresh::new();
        b.iter(|| {
            let out = rt.block_on(cap.run(black_box(serde_json::json!({}))));
            black_box(out)
        });
    });
}

criterion_group!(
    benches,
    bench_registry_refresh_cold,
    bench_registry_refresh_warm
);
criterion_main!(benches);
