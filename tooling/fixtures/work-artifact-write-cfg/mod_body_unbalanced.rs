// Fixture: a test-gated module body that never closes fails closed with a named
// diagnostic instead of skipping the rest of the file.
#[cfg(test)]
mod tests {
    fn t(path: &str) {
