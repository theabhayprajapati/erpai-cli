mod common;
use common::*;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn tables_require_app_and_preflight_blocks_without_request() {
    let s = server().await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let a = erpai(d.path()).args(["tables", "list"]).assert().code(2);
    assert!(stderr_json(&a)["error"]["hint"]
        .as_str()
        .unwrap()
        .contains("--app"));
    let a = erpai(d.path())
        .args(["tables", "list", "--app", "a2"])
        .assert()
        .code(4);
    assert_eq!(stderr_json(&a)["error"]["code"], "forbidden");
    assert_eq!(s.received_requests().await.unwrap().len(), 0);
}

#[tokio::test]
async fn tables_create_and_delete_gated() {
    let s = server().await;
    Mock::given(method("POST"))
        .and(path("/v1/app-builder/table"))
        .and(query_param("appId", "a1"))
        .and(body_json(
            serde_json::json!({"name":"Orders","objectType":"TABLE","category":"Sales"}),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"success":true,"body":{"_id":"t1","name":"Orders"}}),
            ),
        )
        .expect(1)
        .mount(&s)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/v1/app-builder/table/t1"))
        .and(query_param("appId", "a1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"success":true})))
        .expect(1)
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let v = stdout_json(
        &erpai(d.path())
            .args([
                "tables",
                "create",
                "--app",
                "a1",
                "--name",
                "Orders",
                "--category",
                "Sales",
            ])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["_id"], "t1");
    assert_eq!(v["context"]["app"], "a1");
    let a = erpai(d.path())
        .args(["tables", "delete", "--app", "a1", "t1"])
        .assert()
        .code(2);
    assert!(stderr_json(&a)["error"]["hint"]
        .as_str()
        .unwrap()
        .contains("--yes"));
    let v = stdout_json(
        &erpai(d.path())
            .args(["tables", "delete", "--app", "a1", "t1", "--dry-run"])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["dryRun"], true);
    assert_eq!(
        s.received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.method == "DELETE")
            .count(),
        0
    );
    erpai(d.path())
        .args(["tables", "delete", "--app", "a1", "t1", "--yes"])
        .assert()
        .success();
}

#[tokio::test]
async fn tables_actions_list_filters_auto_builder() {
    let s = server().await;
    Mock::given(method("GET"))
        .and(path("/v1/app-builder/table/t1"))
        .and(query_param("appId", "a1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "success":true,"body":{"_id":"t1","customActions":[
                {"id":"aB3x","type":"AUTO_BUILDER","title":"Convert"},
                {"id":"zZ9q","type":"URL","title":"Open"}]}
        })))
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let v = stdout_json(
        &erpai(d.path())
            .args(["tables", "actions", "list", "--app", "a1", "t1"])
            .assert()
            .success(),
    );
    assert_eq!(v["data"].as_array().unwrap().len(), 1);
    assert_eq!(v["data"][0]["id"], "aB3x");
}
