// Executes the packed crate's public API: only an archive that really carries
// `nexus-fixture-public` can build and run this binary.
fn main() {
    assert_eq!(nexus_fixture_public::answer(), 42);
    println!("nexus-fixture-public answered");
}
