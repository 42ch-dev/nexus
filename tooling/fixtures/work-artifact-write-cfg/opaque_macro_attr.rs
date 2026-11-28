// Fixture: a test-gated attribute on a macro invocation is an opaque body the
// span tracker cannot follow, so the guard fails closed instead of scanning
// past it.
#[cfg(test)]
include!("fixture_tests_body.rs");
