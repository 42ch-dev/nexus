// Fixture: a test-gated macro body that never closes fails closed with a named
// diagnostic instead of skipping the rest of the file.
#[cfg(test)]
paste::paste! {
    mod generated {
        fn write_fixture(path: &str, bytes: &[u8]) {
