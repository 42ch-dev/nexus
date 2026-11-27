// Fixture: a `cfg(any(test, …))` on a struct field opens no test span; the
// write elsewhere in the file must still be reported.
pub struct Registry {
    #[cfg(any(test, feature = "test-hooks"))]
    consultations: usize,
}

impl Registry {
    pub fn write_fixture(path: &str, bytes: &[u8]) {
        std::fs::write(path, bytes).unwrap();
    }
}
