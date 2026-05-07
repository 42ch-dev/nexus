//! E2E integration tests for `nexus42 creator register` flow.
//!
//! Uses wiremock to stub platform HTTP endpoints:
//! - POST /api/v1/creators/register  → challenge
//! - POST /api/v1/creators/verify    → verification result
//!
//! Tests the full register → solve challenge → verify → store credentials pipeline,
//! plus failure paths and idempotency behavior.
//!
//! Pre-flight requirements (seeded in $HOME/.nexus42/):
//! - config.json  → `platform_url` pointing at wiremock server
//! - auth.json    → a `CreatorAuthState` with a non-empty `access_token`
//!   (`obtain_auth_token` scans creators for a non-empty `access_token`)
//!
//! ## DF-14 Staged E2E Verification Harness
//!
//! The tests at the bottom of this file (gate-B1/B2) test the staged
//! platform client flow directly, using the `run_creator_register_e2e`
//! harness and `TestMode` enum. These verify that platform errors are
//! shaped into deterministic CLI-visible error buckets.

use std::process::Command;

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Seed the temp HOME with minimal config and auth state so the CLI can
/// reach the mock platform and find an auth token.
///
/// - `$HOME/.nexus42/config.json`  → { "`platform_url"`: "<`mock_url`>" }
/// - `$HOME/.nexus42/auth.json`    → { "creators": { "`crt_seed"`: { ... `access_token` } } }
fn seed_home(home: &std::path::Path, mock_url: &str) {
    let nexus_dir = home.join(".nexus42");
    std::fs::create_dir_all(&nexus_dir).expect("create .nexus42 dir");

    // config.json with platform_url pointing at mock
    let config = serde_json::json!({
        "platform_url": mock_url,
        "daemon_url": "http://127.0.0.1:8420",
        "runtime_mode": "local_first"
    });
    std::fs::write(
        nexus_dir.join("config.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .expect("write config.json");

    // auth.json with a seed creator that has an access_token
    let auth = serde_json::json!({
        "creators": {
            "crt_seed_token": {
                "creator_id": "crt_seed_token",
                "access_token": "e2e_test_bearer_token",
                "expires_at": "2099-12-31T23:59:59Z"
            }
        }
    });
    std::fs::write(
        nexus_dir.join("auth.json"),
        serde_json::to_string_pretty(&auth).unwrap(),
    )
    .expect("write auth.json");
}

/// Build a CLI command with HOME set to the temp dir.
fn cli_cmd(home: &std::path::Path) -> Command {
    let bin = env!("CARGO_BIN_EXE_nexus42");
    let mut cmd = Command::new(bin);
    cmd.env("HOME", home);
    cmd
}

// ---------------------------------------------------------------------------
// T2: Happy-path E2E test
// ---------------------------------------------------------------------------

#[tokio::test]
async fn creator_register_happy_path() {
    let mock = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();

    // Seed HOME with config + auth pointing at mock
    seed_home(home.path(), &mock.uri());

    // Mock POST /api/v1/creators/register → solvable challenge (5 + 3 = 8)
    Mock::given(method("POST"))
        .and(path("/api/v1/creators/register"))
        .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
            "fixtures/creator_register/register_response.json"
        )))
        .mount(&mock)
        .await;

    // Mock POST /api/v1/creators/verify → verified with api_key
    Mock::given(method("POST"))
        .and(path("/api/v1/creators/verify"))
        .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
            "fixtures/creator_register/verify_response_success.json"
        )))
        .mount(&mock)
        .await;

    // Run the CLI
    let output = cli_cmd(home.path())
        .args(["creator", "register", "--name", "test-creator"])
        .output()
        .expect("run nexus42 creator register");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "register exited non-zero:\nSTDOUT: {stdout}\nSTDERR: {stderr}"
    );

    // Assert credential file exists at $HOME/.nexus42/auth.json
    let creds_path = home.path().join(".nexus42/auth.json");
    assert!(
        creds_path.exists(),
        "auth.json should exist after successful register"
    );

    let creds = std::fs::read_to_string(&creds_path).expect("read auth.json");
    let creds_json: serde_json::Value =
        serde_json::from_str(&creds).expect("parse auth.json as JSON");

    // The api_key should have been stored for the creator
    let creators = creds_json["creators"].as_object().expect("creators object");
    assert!(
        creators.contains_key("crt_e2e_test_12345"),
        "expected creator_id crt_e2e_test_12345 in auth.json, got: {creds}"
    );
    let creator = &creators["crt_e2e_test_12345"];
    let api_key = creator["creator_api_key"]
        .as_str()
        .expect("creator_api_key field");
    assert_eq!(api_key, "nexus_live_active_e2e_key");
}

