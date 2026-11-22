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

#[tokio::test]
async fn creator_register_happy_path() {
    let mock = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();

    seed_home(home.path(), &mock.uri());

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

    let creds_path = home.path().join(".nexus42/auth.json");
    assert!(
        creds_path.exists(),
        "auth.json should exist after successful register"
    );

    let creds = std::fs::read_to_string(&creds_path).expect("read auth.json");
    let creds_json: serde_json::Value =
        serde_json::from_str(&creds).expect("parse auth.json as JSON");

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

#[tokio::test]
async fn creator_register_challenge_unsolvable_falls_back_or_errors() {
    let mock = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    seed_home(home.path(), &mock.uri());

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

    assert!(
        !output.status.success(),
        "register should fail with unsolvable challenge; got stdout={stdout} stderr={stderr}"
    );

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

    Mock::given(method("POST"))
        .and(path("/api/v1/creators/register"))
        .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
            "fixtures/creator_register/register_response.json"
        )))
        .mount(&mock)
        .await;

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

    assert!(
        combined.contains("Verification") || combined.contains("verification"),
        "error must mention verification; got: {combined}"
    );

    let creds_path = home.path().join(".nexus42/auth.json");
    if creds_path.exists() {
        let creds = std::fs::read_to_string(&creds_path).expect("read auth.json");
        let creds_json: serde_json::Value = serde_json::from_str(&creds).expect("parse auth.json");
        let creators = creds_json["creators"].as_object().expect("creators object");
        assert!(
            !creators.contains_key("crt_e2e_test_12345"),
            "no api_key credential should be stored after verify failure"
        );
    }
}

#[tokio::test]
async fn creator_register_twice_does_not_double_write_credentials() {
    let mock = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    seed_home(home.path(), &mock.uri());

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
        .expect("first run");
    assert!(output1.status.success(), "first register should succeed");

    let output2 = cli_cmd(home.path())
        .args(["creator", "register", "--name", "test-creator"])
        .output()
        .expect("second run");

    let _stdout2 = String::from_utf8_lossy(&output2.stdout);
    let _stderr2 = String::from_utf8_lossy(&output2.stderr);

    let creds_after_second = std::fs::read_to_string(home.path().join(".nexus42/auth.json"))
        .expect("read auth.json after second run");

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

    assert_eq!(
        api_key, "nexus_live_active_e2e_key",
        "api_key should retain its correct value after second run"
    );

    // other creator entries in the auth store).
    assert!(
        creators.contains_key("crt_seed_token"),
        "seed token entry must not be lost after second run"
    );
}

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

    let creds_after_first = std::fs::read_to_string(home.path().join(".nexus42/auth.json"))
        .expect("read auth.json after first run");
    let first_json: serde_json::Value =
        serde_json::from_str(&creds_after_first).expect("parse auth.json after first run");
    let original_key = first_json["creators"]["crt_e2e_test_12345"]["creator_api_key"]
        .as_str()
        .expect("creator_api_key after first run");
    assert_eq!(original_key, "nexus_live_active_e2e_key");

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

#[tokio::test]
async fn creator_register_verify_http_500_exits_with_error() {
    let mock = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    seed_home(home.path(), &mock.uri());

    Mock::given(method("POST"))
        .and(path("/api/v1/creators/register"))
        .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
            "fixtures/creator_register/register_response.json"
        )))
        .mount(&mock)
        .await;

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

    assert!(
        !output.status.success(),
        "register should fail when verify returns HTTP 500; stdout={stdout} stderr={stderr}"
    );

    assert!(
        combined.contains("500") || combined.contains("verification"),
        "error output should reference the 500 status or verification failure; got: {combined}"
    );
}

// The verify endpoint returns {"status": "expired"} indicating the
// challenge is no longer valid by the time the CLI submits the answer.
// The CLI must exit non-zero with an appropriate expiry-related message.

#[tokio::test]
async fn creator_register_verify_expired_challenge_exits_with_error() {
    let mock = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    seed_home(home.path(), &mock.uri());

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

    assert!(
        !output.status.success(),
        "register should fail when verify reports expired challenge; stdout={stdout} stderr={stderr}"
    );

    assert!(
        combined.contains("expired") || combined.contains("timed out"),
        "error output should mention expiry or timeout; got: {combined}"
    );

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

// WS-C requirement: positional syntax (old `creator register <name>` without --name)
// should produce a clear error message guiding users to the new flag syntax.

#[tokio::test]
async fn creator_register_positional_syntax_rejected_with_migration_hint() {
    let home = tempfile::tempdir().unwrap();

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

    // (without --name flag)
    let output = cli_cmd(home.path())
        .args(["creator", "register", "My Agent"])
        .output()
        .expect("run nexus42 creator register with positional arg");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        !output.status.success(),
        "register should fail when positional syntax is used; stdout={stdout} stderr={stderr}"
    );

    assert!(
        combined.contains("--name"),
        "error output should mention --name; got: {combined}"
    );
}
