//! Automated control for the shared `wasm32-unknown-unknown` probe.
//!
//! The probe is exercised against fake sysroots printed by a shell wrapper, so
//! this test does not depend on the host toolchain. The negative case — a
//! sysroot without the target directory, wrapper exiting 0 — is exactly what
//! the old exit-status-only logic reported as installed.
#![cfg(unix)]

#[path = "../wasm_target_probe.rs"]
mod wasm_target_probe;

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Writes an executable shell script with `body` and returns its path.
fn shell_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    let mut file = fs::File::create(&path).expect("create shell script");
    writeln!(file, "#!/bin/sh").expect("write shebang");
    write!(file, "{body}").expect("write script body");
    drop(file);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod shell script");
    path
}

/// Writes an executable wrapper that ignores its arguments and prints `sysroot`.
fn fake_rustc(dir: &Path, sysroot: &Path) -> PathBuf {
    shell_script(
        dir,
        "fake-rustc.sh",
        &format!("printf '%s\\n' '{}'\n", sysroot.display()),
    )
}

#[test]
fn fake_sysroot_without_target_is_not_installed() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let sysroot = tmp.path().join("sysroot");
    fs::create_dir_all(sysroot.join("lib/rustlib")).expect("create rustlib");
    let rustc = fake_rustc(tmp.path(), &sysroot);

    assert!(
        !wasm_target_probe::wasm_target_installed(rustc.to_str().expect("utf-8 path")),
        "probe reported the target installed for a sysroot without lib/rustlib/wasm32-unknown-unknown"
    );
}

#[test]
fn fake_sysroot_with_target_is_installed() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let sysroot = tmp.path().join("sysroot");
    fs::create_dir_all(sysroot.join("lib/rustlib/wasm32-unknown-unknown"))
        .expect("create target dir");
    let rustc = fake_rustc(tmp.path(), &sysroot);

    assert!(
        wasm_target_probe::wasm_target_installed(rustc.to_str().expect("utf-8 path")),
        "probe failed to see a sysroot containing lib/rustlib/wasm32-unknown-unknown"
    );
}

#[test]
#[should_panic(expected = "not a directory")]
fn non_directory_target_entry_panics() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let sysroot = tmp.path().join("sysroot");
    fs::create_dir_all(sysroot.join("lib/rustlib")).expect("create rustlib");
    fs::write(sysroot.join("lib/rustlib/wasm32-unknown-unknown"), b"").expect("create stray file");
    let rustc = fake_rustc(tmp.path(), &sysroot);

    let _ = wasm_target_probe::wasm_target_installed(rustc.to_str().expect("utf-8 path"));
}

#[test]
#[should_panic(expected = "could not be run")]
fn missing_rustc_panics() {
    let _ = wasm_target_probe::wasm_target_installed("/nonexistent/rustc");
}

#[test]
#[should_panic(expected = "failed with")]
fn failing_rustc_panics() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let rustc = shell_script(tmp.path(), "failing-rustc.sh", "echo boom >&2\nexit 2\n");

    let _ = wasm_target_probe::wasm_target_installed(rustc.to_str().expect("utf-8 path"));
}

#[test]
#[should_panic(expected = "unusable path")]
fn unusable_sysroot_output_panics() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let not_a_directory = tmp.path().join("not-a-directory");
    fs::write(&not_a_directory, b"").expect("create regular file");
    let rustc = fake_rustc(tmp.path(), &not_a_directory);

    let _ = wasm_target_probe::wasm_target_installed(rustc.to_str().expect("utf-8 path"));
}
