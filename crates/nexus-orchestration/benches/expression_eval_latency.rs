//! V1.60 P-last (R-V156P2-L001) — preset expression evaluator latency bench.
//!
//! Mirrors the `registry_refresh_latency` bench shape: explicit Criterion
//! config (tight warm-up / measurement so the bench runs in CI under 15s),
//! three regimes covering the evaluator's hot paths.
//!
//! ## Regimes
//!
//! - `simple_field_truthy` — `_context.user.is_admin` style field access +
//!   truthy coercion. Exercises `resolve_field` + `is_truthy` (the leaf
//!   fast-path most `when:` expressions hit).
//! - `comparison_numeric` — `_context.score >= 0.8` style numeric comparison.
//!   Exercises `eval_value` + `compare` (the canonical routing condition).
//! - `complex_boolean_nested` — a 4-deep `(a && b) || (c && !d)` expression
//!   with mixed comparisons. Exercises recursive descent + short-circuit.
//!
//! ## Latency target
//!
//! Expression evaluation runs in the preset-state-machine hot loop (every
//! `exit_when` check on every state visit). Target: **< 10 µs** per
//! evaluation on the complex regime (parsed AST reuse; parse cost is
//! amortized across state visits and is intentionally NOT benchmarked here —
//! it is a one-shot loader cost, not a steady-state cost).
//!
//! Run with: `cargo bench -p nexus-orchestration --bench expression_eval_latency`

use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use nexus_orchestration::preset::expr::{evaluate, parse};

/// V1.58 P0 fix-wave (QC3 F-004) pattern: explicit Criterion config instead
/// of the default so the bench is CI-friendly (< 15s total) while keeping
/// the default sample size for statistical confidence.
fn criterion_config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .sample_size(100)
}

/// Build a representative evaluation context (camelCase + snake_case keys,
/// nested objects, mixed types — mirrors real `_context` shape after
/// `build_context_json`).
fn sample_context() -> serde_json::Value {
    serde_json::json!({
        "user": { "is_admin": true, "plan": "pro", "id": "usr_123" },
        "score": 0.87,
        "status": "ready",
        "_judge_result": true,
        "_judge_reason": "all checks passed",
        "registry_refresh": {
            "source": "cdn",
            "capabilityCount": 31
        },
        "items": [1, 2, 3]
    })
}

fn bench_simple_field_truthy(c: &mut Criterion) {
    // Leaf fast-path: field access + truthy coercion.
    // Target: < 1 µs.
    let expr = parse("_context.user.is_admin").expect("parse simple field");
    let ctx = sample_context();
    c.bench_function("simple_field_truthy", |b| {
        b.iter(|| {
            let r = evaluate(black_box(&expr), black_box(&ctx));
            black_box(r)
        });
    });
}

fn bench_comparison_numeric(c: &mut Criterion) {
    // Canonical routing condition: numeric comparison.
    // Target: < 5 µs.
    let expr = parse("_context.score >= 0.8").expect("parse comparison");
    let ctx = sample_context();
    c.bench_function("comparison_numeric", |b| {
        b.iter(|| {
            let r = evaluate(black_box(&expr), black_box(&ctx));
            black_box(r)
        });
    });
}

fn bench_complex_boolean_nested(c: &mut Criterion) {
    // 4-deep mixed boolean: exercises recursive descent + short-circuit.
    // Target: < 10 µs.
    let expr = parse(
        "(_context.user.is_admin && _context.score >= 0.8) \
         || (_context.status == 'ready' && !(_context.registry_refresh.source == 'synthetic_fallback'))",
    )
    .expect("parse complex boolean");
    let ctx = sample_context();
    c.bench_function("complex_boolean_nested", |b| {
        b.iter(|| {
            let r = evaluate(black_box(&expr), black_box(&ctx));
            black_box(r)
        });
    });
}

criterion_group! {
    name = benches;
    config = criterion_config();
    targets =
        bench_simple_field_truthy,
        bench_comparison_numeric,
        bench_complex_boolean_nested
}
criterion_main!(benches);
