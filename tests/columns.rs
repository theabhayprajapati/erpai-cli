mod common;
use common::*;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn columns_list_projects_metadata() {
    let s = server().await;
    Mock::given(method("GET"))
        .and(path("/v1/app-builder/table/t1"))
        .and(query_param("appId", "a1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "success":true,"body":{"columnsMetaData":[
                {"id":"c1","name":"Name","type":"text","columnCode":"NAME","systemField":false,"required":true,"options":[]}]}
        })))
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let v = stdout_json(
        &erpai(d.path())
            .args(["columns", "list", "--app", "a1", "t1"])
            .assert()
            .success(),
    );
    assert_eq!(v["data"][0]["id"], "c1");
    assert_eq!(v["data"][0]["type"], "text");
}

#[tokio::test]
async fn columns_add_single_and_bulk() {
    let s = server().await;
    Mock::given(method("POST"))
        .and(path("/v1/app-builder/table/t1/column"))
        .and(body_json(serde_json::json!({"name":"Status","type":"select","options":[{"name":"Open","color":"blue"}]})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"success":true,"body":{"id":"c2"}})))
        .expect(1)
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/app-builder/table/t1/column/bulk"))
        .and(body_json(serde_json::json!({"columns":[{"name":"A","type":"text"},{"name":"B","type":"number"}]})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"success":true,"body":[{"id":"c3"},{"id":"c4"}]})))
        .expect(1)
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let v = stdout_json(
        &erpai(d.path())
            .args([
                "columns",
                "add",
                "--app",
                "a1",
                "t1",
                "--body",
                r#"{"name":"Status","type":"select","options":[{"name":"Open","color":"blue"}]}"#,
            ])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["id"], "c2");
    let v = stdout_json(
        &erpai(d.path())
            .args([
                "columns",
                "add",
                "--app",
                "a1",
                "t1",
                "--body",
                r#"[{"name":"A","type":"text"},{"name":"B","type":"number"}]"#,
            ])
            .assert()
            .success(),
    );
    assert_eq!(v["data"][1]["id"], "c4");
}

#[tokio::test]
async fn columns_delete_is_gated() {
    let s = server().await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    erpai(d.path())
        .args(["columns", "delete", "--app", "a1", "t1", "c1"])
        .assert()
        .code(2);
    assert_eq!(s.received_requests().await.unwrap().len(), 0);
}
