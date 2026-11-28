// Fixture: an out-of-line `mod tests;` declaration opens no span — its body is
// another file — so the production write below must still be reported.
#[cfg(test)]
mod tests;

fn write_fixture(path: &str, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
}
