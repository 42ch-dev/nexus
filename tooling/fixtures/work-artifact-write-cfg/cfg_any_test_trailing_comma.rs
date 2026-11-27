// Fixture: a trailing comma (`any(test,)`) leaves no predicate of its own, so
// the attribute still compiles only under `test` and its span is excluded.
#[cfg(any(test,))]
mod tests {
    fn write_fixture(path: &str, bytes: &[u8]) {
        std::fs::write(path, bytes).unwrap();
    }
}
