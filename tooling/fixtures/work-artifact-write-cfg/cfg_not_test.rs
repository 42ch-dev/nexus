// Fixture: `cfg(not(test))` is production, so the write below must still be
// reported as a violation.
#[cfg(not(test))]
pub mod runtime {
    pub fn write_fixture(path: &str, bytes: &[u8]) {
        std::fs::write(path, bytes).unwrap();
    }
}
