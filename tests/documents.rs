mod common;
use common::*;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn documents_create_defaults_and_folders_delete_gated() {
    let s = server().await;
    Mock::given(method("POST"))
        .and(path("/v1/app-builder/app-document"))
        .and(query_param("appId", "a1"))
        .and(body_json(
            serde_json::json!({"name":"Notes","content":[],"isDraft":false,"appId":"a1"}),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"success":true,"body":{"_id":"d1"}})),
        )
        .expect(1)
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let v = stdout_json(
        &erpai(d.path())
            .args(["documents", "create", "--app", "a1", "--name", "Notes"])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["_id"], "d1");
    let bad = d.path().join("c.json");
    std::fs::write(&bad, "{}").unwrap();
    let a = erpai(d.path())
        .args([
            "documents",
            "create",
            "--app",
            "a1",
            "--name",
            "N",
            "--content-file",
            bad.to_str().unwrap(),
        ])
        .assert()
        .code(2);
    assert_eq!(stderr_json(&a)["error"]["code"], "validation_error");
    erpai(d.path())
        .args(["documents", "folders", "delete", "--app", "a1", "f1"])
        .assert()
        .code(2);
    erpai(d.path())
        .args(["documents", "delete", "--app", "a1", "d1"])
        .assert()
        .code(2);
    assert_eq!(s.received_requests().await.unwrap().len(), 1);
}
