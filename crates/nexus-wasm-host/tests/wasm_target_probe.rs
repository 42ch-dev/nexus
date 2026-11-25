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
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::sleep;
use std::time::Duration;

/// `ETXTBSY`: a filesystem that has only just seen the wrapper written can
/// still report it as busy when exec'd (seen on CI runners).
const ETXTBSY: i32 = 26;

static SCRIPT_SEQ: AtomicU64 = AtomicU64::new(0);

/// Writes an executable shell script with `body` and returns its path.
///
/// Every call gets a fresh file name; the script is flushed and fsynced, and
/// the writing handle is dropped before `chmod` and exec. That closes this
/// thread's descriptor, but not the freshly written script's `ETXTBSY` window
/// (a concurrently forked child can still hold an inherited descriptor), so
/// callers exec through `probe`, whose bounded `wait_until_executable` drain
/// covers the remaining window and returns every non-`ETXTBSY` outcome
/// unchanged.
fn shell_script(dir: &Path, stem: &str, body: &str) -> PathBuf {
    let path = dir.join(format!(
        "{stem}-{}-{}.sh",
        std::process::id(),
        SCRIPT_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = fs::File::create(&path).expect("create shell script");
    writeln!(file, "#!/bin/sh").expect("write shebang");
    write!(file, "{body}").expect("write script body");
    file.flush().expect("flush shell script");
    file.sync_all().expect("sync shell script");
    drop(file);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod shell script");
    path
}

/// Writes an executable wrapper that ignores its arguments and prints `sysroot`.
fn fake_rustc(dir: &Path, sysroot: &Path) -> PathBuf {
    shell_script(
        dir,
        "fake-rustc",
        &format!("printf '%s\\n' '{}'\n", sysroot.display()),
    )
}

/// Runs the probe against `rustc` once the wrapper is definitely exec-able.
///
/// A transient `ETXTBSY` must never stand in for the probe's own verdict, so
/// the exec race is drained here before the assertion looks at the outcome.
fn probe(rustc: &Path) -> bool {
    wait_until_executable(rustc);
    wasm_target_probe::wasm_target_installed(rustc.to_str().expect("utf-8 path"))
}

/// Retries a bare exec of `path` until the kernel stops reporting `ETXTBSY`.
fn wait_until_executable(path: &Path) {
    for attempt in 0..100 {
        match Command::new(path).arg("--print").output() {
            Err(err) if err.raw_os_error() == Some(ETXTBSY) => {
                assert!(attempt < 99, "wrapper never became executable: {err}");
                sleep(Duration::from_millis(10));
            }
            // Exec got past the race; the wrapper's own output is what the
            // probe is meant to judge.
            _ => return,
        }
    }
}

#[test]
fn fake_sysroot_without_target_is_not_installed() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let sysroot = tmp.path().join("sysroot");
    fs::create_dir_all(sysroot.join("lib/rustlib")).expect("create rustlib");
    let rustc = fake_rustc(tmp.path(), &sysroot);

    assert!(
        !probe(&rustc),
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
        probe(&rustc),
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

    let _ = probe(&rustc);
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
    let rustc = shell_script(tmp.path(), "failing-rustc", "echo boom >&2\nexit 2\n");

    let _ = probe(&rustc);
}

#[test]
#[should_panic(expected = "unusable path")]
fn unusable_sysroot_output_panics() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let not_a_directory = tmp.path().join("not-a-directory");
    fs::write(&not_a_directory, b"").expect("create regular file");
    let rustc = fake_rustc(tmp.path(), &not_a_directory);

    let _ = probe(&rustc);
}