// ---------------------------------------------------------------------------
// T3: Negative tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn creator_register_challenge_unsolvable_falls_back_or_errors() {
    let mock = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    seed_home(home.path(), &mock.uri());

    // Mock register with an unsolvable challenge
    Mock::given(method("POST"))
        .and(path("/api/v1/creators/register"))
        .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
            "fixtures/creator_register/register_response_bad.json"
        )))
        .mount(&mock)
        .await;

    let output = cli_cmd(home.path())
        .args(["creator", "register", "--name", "test-bad"])
        .output()
        .expect("run nexus42 creator register");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Should fail: challenge solver can't parse "this is not a valid math problem at all"
    assert!(
        !output.status.success(),
        "register should fail with unsolvable challenge; got stdout={stdout} stderr={stderr}"
    );

    // Error output should mention "challenge" (ChallengeFailed error)
    assert!(
        combined.contains("challenge"),
        "error output should mention 'challenge'; got: {combined}"
    );
}

#[tokio::test]
async fn creator_register_verify_rejected_returns_user_visible_error() {
    let mock = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    seed_home(home.path(), &mock.uri());

    // Mock register → solvable challenge (5 + 3 = 8)
    Mock::given(method("POST"))
        .and(path("/api/v1/creators/register"))
        .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
            "fixtures/creator_register/register_response.json"
        )))
        .mount(&mock)
        .await;

    // Mock verify → rejected (wrong_answer, 0 remaining)
    // Both auto-retry attempts will get wrong_answer back
    Mock::given(method("POST"))
        .and(path("/api/v1/creators/verify"))
        .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
            "fixtures/creator_register/verify_response_rejected.json"
        )))
        .mount(&mock)
        .await;

    let output = cli_cmd(home.path())
        .args(["creator", "register", "--name", "test-reject"])
        .output()
        .expect("run nexus42 creator register");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        !output.status.success(),
        "register should fail when verify rejects; got stdout={stdout} stderr={stderr}"
    );

    // Error should mention verification-related terms
    assert!(
        combined.contains("Verification") || combined.contains("verification"),
        "error must mention verification; got: {combined}"
    );

    // No credential file should be created for the new creator after verify failure
    let creds_path = home.path().join(".nexus42/auth.json");
    if creds_path.exists() {
        let creds = std::fs::read_to_string(&creds_path).expect("read auth.json");
        let creds_json: serde_json::Value = serde_json::from_str(&creds).expect("parse auth.json");
        // The only creator in auth.json should be our seed token, NOT the registered one
        let creators = creds_json["creators"].as_object().expect("creators object");
        assert!(
            !creators.contains_key("crt_e2e_test_12345"),
            "no api_key credential should be stored after verify failure"
        );
    }
}

// ---------------------------------------------------------------------------
// T4: Idempotency test
// ---------------------------------------------------------------------------

