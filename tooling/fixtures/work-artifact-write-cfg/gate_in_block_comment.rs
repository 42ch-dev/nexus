// Fixture: a gate inside a block comment is not code, so it opens no span and
// the production write below is reported — exactly once.
/*
#[cfg(test)] mod tests { fn t() { std::fs::write(p, b"test-only"); } }
*/
fn production(path: &str, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
}
