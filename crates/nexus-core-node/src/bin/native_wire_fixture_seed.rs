//! Seeds a disposable home for native wire proof (mirrors world_kb_contract fixture).

use std::path::PathBuf;

use nexus_core_node::wire_fixture::seed_wire_home;

#[tokio::main]
async fn main() {
    let user_home = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("usage: native-wire-fixture-seed <user_home>"),
    );
    seed_wire_home(&user_home).await;
    println!("{}", user_home.display());
}
