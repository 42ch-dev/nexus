//! Test-only `tracing` capture helpers shared by DAO mutation-path tests.
//!
//! Extracted (R-V146P4-QC1-S1 / R-V146P4-QC3-S1) from the inline
//! `CaptureLayer` originally defined in
//! `novel_pool_entries::tests::test_promote_to_active_emits_trace`, so every
//! instrumented mutation path across `novel_pool_entries` and
//! `inspiration_items` can assert its structured `tracing::info!` without
//! duplicating the ~40-line layer/visitor boilerplate per test.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

/// `tracing_subscriber` layer that records every INFO event's fields into a
/// shared buffer. Clone-safe via `Arc<Mutex<...>>`.
#[derive(Clone)]
pub(crate) struct CaptureLayer {
    /// Captured INFO-event field renderings (one String per event).
    pub messages: Arc<Mutex<Vec<String>>>,
}

impl<S> tracing_subscriber::Layer<S> for CaptureLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if event.metadata().level() == &tracing::Level::INFO {
            let mut visitor = CaptureVisitor(String::new());
            event.record(&mut visitor);
            let mut msgs = self.messages.lock().unwrap();
            msgs.push(visitor.0);
        }
    }
}

struct CaptureVisitor(String);
impl tracing::field::Visit for CaptureVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        use std::fmt::Write;
        let _ = write!(&mut self.0, "{}={:?} ", field.name(), value);
    }
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        use std::fmt::Write;
        let _ = write!(&mut self.0, "{}={} ", field.name(), value);
    }
}

/// Build a fresh capture layer + its shared message buffer.
pub(crate) fn capture_layer() -> (CaptureLayer, Arc<Mutex<Vec<String>>>) {
    let messages = Arc::new(Mutex::new(Vec::new()));
    (
        CaptureLayer {
            messages: messages.clone(),
        },
        messages,
    )
}

/// Compose a `tracing` subscriber wired to `layer` for use with
/// `tracing::subscriber::set_default`. The composed subscriber owns its data
/// (Registry + `Arc`-backed layer) and is `Send + Sync + 'static`.
pub(crate) fn subscriber_with(
    layer: CaptureLayer,
) -> impl tracing::Subscriber + Send + Sync + 'static {
    use tracing_subscriber::layer::SubscriberExt;
    <tracing_subscriber::Registry as tracing_subscriber::layer::SubscriberExt>::with(
        tracing_subscriber::registry::Registry::default(),
        layer,
    )
}

/// Assert at least one captured INFO event contains every needle in `needles`.
///
/// Use inside the `set_default` guard scope OR after it (the buffer is shared
/// and outlives the guard).
pub(crate) fn assert_info_emitted(messages: &Arc<Mutex<Vec<String>>>, needles: &[&str]) {
    let msgs = messages.lock().unwrap();
    assert!(
        msgs.iter().any(|m| needles.iter().all(|n| m.contains(n))),
        "expected an INFO trace containing all of {needles:?}; captured: {msgs:?}"
    );
}
