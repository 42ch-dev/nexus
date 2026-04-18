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
//! - config.json  → platform_url pointing at wiremock server
//! - auth.json    → a CreatorAuthState with a non-empty access_token
//!   (obtain_auth_token scans creators for a non-empty access_token)

use std::process::Command;

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Seed the temp HOME with minimal config and auth state so the CLI can
/// reach the mock platform and find an auth token.
///
/// - `$HOME/.nexus42/config.json`  → { "platform_url": "<mock_url>" }
/// - `$HOME/.nexus42/auth.json`    → { "creators": { "crt_seed": { ... access_token } } }
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
        .args(["creator", "register", "test-creator"])
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
        .args(["creator", "register", "test-bad"])
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
        .args(["creator", "register", "test-reject"])
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
        .args(["creator", "register", "test-creator"])
        .output()
        .expect("first run");
    assert!(output1.status.success(), "first register should succeed");

    // --- Second run (same mock, same name) ---
    let output2 = cli_cmd(home.path())
        .args(["creator", "register", "test-creator"])
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
