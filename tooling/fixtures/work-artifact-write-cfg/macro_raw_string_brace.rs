// Fixture: braces inside a multi-line raw string are not body braces, so the
// span ends at the macro's own close and the production write below is
// reported (the test write inside is not).
#[cfg(test)]
mac! {
    const RAW: &str = r#"first {
} still raw"#;
    fn t() { std::fs::write(p, b"test-only"); }
}

fn production(path: &str, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
}
