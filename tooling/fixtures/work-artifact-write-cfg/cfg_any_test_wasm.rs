// Fixture: `cfg(any(test, feature = "wasm"))` is not a test gate — the `wasm`
// disjunct is a production selection — so the module stays scanned and the
// write below is reported.
#[cfg(any(test, feature = "wasm"))]
pub mod compute {
    pub fn write_fixture(path: &str, bytes: &[u8]) {
        std::fs::write(path, bytes).unwrap();
    }
}