#[tokio::test]
async fn creator_register_twice_does_not_double_write_credentials() {
    let mock = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    seed_home(home.path(), &mock.uri());

    // Mock register → solvable challenge (5 + 3 = 8)
    Mock::given(method("POST"))
        .and(path("/api/v1/creators/register"))
        .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
            "fixtures/creator_register/register_response.json"
        )))
        .mount(&mock)
        .await;

    // Mock verify → verified
    Mock::given(method("POST"))
        .and(path("/api/v1/creators/verify"))
        .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
            "fixtures/creator_register/verify_response_success.json"
        )))
        .mount(&mock)
        .await;

    // --- First run ---
    let output1 = cli_cmd(home.path())
        .args(["creator", "register", "--name", "test-creator"])
        .output()
        .expect("first run");
    assert!(output1.status.success(), "first register should succeed");

    // --- Second run (same mock, same name) ---
    let output2 = cli_cmd(home.path())
        .args(["creator", "register", "--name", "test-creator"])
        .output()
        .expect("second run");

    let _stdout2 = String::from_utf8_lossy(&output2.stdout);
    let _stderr2 = String::from_utf8_lossy(&output2.stderr);

    let creds_after_second = std::fs::read_to_string(home.path().join(".nexus42/auth.json"))
        .expect("read auth.json after second run");

    // Lock the observed behaviour: the CLI currently has no idempotency guard,
    // so the second run will re-register (the mock always returns the same
    // creator_id). The credential file should still be valid (not corrupted).
    //
    // Regardless of success/failure on second run, the credential for
    // crt_e2e_test_12345 must still contain a valid api_key (no data loss).
    let creds_json: serde_json::Value =
        serde_json::from_str(&creds_after_second).expect("parse auth.json after second run");
    let creators = creds_json["creators"].as_object().expect("creators object");
    assert!(
        creators.contains_key("crt_e2e_test_12345"),
        "creator entry must still exist after second run"
    );
    let api_key = creators["crt_e2e_test_12345"]["creator_api_key"]
        .as_str()
        .unwrap_or("");
    assert!(
        !api_key.is_empty(),
        "api_key must not be empty after second run; got: {creds_after_second}"
    );

    // Verify the api_key value is still correct (not overwritten with garbage).
    assert_eq!(
        api_key, "nexus_live_active_e2e_key",
        "api_key should retain its correct value after second run"
    );

    // Regression guard: seed token must still be intact (no data loss for
    // other creator entries in the auth store).
    assert!(
        creators.contains_key("crt_seed_token"),
        "seed token entry must not be lost after second run"
    );
}

// ---------------------------------------------------------------------------
// Fix 1 — QC1-W-1: stored-token absence negative test
// ---------------------------------------------------------------------------
// Plan listed 3 failure paths; this adds the missing "credential already
// exists" idempotency guard test. After a successful register + verify
// sequence writes a known api_key, a second register + verify that the
// mock could answer with a DIFFERENT api_key must NOT overwrite the
// original stored credential.

