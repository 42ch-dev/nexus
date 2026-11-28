// Fixture: a test-gated macro body that opens and closes on the attribute's own
// line is skipped exactly — including its own write — while the production
// write on the next line is still reported.
#[cfg(test)] mac!{ std::fs::write(p, b"test-only"); }

fn write_fixture(path: &str, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
}
