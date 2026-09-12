use std::process::Command;

#[test]
fn metrics_explain_scope_without_creating_a_notebook() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("absent");
    for shared in [false, true] {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_regurgitate"));
        cmd.args(["experience", "metrics", "--brief", "--data-home"])
            .arg(&data);
        if shared {
            cmd.arg("--shared");
        }
        let result = cmd.output().unwrap();
        assert!(result.status.success());
        let text = String::from_utf8(result.stdout).unwrap();
        assert!(text.contains("No saved lessons in this scope yet"));
        assert!(text.contains(if shared {
            "Machine-shared"
        } else {
            "this project"
        }));
        assert!(result.stderr.is_empty());
        assert!(!data.exists());
    }
}