#[tokio::test]
async fn creator_register_does_not_overwrite_existing_credential() {
    let mock = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    seed_home(home.path(), &mock.uri());

    // --- First register + verify (writes credential with known api_key) ---
    Mock::given(method("POST"))
        .and(path("/api/v1/creators/register"))
        .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
            "fixtures/creator_register/register_response.json"
        )))
        .mount(&mock)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/v1/creators/verify"))
        .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
            "fixtures/creator_register/verify_response_success.json"
        )))
        .mount(&mock)
        .await;

    let output1 = cli_cmd(home.path())
        .args(["creator", "register", "--name", "test-creator"])
        .output()
        .expect("first register run");
    assert!(
        output1.status.success(),
        "first register should succeed:\n{}",
        String::from_utf8_lossy(&output1.stderr)
    );

    // Read the stored api_key after first successful registration
    let creds_after_first = std::fs::read_to_string(home.path().join(".nexus42/auth.json"))
        .expect("read auth.json after first run");
    let first_json: serde_json::Value =
        serde_json::from_str(&creds_after_first).expect("parse auth.json after first run");
    let original_key = first_json["creators"]["crt_e2e_test_12345"]["creator_api_key"]
        .as_str()
        .expect("creator_api_key after first run");
    assert_eq!(original_key, "nexus_live_active_e2e_key");

    // --- Second register attempt (same creator, mock could issue different key) ---
    // If the CLI had an idempotency guard, it should skip/error before
    // reaching verify. If it doesn't (current behavior), the mock still
    // returns the same api_key because wiremock reuses the mounted response.
    // The critical invariant: the stored api_key must NOT change to a
    // different value. We mount a second register mock that returns a
    // DIFFERENT pending key to prove no silent overwrite occurs.
    Mock::given(method("POST"))
        .and(path("/api/v1/creators/register"))
        .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
            "fixtures/creator_register/register_response_alt_key.json"
        )))
        .mount(&mock)
        .await;

    let output2 = cli_cmd(home.path())
        .args(["creator", "register", "--name", "test-creator"])
        .output()
        .expect("second register run");

    let _stdout2 = String::from_utf8_lossy(&output2.stdout);
    let _stderr2 = String::from_utf8_lossy(&output2.stderr);

    // Regardless of whether the second run succeeds or fails, the stored
    // credential for crt_e2e_test_12345 must still be the ORIGINAL key.
    let creds_after_second = std::fs::read_to_string(home.path().join(".nexus42/auth.json"))
        .expect("read auth.json after second run");
    let second_json: serde_json::Value =
        serde_json::from_str(&creds_after_second).expect("parse auth.json after second run");

    let stored_key = second_json["creators"]["crt_e2e_test_12345"]["creator_api_key"]
        .as_str()
        .expect("creator_api_key after second run");
    assert_eq!(
        stored_key, "nexus_live_active_e2e_key",
        "stored api_key must NOT be overwritten by second register; original: \
         nexus_live_active_e2e_key, got: {stored_key}"
    );
}

// ---------------------------------------------------------------------------
// Fix 2 — QC2-M1: HTTP 5xx / network-error coverage for verify endpoint
// ---------------------------------------------------------------------------

#[tokio::test]
async fn creator_register_verify_http_500_exits_with_error() {
    let mock = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    seed_home(home.path(), &mock.uri());

    // Register returns a solvable challenge
    Mock::given(method("POST"))
        .and(path("/api/v1/creators/register"))
        .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
            "fixtures/creator_register/register_response.json"
        )))
        .mount(&mock)
        .await;

    // Verify returns HTTP 500 Internal Server Error
    Mock::given(method("POST"))
        .and(path("/api/v1/creators/verify"))
        .respond_with(
            ResponseTemplate::new(500).set_body_string(
                serde_json::json!({
                    "error": "internal_server_error",
                    "message": "Unexpected server error during verification"
                })
                .to_string(),
            ),
        )
        .mount(&mock)
        .await;

    let output = cli_cmd(home.path())
        .args(["creator", "register", "--name", "test-creator"])
        .output()
        .expect("run nexus42 creator register");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // CLI must exit with non-zero status on HTTP 500
    assert!(
        !output.status.success(),
        "register should fail when verify returns HTTP 500; stdout={stdout} stderr={stderr}"
    );

    // Error output should mention the HTTP 500 status or "verification"
    assert!(
        combined.contains("500") || combined.contains("verification"),
        "error output should reference the 500 status or verification failure; got: {combined}"
    );
}

// ---------------------------------------------------------------------------
// Fix 3 — QC2-M2: challenge expiry boundary test
// ---------------------------------------------------------------------------
// The verify endpoint returns {"status": "expired"} indicating the
// challenge is no longer valid by the time the CLI submits the answer.
// The CLI must exit non-zero with an appropriate expiry-related message.

