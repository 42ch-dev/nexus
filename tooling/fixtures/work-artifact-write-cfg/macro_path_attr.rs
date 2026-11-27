// Fixture: a test-gated path-qualified macro invocation is a test span — its
// brace body is matched and excluded — so the write inside is not reported.
#[cfg(test)]
paste::paste! {
    mod generated {
        fn write_fixture(path: &str, bytes: &[u8]) {
            std::fs::write(path, bytes).unwrap();
        }
    }
}
