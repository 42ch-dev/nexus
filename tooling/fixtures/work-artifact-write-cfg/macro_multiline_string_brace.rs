// Fixture: a `}` inside a multi-line ordinary string is not a body brace, so the
// span ends at the macro's own close and the production write below is
// reported (the test write inside is not).
#[cfg(test)]
mac! {
    const TEXT: &str = "first line
} still inside the string";
    fn t() { std::fs::write(p, b"test-only"); }
}

fn production(path: &str, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
}