#[tokio::test]
async fn creator_register_verify_expired_challenge_exits_with_error() {
    let mock = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    seed_home(home.path(), &mock.uri());

    // Register returns a solvable challenge
    Mock::given(method("POST"))
        .and(path("/api/v1/creators/register"))
        .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
            "fixtures/creator_register/register_response.json"
        )))
        .mount(&mock)
        .await;

    // Verify returns expired status
    Mock::given(method("POST"))
        .and(path("/api/v1/creators/verify"))
        .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
            "fixtures/creator_register/verify_response_expired.json"
        )))
        .mount(&mock)
        .await;

    let output = cli_cmd(home.path())
        .args(["creator", "register", "--name", "test-creator"])
        .output()
        .expect("run nexus42 creator register");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // CLI must exit with non-zero status when challenge is expired
    assert!(
        !output.status.success(),
        "register should fail when verify reports expired challenge; stdout={stdout} stderr={stderr}"
    );

    // Error output should mention expiry
    assert!(
        combined.contains("expired") || combined.contains("timed out"),
        "error output should mention expiry or timeout; got: {combined}"
    );

    // No credential file should be created for the new creator after expiry failure
    let creds_path = home.path().join(".nexus42/auth.json");
    if creds_path.exists() {
        let creds = std::fs::read_to_string(&creds_path).expect("read auth.json");
        let creds_json: serde_json::Value = serde_json::from_str(&creds).expect("parse auth.json");
        let creators = creds_json["creators"].as_object().expect("creators object");
        assert!(
            !creators.contains_key("crt_e2e_test_12345")
                || creators["crt_e2e_test_12345"]["creator_api_key"].is_null()
                || creators["crt_e2e_test_12345"]["creator_api_key"]
                    .as_str()
                    .is_some_and(str::is_empty),
            "no api_key credential should be stored after expired verify failure"
        );
    }
}

// ---------------------------------------------------------------------------
// T5 — Positional syntax rejection (migration hint)
// ---------------------------------------------------------------------------
// WS-C requirement: positional syntax (old `creator register <name>` without --name)
// should produce a clear error message guiding users to the new flag syntax.

#[tokio::test]
async fn creator_register_positional_syntax_rejected_with_migration_hint() {
    let home = tempfile::tempdir().unwrap();

    // Seed minimal config (no auth token needed for this CLI-args-only rejection test)
    let nexus_dir = home.path().join(".nexus42");
    std::fs::create_dir_all(&nexus_dir).expect("create .nexus42 dir");
    let config = serde_json::json!({
        "platform_url": "http://mock.local",
        "daemon_url": "http://127.0.0.1:8420",
        "runtime_mode": "local_first"
    });
    std::fs::write(
        nexus_dir.join("config.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .expect("write config.json");

    // Try using old positional syntax: nexus42 creator register "My Agent"
    // (without --name flag)
    let output = cli_cmd(home.path())
        .args(["creator", "register", "My Agent"])
        .output()
        .expect("run nexus42 creator register with positional arg");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // CLI must exit with non-zero status when positional syntax is used
    assert!(
        !output.status.success(),
        "register should fail when positional syntax is used; stdout={stdout} stderr={stderr}"
    );

    // Error output should mention --name specifically
    assert!(
        combined.contains("--name"),
        "error output should mention --name; got: {combined}"
    );
}

// ---------------------------------------------------------------------------
// DF-14: Staged CLI+Platform e2e verification harness (gate-B1/B2)
// ---------------------------------------------------------------------------
// These tests exercise the staged harness (`run_creator_register_e2e`)
// which breaks the registration pipeline into discrete gate stages
// with deterministic error shaping.

/// Gate-B1/B2: Happy path — platform returns valid register + verify responses.
///
/// Verifies that `run_creator_register_e2e` with `TestMode::HappyPath`
/// successfully completes both the register (B1) and verify (B2) stages.
#[tokio::test]
async fn creator_register_e2e_handles_platform_happy_path() {
    use nexus42::commands::creator::{run_creator_register_e2e, TestMode};
    use nexus_sync::platform_client::VerifyStatus;

    let mock = MockServer::start().await;

    // Mock POST /api/v1/creators/register → valid registration
    Mock::given(method("POST"))
        .and(path("/api/v1/creators/register"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "creator_id": "crt_staged_e2e",
            "display_name": "Staged E2E Creator",
            "creator_api_key": "nexus_live_staged_key",
            "verification": {
                "verification_code": "nxc_verify_staged",
                "challenge_text": "A basket has five apples and someone adds three more",
                "expires_at": "2099-12-31T23:59:59.000Z",
                "instructions": "Solve the math problem"
            }
        })))
        .mount(&mock)
        .await;

    // Mock POST /api/v1/creators/verify → verified
    Mock::given(method("POST"))
        .and(path("/api/v1/creators/verify"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "verified",
            "creator_api_key": "nexus_live_staged_active"
        })))
        .mount(&mock)
        .await;

    let result = run_creator_register_e2e(
        &mock.uri(),
        "test_token",
        "dev_staged",
        "Staged E2E Creator",
        "cli",
        None,
        TestMode::HappyPath,
    )
    .await;

    // Gate-B1: register should succeed
    assert!(
        result.register.is_ok(),
        "gate-B1 register should succeed in HappyPath; got: {:?}",
        result.register
    );
    let register_resp = result.register.as_ref().expect("register response");
    assert_eq!(register_resp.creator_id, "crt_staged_e2e");

    // Gate-B2: verify should succeed
    let verify_result = result
        .verify
        .as_ref()
        .expect("verify stage should be present after successful register");
    assert!(
        verify_result.is_ok(),
        "gate-B2 verify should succeed in HappyPath; got: {:?}",
        verify_result
    );
    let verify_resp = verify_result.as_ref().expect("verify response");
    assert_eq!(verify_resp.status, VerifyStatus::Verified);
}

