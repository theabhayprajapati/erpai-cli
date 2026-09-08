mod common;
use common::*;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn apps_list_prints_list_envelope() {
    let s = server().await;
    Mock::given(method("GET"))
        .and(path("/v1/app-builder/app"))
        .and(query_param("pageNo", "1"))
        .and(query_param("pageSize", "30"))
        .and(query_param("sortCol", "updatedAt"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"data":[{"_id":"a1","name":"CRM"}],"totalCount":1}),
            ),
        )
        .expect(1)
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &[]);
    let a = erpai(d.path()).args(["apps", "list"]).assert().success();
    let v = stdout_json(&a);
    assert_eq!(v["data"][0]["name"], "CRM");
    assert_eq!(v["page"]["total"], 1);
}

#[tokio::test]
async fn apps_get_unwraps_body() {
    let s = server().await;
    Mock::given(method("GET"))
        .and(path("/v1/app-builder/app/a1"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"success":true,"body":{"_id":"a1","name":"CRM"}}),
            ),
        )
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &[]);
    let v = stdout_json(
        &erpai(d.path())
            .args(["apps", "get", "a1"])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["name"], "CRM");
}

#[tokio::test]
async fn page_size_over_500_is_validation_error() {
    let s = server().await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &[]);
    let a = erpai(d.path())
        .args(["apps", "list", "--page-size", "501"])
        .assert()
        .code(2);
    assert_eq!(stderr_json(&a)["error"]["code"], "validation_error");
}
