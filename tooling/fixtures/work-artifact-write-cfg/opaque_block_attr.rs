// Fixture: a test-gated attribute on a block expression statement is opaque to
// the span tracker, so the guard fails closed instead of scanning past it.
fn write_only_in_tests() {
    #[cfg(test)]
    {
        let path = "Works/wrk_x/ch1.md";
        std::fs::write(path, b"x").unwrap();
    }
}