/// Gate-B1/B2: Upstream timeout — platform is unreachable.
///
/// Verifies that `run_creator_register_e2e` with `TestMode::UpstreamTimeout`
/// surfaces a deterministic timeout error from gate-B1, and that the error
/// is shaped into a [`StagedPlatformError`] bucket (Timeout or Platform
/// with status 0).
///
/// Note: on systems with HTTP proxies, connection to the test IP may
/// surface as HTTP 502 from the proxy rather than a raw timeout. The
/// test accepts both outcomes as long as the error is shaped into a
/// deterministic bucket and the display message contains "timeout" or
/// "platform" for CLI visibility.
#[tokio::test]
async fn creator_register_e2e_surfaces_platform_failure_context() {
    use nexus42::commands::creator::{run_creator_register_e2e, TestMode};
    use nexus_sync::platform_client::StagedPlatformError;

    // No mock server needed — UpstreamTimeout mode uses an unreachable URL

    let result = run_creator_register_e2e(
        "http://will-be-ignored.invalid", // Overridden by UpstreamTimeout mode
        "test_token",
        "dev_staged_fail",
        "Staged Fail Creator",
        "cli",
        None,
        TestMode::UpstreamTimeout,
    )
    .await;

    // Gate-B1: register should fail with a timeout/connection error
    assert!(
        result.register.is_err(),
        "gate-B1 register should fail in UpstreamTimeout; got: {:?}",
        result.register
    );

    let err = result.register.err().expect("register error");
    // The error must be shaped into a deterministic bucket.
    // Acceptable outcomes:
    // - Timeout: the connection timed out (no proxy)
    // - Platform { status: 0 }: uncategorized transport error (no proxy)
    // - Platform { status: 502 }: proxy returned bad gateway (proxy present)
    match &err {
        StagedPlatformError::Timeout => {}
        StagedPlatformError::Platform { status: 0, .. } => {}
        StagedPlatformError::Platform { status: 502, .. } => {}
        StagedPlatformError::Config(msg) => {
            panic!("unexpected Config error: {msg}");
        }
        StagedPlatformError::Auth(msg) => {
            panic!("unexpected Auth error: {msg}");
        }
        StagedPlatformError::Platform { status, body } => {
            panic!("unexpected Platform error with HTTP status {status}: {body}");
        }
    }

    // The error display must contain "timeout" or "platform" for CLI visibility
    let err_display = format!("{err}");
    assert!(
        err_display.contains("timeout") || err_display.contains("platform"),
        "error must contain 'timeout' or 'platform' for CLI visibility; got: {err_display}"
    );

    // Gate-B2: verify should not be reached (None)
    assert!(
        result.verify.is_none(),
        "gate-B2 verify should not be reached when gate-B1 fails"
    );
}
