use std::{fs, process::Command};

#[test]
fn brief_without_history_is_one_quiet_status_and_creates_nothing() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("absent");
    let output = Command::new(env!("CARGO_BIN_EXE_regurgitate"))
        .args([
            "recall",
            "--brief",
            "--best-effort",
            "--limit",
            "2",
            "--query",
            "rust testing",
            "--data-home",
        ])
        .arg(&data)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        r#"{"status":"no_matches"}"#
    );
    assert!(output.stderr.is_empty());
    assert!(!data.exists());
}

#[test]
fn brief_best_effort_hides_backend_details_but_not_invalid_arguments() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("PRIVATE_BAD_DATA_HOME");
    fs::write(&data, "not a directory").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_regurgitate"))
        .args(["recall", "--brief", "--best-effort", "--data-home"])
        .arg(&data)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        r#"{"status":"unavailable"}"#
    );
    assert!(output.stderr.is_empty());
    let output = Command::new(env!("CARGO_BIN_EXE_regurgitate"))
        .args([
            "recall",
            "--brief",
            "--best-effort",
            "--token-budget",
            "1",
            "--data-home",
        ])
        .arg(&data)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read_to_string(data).unwrap(), "not a directory");
}
