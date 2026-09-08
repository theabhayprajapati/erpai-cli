mod common;
use common::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn update_check_reports_newer_release() {
    let s = server().await;
    // the repo carries plugin releases too — only cli-v* tags count, newest wins
    Mock::given(method("GET")).and(path("/repos/erphq/agent-plugins/releases"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"tag_name":"plugin-v9.9.9","html_url":"https://github.com/erphq/agent-plugins/releases/tag/plugin-v9.9.9","body":"plugin"},
            {"tag_name":"cli-v0.3.0","html_url":"https://github.com/erphq/agent-plugins/releases/tag/cli-v0.3.0","body":"## What's new\n- things"},
            {"tag_name":"cli-v0.1.9","html_url":"https://github.com/erphq/agent-plugins/releases/tag/cli-v0.1.9","body":"old"}
        ])))
        .mount(&s).await;
    let d = tempfile::tempdir().unwrap();
    let v = stdout_json(
        &erpai(d.path())
            .env("ERPAI_UPDATE_API", s.uri())
            .args(["update", "--check"])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["updateAvailable"], true);
    assert_eq!(v["data"]["latest"], "0.3.0");
    assert_eq!(v["data"]["current"], "0.2.0");
    assert!(v["data"]["releaseUrl"]
        .as_str()
        .unwrap()
        .ends_with("cli-v0.3.0"));
}

#[tokio::test]
async fn update_check_network_failure_is_exit_5() {
    let d = tempfile::tempdir().unwrap();
    let a = erpai(d.path())
        .env("ERPAI_UPDATE_API", "http://127.0.0.1:9")
        .args(["update", "--check"])
        .assert()
        .code(5);
    assert_eq!(stderr_json(&a)["error"]["code"], "network_error");
}
