//! V1.58 P0 T14 (R-V156P1-L005) + fix-wave QC3 F-004 — registry.refresh
//! latency benchmark.
//!
//! Measures the `RegistryRefresh` capability invocation latency across three
//! regimes:
//!
//! - `synthetic_cold_construct_and_run` — construct a fresh capability + run
//!   (measures allocation + first-call overhead; the synthetic path exercises
//!   the embedded snapshot and the metrics/tracing fast-paths).
//! - `synthetic_warm_run` — reuse an already-constructed capability across
//!   iterations (measures steady-state `run()` cost; this is the spec's
//!   primary latency target).
//! - `fallback_warm_run` — CDN configured with a literal blocked IP
//!   (`127.0.0.1`); the fetch fails fast at the `is_blocked_ip` guard (no
//!   DNS, no retry, no network I/O) and the capability serves the embedded
//!   synthetic snapshot as `source = "synthetic_fallback"`. Measures the
//!   fallback overhead deterministically — no external I/O, no mock server
//!   required.
//!
//! # Latency targets (V1.58 spec)
//!
//! | Path                             | Target  | Notes                                              |
//! |----------------------------------|---------|----------------------------------------------------|
//! | `synthetic_warm_run`             | < 1 ms  | Steady-state `run()`; snapshot + metrics + tracing |
//! | `synthetic_cold_construct_and_run` | < 5 ms | Adds capability allocation + LazyLock init         |
//! | `fallback_warm_run`              | < 5 ms  | Adds `CdnError` construction + blocked-IP guard    |
//!
//! The actual CDN/network happy path is intentionally NOT benchmarked here
//! because it depends on external I/O (DNS, TLS, HTTP) that is
//! non-deterministic in CI. A mock-server benchmark for the happy CDN path
//! is deferred to a future plan that can stand up a local HTTPS fixture
//! (QC3 F-004 deferral note).
//!
//! Run with: `cargo bench -p nexus-orchestration --bench registry_refresh_latency`

use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use nexus_orchestration::capability::{
    builtins::registry::{CdnConfig, RegistryRefresh, DEFAULT_MAX_CDN_BODY_SIZE},
    Capability,
};

/// V1.58 P0 fix-wave (QC3 F-004): explicit Criterion config instead of the
/// default. The default Criterion config is 3s warm-up / 5s measurement /
/// 100 samples; we tighten warm-up and measurement to keep the bench
/// runnable in CI under 15s total while keeping the default sample size for
/// statistical confidence. Made explicit so the config is self-documenting
/// and tunable.
fn criterion_config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .sample_size(100)
}

fn bench_synthetic_cold(c: &mut Criterion) {
    // Cold path: construct + run. Measures allocation + first-call overhead.
    // Target: < 5 ms (adds capability allocation + LazyLock init).
    c.bench_function("synthetic_cold_construct_and_run", |b| {
        b.iter(|| {
            let cap = RegistryRefresh::new();
            let out = tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(cap.run(black_box(serde_json::json!({}))));
            black_box(out)
        });
    });
}

fn bench_synthetic_warm(c: &mut Criterion) {
    // Warm path: reuse a pre-constructed capability. Measures steady-state
    // run() cost (embedded snapshot + metrics + tracing).
    // Target: < 1 ms (V1.58 spec primary latency target).
    let rt = tokio::runtime::Runtime::new().unwrap();
    c.bench_function("synthetic_warm_run", |b| {
        let cap = RegistryRefresh::new();
        b.iter(|| {
            let out = rt.block_on(cap.run(black_box(serde_json::json!({}))));
            black_box(out)
        });
    });
}

fn bench_fallback_warm(c: &mut Criterion) {
    // Fallback path (V1.58 P0 fix-wave — QC3 F-004): CDN configured with a
    // literal blocked IP. `fetch_from_cdn` fails fast at `is_blocked_ip`
    // (the host is a literal loopback address; `tokio::net::lookup_host`
    // returns immediately without DNS; the retry loop is never entered
    // because `BlockedHost` returns before it). The capability then serves
    // the embedded synthetic snapshot with `source = "synthetic_fallback"`.
    // Deterministic, no external I/O, no mock server.
    // Target: < 5 ms (adds CdnError construction + blocked-IP guard).
    let rt = tokio::runtime::Runtime::new().unwrap();
    c.bench_function("fallback_warm_run", |b| {
        let cap = RegistryRefresh::with_cdn(CdnConfig {
            url: "https://127.0.0.1/registry.json".to_string(),
            timeout_ms: 1_000,
            max_retries: 0,
            max_body_bytes: DEFAULT_MAX_CDN_BODY_SIZE,
        });
        b.iter(|| {
            let out = rt.block_on(cap.run(black_box(serde_json::json!({}))));
            black_box(out)
        });
    });
}

criterion_group! {
    name = benches;
    config = criterion_config();
    targets =
        bench_synthetic_cold,
        bench_synthetic_warm,
        bench_fallback_warm
}
criterion_main!(benches);
