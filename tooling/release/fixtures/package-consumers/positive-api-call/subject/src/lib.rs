/// Public API the fixture consumer calls; a build that never links the archive
/// fails, so the positive case cannot pass as a dependency-free no-op.
pub fn answer() -> u32 {
    42
}
