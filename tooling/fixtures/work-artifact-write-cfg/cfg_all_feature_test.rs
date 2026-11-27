// Fixture: `cfg(all(feature = "test-hooks", test))` requires `test`, so the
// span is excluded like `cfg(test)`.
#[cfg(all(feature = "test-hooks", test))]
mod test_support {
    fn write_fixture(path: &str, bytes: &[u8]) {
        fs::write(path, bytes).unwrap();
    }
}
