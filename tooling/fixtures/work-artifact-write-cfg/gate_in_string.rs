// Fixture: a gate inside a multi-line ordinary string is not code, so it opens
// no span and the production write below is reported — exactly once.
const EXAMPLE: &str = "docs:
#[cfg(test)] mod tests { fn t() { std::fs::write(p, b\"test-only\"); } }
still inside the string";

fn production(path: &str, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
}
