// Fixture: `cfg(test)` on a module — the test span is excluded, so the direct
// write below is never a production violation.
#[cfg(test)]
mod tests {
    fn write_fixture(path: &str, bytes: &[u8]) {
        std::fs::write(path, bytes).unwrap();
    }
}
