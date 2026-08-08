//! V1.155 P1 T1 — `nexus42 connect token issue` CLI surface (capability-token
//! production issuance).
//!
//! Runs the REAL binary against a hermetic `HOME` and proves the contract:
//! token JSON on stdout (issue → verify green with the derived issuer peer
//! id), no secrets echoed, create-once 0600 issuer key, and usage errors
//! exit non-zero. Compiled only with `--features connect-host` (same gate
//! as the `connect` command).

#![cfg(feature = "connect-host")]

use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;
use std::process::Output;
use tempfile::TempDir;

/// The canonical key file path inside a hermetic home (literal `.nexus42`
/// join so the test does not need the layout crate).
fn issuer_key_path(home: &Path) -> std::path::PathBuf {
    home.join(".nexus42").join("connect").join("issuer.key")
}

fn cmd_with_home(home: &Path) -> assert_cmd::Command {
    let mut cmd = Command::cargo_bin("nexus42").expect("nexus42 binary");
    cmd.env("HOME", home);
    cmd
}

fn now_plus(secs: u64) -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs()
        + secs
}

/// Run `connect token issue` with the standard happy-path flags plus `extra`.
fn issue(home: &Path, extra: &[&str]) -> Output {
    let mut cmd = cmd_with_home(home);
    cmd.args([
        "connect",
        "token",
        "issue",
        "--sub",
        "subject-peer",
        "--aud",
        "audience-peer",
        "--capabilities",
        "spoke-baseline",
        "--exp",
        &now_plus(3600).to_string(),
    ]);
    cmd.args(extra);
    cmd.output().expect("run issue")
}

#[test]
fn token_issue_prints_verifiable_token_and_no_secrets() {
    let tmp = TempDir::new().expect("tempdir");
    let out = issue(tmp.path(), &[]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // stdout is the token JSON — nothing else (no key material, no status
    // chatter).
    let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
    let proof: Value = serde_json::from_str(stdout.trim()).expect("stdout parses as token JSON");
    assert_eq!(proof["v"], 1, "wire version 1");
    let claims = &proof["claims"];
    assert_eq!(claims["sub"], "subject-peer");
    assert_eq!(claims["aud"], "audience-peer");
    assert_eq!(claims["capabilities"][0], "spoke-baseline");
    let sig = proof["sig"].as_str().expect("sig string");
    assert!(!sig.is_empty(), "signature present");

    // The issuer peer id derives from the on-disk key; default iss equals it.
    let key_bytes = std::fs::read(issuer_key_path(tmp.path())).expect("issuer.key on disk");
    let keypair = libp2p::identity::Keypair::from_protobuf_encoding(&key_bytes)
        .expect("key parses as protobuf");
    let issuer = keypair.public().to_peer_id().to_string();
    assert_eq!(claims["iss"], issuer, "default iss = issuer-derived peer id");

    // Issue → verify green (spoke, correct trusted_issuers).
    let proof: spoke_connect::core::CapabilityTokenProof =
        serde_json::from_str(stdout.trim()).expect("proof shape");
    let granted = spoke_connect::core::verify_capability_token(
        &proof,
        &[issuer],
        "audience-peer",
        "subject-peer",
        now_plus(120),
    )
    .expect("token verifies green");
    assert_eq!(granted, vec!["spoke-baseline".to_string()]);

    // No secrets echoed: the raw stdout is exactly the JSON token (claims
    // carry only the public issuer peer id — never key material).
    assert!(!stdout.contains("issuer.key"), "key path must not leak");
    assert!(!stdout.contains("-----BEGIN"), "no PEM-style key material");
    assert!(
        !stdout.contains("ed25519"),
        "no key-encoding labels leak"
    );
}

#[test]
fn token_issue_requires_sub_and_aud() {
    let tmp = TempDir::new().expect("tempdir");
    let out = cmd_with_home(tmp.path())
        .args([
            "connect",
            "token",
            "issue",
            "--aud",
            "audience-peer",
            "--capabilities",
            "spoke-baseline",
            "--exp",
            &now_plus(3600).to_string(),
        ])
        .output()
        .expect("run");
    assert!(!out.status.success(), "missing --sub must fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--sub"),
        "usage error names the missing flag: {stderr}"
    );
}

#[test]
fn token_issue_rejects_iss_override_mismatch() {
    let tmp = TempDir::new().expect("tempdir");
    let out = cmd_with_home(tmp.path())
        .args([
            "connect",
            "token",
            "issue",
            "--sub",
            "subject-peer",
            "--aud",
            "audience-peer",
            "--capabilities",
            "spoke-baseline",
            "--exp",
            &now_plus(3600).to_string(),
            "--iss",
            "12D3KooWNotTheIssuerKey",
        ])
        .output()
        .expect("run");
    assert!(!out.status.success(), "--iss mismatch must fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--iss"),
        "error names the iss conflict: {stderr}"
    );
}

#[test]
fn token_issue_rejects_exp_within_skew_window() {
    let tmp = TempDir::new().expect("tempdir");
    let out = cmd_with_home(tmp.path())
        .args([
            "connect",
            "token",
            "issue",
            "--sub",
            "subject-peer",
            "--aud",
            "audience-peer",
            "--capabilities",
            "spoke-baseline",
            "--exp",
            &now_plus(30).to_string(),
        ])
        .output()
        .expect("run");
    assert!(!out.status.success(), "exp within the 60s skew window must fail");
}

#[test]
fn token_issue_rejects_empty_capabilities() {
    let tmp = TempDir::new().expect("tempdir");
    let out = cmd_with_home(tmp.path())
        .args([
            "connect",
            "token",
            "issue",
            "--sub",
            "subject-peer",
            "--aud",
            "audience-peer",
            "--capabilities",
            "",
            "--exp",
            &now_plus(3600).to_string(),
        ])
        .output()
        .expect("run");
    assert!(
        !out.status.success(),
        "empty --capabilities must fail: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn token_issue_reuses_issuer_key_across_invocations() {
    let tmp = TempDir::new().expect("tempdir");
    let key_path = issuer_key_path(tmp.path());

    let out1 = issue(tmp.path(), &[]);
    assert!(out1.status.success(), "first issue");
    let bytes1 = std::fs::read(&key_path).expect("key created on first issue");

    let out2 = issue(tmp.path(), &[]);
    assert!(out2.status.success(), "second issue");
    let bytes2 = std::fs::read(&key_path).expect("key present on second issue");
    assert_eq!(bytes1, bytes2, "create-once: second issue reuses the key");

    let v1: Value = serde_json::from_slice(&out1.stdout).expect("token 1");
    let v2: Value = serde_json::from_slice(&out2.stdout).expect("token 2");
    assert_eq!(
        v1["claims"]["iss"], v2["claims"]["iss"],
        "stable issuer across invocations"
    );
}

#[cfg(unix)]
#[test]
fn token_issue_creates_issuer_key_0600() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = TempDir::new().expect("tempdir");
    let out = issue(tmp.path(), &[]);
    assert!(out.status.success(), "issue succeeds");
    let mode = std::fs::metadata(issuer_key_path(tmp.path()))
        .expect("key metadata")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600, "issuer key must be owner-only (0600)");
}

#[test]
fn connect_token_help_lists_issue() {
    let tmp = TempDir::new().expect("tempdir");
    cmd_with_home(tmp.path())
        .args(["connect", "token", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("issue"));
}
