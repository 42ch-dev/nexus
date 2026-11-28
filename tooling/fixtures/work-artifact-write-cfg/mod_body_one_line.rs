// Fixture: a single-line `#[cfg(test)] mod tests { }` closes on that line, so
// the production write after it must still be reported.
#[cfg(test)] mod tests { fn t() { std::fs::write(p, b"test-only"); } }

fn production(path: &str, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
}
