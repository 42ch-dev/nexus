// Fixture: `cfg(any(test, feature = "test-hooks"))` selects test support, so
// the span is excluded. Before I-000408 this module was scanned and the write
// below was reported as a production violation.
#[cfg(any(test, feature = "test-hooks"))]
pub mod test_support {
    pub fn write_fixture(path: &str, bytes: &[u8]) {
        std::fs::write(path, bytes).unwrap();
    }
}
