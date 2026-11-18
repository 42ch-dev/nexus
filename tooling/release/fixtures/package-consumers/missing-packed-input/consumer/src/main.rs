// Reaches the generated type the archive omits: the build must fail with a
// named missing-input cause instead of passing on an incomplete archive.
fn main() {
    let answer = nexus_fixture_generated::Answer(42);
    assert_eq!(answer.0, 42);
    println!("generated answer");
}
