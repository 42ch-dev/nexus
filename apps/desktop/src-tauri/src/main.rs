// Nexus Tauri v2 desktop shell — binary entry. All app logic lives in `lib.rs`
// so it can be reused by Tauri mobile entry points in the future.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    nexus_desktop::run();
}
