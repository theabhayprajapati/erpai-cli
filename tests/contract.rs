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

#[test]
fn settings_works_without_a_profile() {
    let dir = tempfile::tempdir().unwrap();
    let a = erpai()
        .env("ERPAI_CONFIG_HOME", dir.path())
        .arg("settings")
        .assert()
        .success();
    let v: serde_json::Value = serde_json::from_slice(&a.get_output().stdout).unwrap();
    assert_eq!(v["data"]["activeProfile"], "default");
    assert!(v["data"]["active"].is_null());
}

#[test]
fn every_leaf_command_documents_output_shape() {
    let groups: &[(&str, &[&str])] = &[
        ("apps", &["list", "get"]),
        ("tables", &["list", "get", "create", "update", "delete"]),
        ("columns", &["list", "add", "update", "delete"]),
        (
            "records",
            &[
                "query",
                "count",
                "aggregate",
                "get",
                "get-many",
                "create",
                "bulk-create",
                "update",
                "bulk-update",
                "delete",
                "bulk-delete",
                "update-by-filter",
                "delete-by-filter",
            ],
        ),
        ("sql", &["schema", "run", "generate"]),
    ];
    for (g, cmds) in groups {
        for c in *cmds {
            let out = erpai().args([g, c, "--help"]).assert().success();
            let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
            assert!(
                text.contains("Output:"),
                "{g} {c} --help lacks an Output: line"
            );
        }
    }
}

#[test]
fn every_leaf_command_in_every_group_documents_output_shape() {
    let groups: &[(&[&str], &[&str])] = &[
        (
            &["workflows"],
            &[
                "list",
                "get",
                "create",
                "update",
                "patch-node",
                "rename",
                "delete",
                "activate",
                "deactivate",
                "execute",
                "test-node",
                "executions",
                "execution",
                "execution-stop",
                "execution-retry",
                "run-summary",
                "run-node",
            ],
        ),
        (&["workflows", "nodes"], &["list", "schema", "options"]),
        (
            &["workflows", "credentials"],
            &["list", "get", "create", "update", "delete", "test", "types"],
        ),
        (&["layouts"], &["list", "get", "create", "update", "delete"]),
        (
            &["forms"],
            &["get", "list", "set", "update", "patch", "delete"],
        ),
        (
            &["documents"],
            &["list", "get", "create", "update", "delete", "duplicate"],
        ),
        (
            &["documents", "folders"],
            &["list", "create", "update", "delete"],
        ),
        (
            &["roles"],
            &[
                "list",
                "create",
                "update",
                "delete",
                "users",
                "assign",
                "remove",
                "invite",
                "invites",
                "invite-cancel",
            ],
        ),
        (
            &["pages"],
            &[
                "list",
                "get",
                "hydrated",
                "home",
                "create",
                "create-home",
                "update",
                "delete",
                "reorder",
            ],
        ),
        (&["widgets"], &["get", "list", "set", "update", "delete"]),
        (
            &["catalog"],
            &[
                "list",
                "get",
                "source-publication",
                "source-draft",
                "draft",
                "preview",
                "publish",
                "unpublish",
                "activate",
                "install",
            ],
        ),
        (
            &["catalog", "publisher"],
            &[
                "overview",
                "terms",
                "accept-terms",
                "profile",
                "set-profile",
                "claim",
            ],
        ),
        (&["attachments"], &["upload", "download-url"]),
        (&["tables", "actions"], &["list", "create", "delete"]),
    ];
    for (prefix, cmds) in groups {
        for c in *cmds {
            let mut args: Vec<&str> = prefix.to_vec();
            args.push(c);
            args.push("--help");
            let out = erpai().args(&args).assert().success();
            let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
            assert!(
                text.contains("Output:"),
                "{} --help lacks an Output: line",
                args.join(" ")
            );
        }
    }
}
