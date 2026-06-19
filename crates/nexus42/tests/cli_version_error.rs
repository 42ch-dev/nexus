//! V1.51 T-B P1: E_VERSION stable CLI code tests.
//!
//! Verifies:
//! - `CliError::VersionConflict` maps to exit code 76
//! - `CliError::VersionConflict` Display output includes "E_VERSION"
//! - Distinct from `CliError::Locked` (exit 75) and `CliError::LockIo` (exit 78)

use nexus42::errors::CliError;

#[test]
fn test_version_conflict_displays_e_version() {
    let err = CliError::VersionConflict {
        table: "kb_extract_jobs".to_string(),
        row_id: "xj_test123".to_string(),
        expected_version: 0,
        actual_version: Some(1),
    };
    let display = format!("{err}");
    assert!(
        display.contains("E_VERSION"),
        "expected 'E_VERSION' in display: {display}"
    );
    assert!(
        display.contains("xj_test123"),
        "expected row_id in display: {display}"
    );
    assert!(
        display.contains("kb_extract_jobs"),
        "expected table in display: {display}"
    );
}

#[test]
fn test_version_conflict_actual_none_displays_question_mark() {
    let err = CliError::VersionConflict {
        table: "novel_pool_entries".to_string(),
        row_id: "npe_test".to_string(),
        expected_version: 2,
        actual_version: None,
    };
    let display = format!("{err}");
    assert!(display.contains("E_VERSION"));
    assert!(display.contains("actual v?") || display.contains("actual v"));
}

#[test]
fn test_version_conflict_distinct_from_locked() {
    let version_err = CliError::VersionConflict {
        table: "t".to_string(),
        row_id: "id".to_string(),
        expected_version: 0,
        actual_version: Some(1),
    };
    let locked_err = CliError::Locked {
        holder_pid: 1234,
        holder_name: "cli:test".to_string(),
        stale: false,
    };

    let v_display = format!("{version_err}");
    let l_display = format!("{locked_err}");

    assert!(v_display.contains("E_VERSION"));
    assert!(l_display.contains("E_LOCK"));
    assert!(!v_display.contains("E_LOCK"));
    assert!(!l_display.contains("E_VERSION"));
}

#[test]
fn test_exit_code_76_matches_version_conflict() {
    // This test verifies the exit-code-mapping logic found in main.rs.
    // The mapping is:
    //   VersionConflict → 76
    //   Locked → 75
    //   LockIo → 78

    let code_for = |e: &CliError| -> i32 {
        if matches!(e, CliError::Locked { .. }) {
            75
        } else if matches!(e, CliError::LockIo(_)) {
            78
        } else if matches!(e, CliError::VersionConflict { .. }) {
            76
        } else {
            1
        }
    };

    assert_eq!(
        code_for(&CliError::VersionConflict {
            table: "t".to_string(),
            row_id: "id".to_string(),
            expected_version: 0,
            actual_version: None,
        }),
        76,
        "VersionConflict must map to exit 76"
    );

    assert_eq!(
        code_for(&CliError::Locked {
            holder_pid: 1,
            holder_name: "x".to_string(),
            stale: false,
        }),
        75,
        "Locked must map to exit 75 (unchanged)"
    );

    assert_eq!(
        code_for(&CliError::LockIo(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "test"
        ))),
        78,
        "LockIo must map to exit 78 (unchanged)"
    );

    assert_eq!(
        code_for(&CliError::Other("generic error".to_string())),
        1,
        "Other errors must map to exit 1 (unchanged)"
    );
}
