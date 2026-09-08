use assert_cmd::Command;
use predicates::prelude::*;

fn erpai() -> Command {
    Command::cargo_bin("erpai").unwrap()
}

#[test]
fn version_is_text_on_stdout() {
    erpai()
        .arg("--version")
        .assert()
        .success()
        .stdout("erpai 0.2.0\n");
}

#[test]
fn help_is_text_on_stdout() {
    erpai()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage:"));
}

#[test]
fn unknown_command_is_validation_error_json_on_stderr() {
    let a = erpai().arg("nope").assert().code(2);
    let err = String::from_utf8(a.get_output().stderr.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(err.trim()).expect("stderr must be JSON");
    assert_eq!(v["error"]["code"], "validation_error");
    assert!(a.get_output().stdout.is_empty());
}

#[test]
fn missing_profile_is_auth_error_3() {
    let dir = tempfile::tempdir().unwrap();
    let a = erpai()
        .env("ERPAI_CONFIG_HOME", dir.path())
        .args(["apps", "list"])
        .assert()
        .code(3);
    let err = String::from_utf8(a.get_output().stderr.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(err.trim()).unwrap();
    assert_eq!(v["error"]["code"], "auth_error");
    assert!(v["error"]["hint"].as_str().unwrap().contains("erpai login"));
}
